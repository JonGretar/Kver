//! Canonical-path session membership, shared baseline loading, and lifetime.
//! The registry is weak: only participating readers own sessions. Background
//! consumers retain an immutable baseline, never a session or a reader.

#[cfg(test)]
mod current_save_tests;
mod history;
mod save_file;
#[cfg(test)]
mod save_tests;
pub(crate) use save_file::{Destination, SaveError};

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use gpui_kit::{App, AppContext as _, Context, Entity, EntityId, Global, Task, WeakEntity};

use crate::pdf::{
    self,
    annotations::{Annotation, AnnotationId, HighlightColor, MarkupKind, Quad},
};

static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct Sessions {
    open: HashMap<PathBuf, WeakEntity<DocumentSession>>,
    saving: HashMap<PathBuf, WeakEntity<DocumentSession>>,
}

impl Global for Sessions {}

pub(crate) struct DocumentSession {
    path: PathBuf,
    result: Option<Result<Arc<pdf::Document>, String>>,
    epoch: u64,
    revision: u64,
    annotations: Option<Result<Arc<Vec<Annotation>>, String>>,
    // Identity/provenance stays relative to this source for the session lifetime.
    // Saved bytes remain separate for conflict detection and discard baselines.
    editing_source: Option<Arc<pdf::Document>>,
    original_annotations: Arc<Vec<Annotation>>,
    saved_annotations: Arc<Vec<Annotation>>,
    preview: Option<Arc<pdf::Document>>,
    preview_work: Option<Task<()>>,
    preview_error: Option<String>,
    markup_colors: [HighlightColor; 4],
    annotation_load: Option<Task<()>>,
    editing_rejection: Option<String>,
    next_serial: u64,
    history: history::AnnotationHistory,
    dirty: bool,
    saved_serial: u64,
    save: Option<Task<()>>,
    save_ticket: u64,
    save_outcome: Option<(u64, Result<(), SaveError>)>,
    participants: HashSet<EntityId>,
    _load: Task<()>,
}

impl DocumentSession {
    /// `path` has already been canonicalized on a background worker. Membership
    /// is resolved on the UI thread before starting I/O, including concurrent opens.
    /// A failed load has no usable baseline and must not prevent a later retry.
    pub(super) fn join(path: PathBuf, cx: &mut App) -> Entity<Self> {
        if !cx.has_global::<Sessions>() {
            cx.set_global(Sessions::default());
        }
        if cx
            .global::<Sessions>()
            .saving
            .get(&path)
            .and_then(WeakEntity::upgrade)
            .is_some()
            && cx
                .global::<Sessions>()
                .open
                .get(&path)
                .and_then(WeakEntity::upgrade)
                .is_none()
        {
            return cx.new(|cx| {
                let mut session = Self::new(path, cx);
                session._load = Task::ready(());
                session.result = Some(Err(
                    "This destination is being saved. Open it after saving finishes.".into(),
                ));
                session
            });
        }
        if let Some(session) = cx
            .global::<Sessions>()
            .open
            .get(&path)
            .and_then(WeakEntity::upgrade)
            && !session.read(cx).result.as_ref().is_some_and(Result::is_err)
        {
            return session;
        }
        let load_path = path.clone();
        let session = cx.new(|cx| Self::new(load_path, cx));
        let registry = &mut cx.global_mut::<Sessions>().open;
        registry.retain(|_, session| session.upgrade().is_some());
        registry.insert(path, session.downgrade());
        session
    }

    fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let load_path = path.clone();
        let work = cx.background_executor().spawn(async move {
            let document = pdf::Document::open(&load_path)?;
            if document.pages().is_empty() {
                return Err("PDF has no pages".into());
            }
            Ok(Arc::new(document))
        });
        Self {
            path,
            result: None,
            epoch: NEXT_EPOCH.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            annotations: None,
            editing_source: None,
            original_annotations: Arc::new(Vec::new()),
            saved_annotations: Arc::new(Vec::new()),
            preview: None,
            preview_work: None,
            preview_error: None,
            markup_colors: [
                HighlightColor::Yellow,
                HighlightColor::Red,
                HighlightColor::Red,
                HighlightColor::Red,
            ],
            annotation_load: None,
            editing_rejection: None,
            next_serial: 1,
            history: history::AnnotationHistory::default(),
            dirty: false,
            saved_serial: 0,
            save: None,
            save_ticket: 0,
            save_outcome: None,
            participants: HashSet::new(),
            _load: cx.spawn(async move |this, cx| {
                let result = work.await;
                let _ = this.update(cx, |this, cx| {
                    this.result = Some(result);
                    cx.notify();
                });
            }),
        }
    }

    pub(super) fn result(&self) -> Option<Result<Arc<pdf::Document>, String>> {
        self.result.clone()
    }

    pub(super) fn presentation(&self) -> Option<Arc<pdf::Document>> {
        self.preview
            .clone()
            .or_else(|| self.result().and_then(Result::ok))
    }

    pub(super) fn preview_error(&self) -> Option<&str> {
        self.preview_error.as_deref()
    }

    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn annotations(&self) -> Option<Result<Arc<Vec<Annotation>>, String>> {
        self.annotations.clone()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(super) fn attach(&mut self, participant: EntityId) {
        self.participants.insert(participant);
    }

    pub(super) fn detach(&mut self, participant: EntityId) {
        self.participants.remove(&participant);
    }

    pub(crate) fn participant_count(&self) -> usize {
        self.participants.len()
    }

    pub(crate) fn discard(&mut self, cx: &mut Context<Self>) {
        if self.is_saving() || !self.is_dirty() {
            return;
        }
        self.annotations = Some(Ok(self.saved_annotations.clone()));
        self.history = history::AnnotationHistory::default();
        self.annotations_changed(cx);
    }

    pub(crate) fn editing_rejection(&self) -> Option<&str> {
        self.editing_rejection.as_deref()
    }

    pub(crate) fn existing_markup(
        &self,
        kind: MarkupKind,
        page: usize,
        quads: &[Quad],
    ) -> Vec<Annotation> {
        let Some(Ok(annotations)) = &self.annotations else {
            return Vec::new();
        };
        let start = annotations.partition_point(|a| a.page() < page);
        let end = annotations.partition_point(|a| a.page() <= page);
        annotations[start..end]
            .iter()
            .filter(|annotation| {
                annotation.kind() == kind && identical_region(annotation.quads(), quads)
            })
            .cloned()
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn create_highlight(
        &mut self,
        page: usize,
        quads: Vec<Quad>,
        cx: &mut Context<Self>,
    ) -> Result<AnnotationId, String> {
        self.create_markup(MarkupKind::Highlight, page, quads, cx)
    }

    pub(crate) fn create_markup(
        &mut self,
        kind: MarkupKind,
        page: usize,
        quads: Vec<Quad>,
        cx: &mut Context<Self>,
    ) -> Result<AnnotationId, String> {
        if let Some(reason) = &self.editing_rejection {
            return Err(reason.clone());
        }
        if quads.is_empty() {
            return Err("Select text before creating markup.".into());
        }
        let document = self
            .result()
            .and_then(Result::ok)
            .ok_or("Document is not ready.")?;
        if document.page_transform(page).is_none() {
            return Err("Cannot annotate a missing PDF page.".into());
        }
        match self.existing_markup(kind, page, &quads).as_slice() {
            [existing] => return Ok(existing.id()),
            [] => {}
            _ => return Err("Choose one of the existing annotations before editing it.".into()),
        }
        let Some(Ok(annotations)) = &mut self.annotations else {
            return Err(
                "Document annotations are not ready. Try again when loading finishes.".into(),
            );
        };
        let page_start = annotations.partition_point(|annotation| annotation.page() < page);
        let page_end = annotations.partition_point(|annotation| annotation.page() <= page);
        let id = AnnotationId::new(self.epoch, (1 << 63) | self.next_serial);
        self.next_serial += 1;
        // Capture identity once, after duplicate detection. Later settings
        // changes and edits must not claim existing or imported annotations.
        let settings = crate::settings_ui::state(cx);
        let author = settings.read(cx).annotation_author();
        let author = (!author.trim().is_empty()).then(|| author.to_owned());
        let annotation = Annotation::new(
            id,
            page,
            kind,
            quads,
            self.markup_colors[kind.color_slot()].color(),
        )
        .with_opacity(if kind == MarkupKind::Highlight {
            0.45
        } else {
            1.
        })
        .with_metadata(author, None, None);
        let key = |annotation: &Annotation| {
            document
                .annotation_bounds(annotation)
                .map(|bounds| (bounds[1], bounds[0]))
                .unwrap_or((f64::INFINITY, f64::INFINITY))
        };
        let (top, left) = key(&annotation);
        // The catalog is already sorted. Only compare within this page instead
        // of re-sorting every annotation in a large document for each creation.
        let insertion = page_start
            + annotations[page_start..page_end].partition_point(|existing| {
                let (y, x) = key(existing);
                y.total_cmp(&top)
                    .then(x.total_cmp(&left))
                    .then(existing.id().serial().cmp(&id.serial()))
                    .is_le()
            });
        Arc::make_mut(annotations).insert(insertion, annotation.clone());
        self.record_edit(insertion, None, Some(annotation), cx);
        Ok(id)
    }

    pub(crate) fn edit_annotation(
        &mut self,
        id: AnnotationId,
        color: Option<HighlightColor>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(reason) = &self.editing_rejection {
            return Err(reason.clone());
        }
        let Some(Ok(annotations)) = &mut self.annotations else {
            return Err("Document annotations are not ready.".into());
        };
        let ix = annotations
            .iter()
            .position(|a| a.id() == id)
            .ok_or("This annotation is no longer available.")?;
        let annotation = annotations[ix].clone();
        if id.epoch() != self.epoch || annotation.is_readonly() {
            return Err("This annotation cannot be edited.".into());
        }
        if let Some(color) = color {
            self.markup_colors[annotation.kind().color_slot()] = color;
            if annotation.color() == color.color() {
                return Ok(());
            }
            let changed = annotation.clone().with_color(color.color());
            Arc::make_mut(annotations)[ix] = changed;
        } else {
            Arc::make_mut(annotations).remove(ix);
        }
        let after = color.map(|color| annotation.clone().with_color(color.color()));
        self.record_edit(ix, Some(annotation), after, cx);
        Ok(())
    }

    /// Render the actual edited PDF, not an overlay over obsolete embedded APs.
    /// No disk writes or rasterization occur here. Rapid edits supersede old work.
    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.editing_source.clone() else {
            return;
        };
        let Some(Ok(current)) = self.annotations.clone() else {
            return;
        };
        let original = self.original_annotations.clone();
        let revision = self.revision;
        let epoch = self.epoch;
        let path = self.path.clone();
        self.preview_error = None;
        let work = cx.background_executor().spawn(async move {
            let bytes = source.with_annotation_edits(&original, &current)?;
            pdf::Document::from_bytes(path, bytes).map(Arc::new)
        });
        self.preview_work = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.epoch != epoch || this.revision != revision {
                    return;
                }
                match result {
                    Ok(document) => this.preview = Some(document),
                    Err(error) => this.preview_error = Some(error),
                }
                this.preview_work = None;
                cx.notify();
            });
        }));
    }

    pub(crate) fn is_saving(&self) -> bool {
        self.save.is_some()
    }

    pub(crate) fn save_outcome(&self) -> Option<(u64, Result<(), SaveError>)> {
        self.save_outcome.clone()
    }

    pub(crate) fn active_save_ticket(&self) -> Option<u64> {
        self.save.as_ref().map(|_| self.save_ticket)
    }

    /// Diagnostic: additions newer than the saved snapshot. The PDF preview
    /// renders their real appearance; these are never also painted as overlays.
    #[cfg(test)]
    pub(crate) fn unsaved_annotations(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations
            .as_ref()
            .and_then(|catalog| catalog.as_ref().ok())
            .into_iter()
            .flat_map(|annotations| annotations.iter())
            .filter(|annotation| {
                annotation.id().serial() & (1 << 63) != 0
                    && (annotation.id().serial() & !(1 << 63)) > self.saved_serial
            })
    }

    pub(crate) fn start_save_as(
        &mut self,
        destination: Arc<Destination>,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        if destination.path() == self.path {
            return Err(
                "Use Save to update the current PDF; Save As needs a different filename.".into(),
            );
        }
        self.start_write(Some(destination), cx)
    }

    pub(crate) fn start_save(&mut self, cx: &mut Context<Self>) -> Result<u64, String> {
        self.start_write(None, cx)
    }

    fn start_write(
        &mut self,
        destination: Option<Arc<Destination>>,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        if self.is_saving() {
            return Err("This document is already being saved.".into());
        }
        if let Some(reason) = self.editing_rejection() {
            return Err(reason.to_owned());
        }
        let document = self
            .result()
            .and_then(Result::ok)
            .ok_or("Document is not ready.")?;
        if self.annotations.as_ref().is_none_or(Result::is_err) {
            return Err("Document annotations are not ready.".into());
        }
        let path = destination.as_ref().map_or_else(
            || self.path.clone(),
            |destination| destination.path().to_path_buf(),
        );
        let sessions = cx.global::<Sessions>();
        if sessions
            .open
            .get(&path)
            .and_then(WeakEntity::upgrade)
            .is_some_and(|session| session.entity_id() != cx.entity_id())
            || sessions
                .saving
                .get(&path)
                .and_then(WeakEntity::upgrade)
                .is_some()
        {
            return Err("This destination is open in another Kver session. Close it or choose another filename.".into());
        }
        let snapshot = self
            .annotations()
            .and_then(Result::ok)
            .ok_or("Document annotations are not ready.")?;
        let saved_snapshot = snapshot.clone();
        let original = self.original_annotations.clone();
        let editing_source = self
            .editing_source
            .clone()
            .ok_or("Document is not ready.")?;
        let revision = self.revision;
        let serial = self.next_serial - 1;
        let source = self.path.clone();
        let epoch = self.epoch;
        self.save_ticket += 1;
        let ticket = self.save_ticket;
        self.save_outcome = None;
        crate::app::quit_guard::invalidate_approval();
        let owner = cx.weak_entity();
        cx.global_mut::<Sessions>()
            .saving
            .insert(path.clone(), owner);
        let work = cx.background_executor().spawn(async move {
            let destination = match destination {
                Some(destination) => {
                    if destination.is_source(&source) {
                        return Err(SaveError::Failed(
                            "Save As must not replace the source file or one of its aliases."
                                .into(),
                        ));
                    }
                    destination
                }
                None => Arc::new(Destination::for_current(
                    &source,
                    document.bytes(),
                    document
                        .file_metadata()
                        .ok_or("The loaded file version is unavailable. Use Save As.")?,
                )?),
            };
            let bytes = editing_source.with_annotation_edits(&original, &snapshot)?;
            // Validate/parse the exact intended output before committing it.
            let saved = pdf::Document::from_bytes(path, bytes)?;
            let metadata = destination.commit(saved.bytes())?;
            Ok::<_, SaveError>(Arc::new(saved.with_file_metadata(metadata)))
        });
        self.save = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.epoch != epoch || this.save_ticket != ticket {
                    return;
                }
                let outcome = result.as_ref().map(|_| ()).map_err(Clone::clone);
                let owner = cx.weak_entity();
                if let Ok(document) = result {
                    let destination = document.path().to_path_buf();
                    let registry = &mut cx.global_mut::<Sessions>().open;
                    registry.remove(&this.path);
                    registry.insert(destination.clone(), owner.clone());
                    this.path = destination;
                    if this.revision == revision {
                        this.preview = Some(document.clone());
                        this.preview_work = None;
                    }
                    this.result = Some(Ok(document));
                    this.saved_annotations = saved_snapshot;
                    this.update_dirty();
                    this.saved_serial = serial;
                    if this.revision != revision {
                        // Save As changes relative-link resolution for every
                        // participant, including newer in-flight preview work.
                        this.refresh_preview(cx);
                    }
                }
                cx.global_mut::<Sessions>()
                    .saving
                    .retain(|_, candidate| candidate != &owner);
                this.save = None;
                this.save_outcome = Some((ticket, outcome));
                cx.notify();
            });
        }));
        cx.notify();
        Ok(ticket)
    }

    /// The session owns one metadata catalog for both imported and authored
    /// annotations. Opening a panel never extracts text or rasterizes pages.
    pub(crate) fn ensure_annotations(&mut self, cx: &mut Context<Self>) {
        if self.annotations.is_some() || self.annotation_load.is_some() {
            return;
        }
        let Some(Ok(document)) = self.result() else {
            return;
        };
        let epoch = self.epoch;
        self.editing_source = Some(document.clone());
        let work = cx.background_executor().spawn(async move {
            let rejection = document.editing_rejection();
            let mut annotations = Vec::new();
            for page in 0..document.pages().len() {
                match document.page_annotations(page, epoch) {
                    Ok(page) => annotations.extend(page),
                    Err(error) => return (Err(error), rejection),
                }
            }
            (Ok(Arc::new(annotations)), rejection)
        });
        self.annotation_load = Some(cx.spawn(async move |this, cx| {
            let (result, rejection) = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.epoch != epoch {
                    return;
                }
                if let Ok(annotations) = &result {
                    this.original_annotations = annotations.clone();
                    this.saved_annotations = annotations.clone();
                }
                this.annotations = Some(result);
                this.editing_rejection = rejection;
                this.annotation_load = None;
                cx.notify();
            });
        }));
    }
}

/// Treat vertex and region ordering as representation, not region identity.
/// The tolerance only absorbs PDF real-number encoding; it does not merge nearby
/// or partially overlapping selections. Imported geometry is never rewritten.
fn identical_region(left: &[Quad], right: &[Quad]) -> bool {
    let canonical = |quads: &[Quad]| {
        let mut quads: Vec<_> = quads
            .iter()
            .map(|quad| {
                let mut points = quad.points();
                points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
                points
            })
            .collect();
        quads.sort_by(|a, b| {
            a.iter()
                .zip(b)
                .find_map(|(a, b)| {
                    let order = a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y));
                    (order != std::cmp::Ordering::Equal).then_some(order)
                })
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        quads
    };
    left.len() == right.len()
        && canonical(left)
            .iter()
            .zip(canonical(right))
            .all(|(left, right)| {
                left.iter().zip(right).all(|(left, right)| {
                    (left.x - right.x).abs() <= 0.001 && (left.y - right.y).abs() <= 0.001
                })
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinates::Point;
    use crate::pdf::annotations::AnnotationColor;
    use gpui_kit::TestAppContext;

    fn quad(left: f64, top: f64, right: f64, bottom: f64) -> Quad {
        Quad::new([
            Point { x: left, y: top },
            Point { x: right, y: top },
            Point { x: left, y: bottom },
            Point {
                x: right,
                y: bottom,
            },
        ])
        .unwrap()
    }

    #[test]
    fn duplicate_region_ignores_vertex_order_but_not_partial_overlap() {
        let first = quad(10., 30., 80., 20.);
        let second = quad(10., 15., 40., 5.);
        let [ul, ur, ll, lr] = first.points();
        let perimeter = Quad::new([ul, ur, lr, ll]).unwrap();
        assert!(identical_region(&[first, second], &[second, perimeter]));
        assert!(!identical_region(&[first], &[quad(11., 30., 80., 20.)]));
        assert!(!identical_region(&[first], &[first, second]));
    }

    #[gpui_kit::test]
    fn repeated_open_shares_one_catalog_and_duplicate_creation_is_not_an_edit(
        cx: &mut TestAppContext,
    ) {
        let path = std::fs::canonicalize("examples/field-guide.pdf").unwrap();
        let (first, second) = cx.update(|cx| {
            let first = DocumentSession::join(path.clone(), cx);
            let second = DocumentSession::join(path, cx);
            assert_eq!(first, second);
            (first, second)
        });
        cx.run_until_parked();
        first.update(cx, |session, cx| session.ensure_annotations(cx));
        cx.run_until_parked();
        let region = vec![quad(48., 734., 170., 720.), quad(48., 714., 145., 700.)];
        let id = first.update(cx, |session, cx| {
            assert!(!session.is_dirty());
            session.create_highlight(0, region.clone(), cx).unwrap()
        });
        let revision = first.read_with(cx, |session, _| session.revision());
        second.update(cx, |session, cx| {
            assert!(session.is_dirty());
            assert_eq!(session.create_highlight(0, region, cx).unwrap(), id);
            assert_eq!(session.revision(), revision);
            let catalog = session.annotations().unwrap().unwrap();
            assert_eq!(
                catalog
                    .iter()
                    .filter(|annotation| annotation.id() == id)
                    .count(),
                1
            );
            assert_eq!(
                catalog
                    .iter()
                    .find(|annotation| annotation.id() == id)
                    .unwrap()
                    .color(),
                AnnotationColor::Rgb([1., 0.85, 0.])
            );
        });
        drop(first);
        second.read_with(cx, |session, _| {
            assert!(
                session.is_dirty(),
                "closing another participant must retain edits"
            );
        });
    }

    #[gpui_kit::test]
    fn discard_restores_recolored_and_deleted_annotations_and_rejects_foreign_epoch(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("first.pdf");
        let other = directory.path().join("other.pdf");
        for path in [&source, &other] {
            std::fs::copy("examples/annotations.pdf", path).unwrap();
        }
        let (first, other) = cx.update(|cx| {
            (
                DocumentSession::join(source, cx),
                DocumentSession::join(other, cx),
            )
        });
        cx.run_until_parked();
        for session in [&first, &other] {
            session.update(cx, |session, cx| session.ensure_annotations(cx));
        }
        cx.run_until_parked();
        let original = first.read_with(cx, |session, _| session.annotations().unwrap().unwrap());
        let target = original
            .iter()
            .find(|a| a.name() == Some("imported-highlight"))
            .unwrap()
            .id();
        first.update(cx, |session, cx| {
            session
                .edit_annotation(target, Some(HighlightColor::Green), cx)
                .unwrap();
            session.edit_annotation(target, None, cx).unwrap();
            assert!(session.is_dirty());
            session.discard(cx);
            assert!(!session.is_dirty());
            assert_eq!(session.annotations().unwrap().unwrap(), original);
        });
        other.update(cx, |session, cx| {
            assert!(session.edit_annotation(target, None, cx).is_err());
            assert!(!session.is_dirty());
            let id = session
                .annotations()
                .unwrap()
                .unwrap()
                .iter()
                .find(|a| a.is_readonly())
                .unwrap()
                .id();
            assert!(
                session
                    .edit_annotation(id, Some(HighlightColor::Blue), cx)
                    .is_err()
            );
            assert!(!session.is_dirty());
        });
        cx.run_until_parked();
        first.read_with(cx, |session, _| {
            let preview = session.presentation().unwrap();
            let readback = preview.page_annotations(0, session.epoch()).unwrap();
            assert_eq!(
                readback.iter().find(|a| a.id() == target).unwrap().color(),
                original.iter().find(|a| a.id() == target).unwrap().color()
            );
        });
    }
}
