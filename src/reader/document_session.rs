//! Canonical-path session membership, shared baseline loading, and lifetime.
//! The registry is weak: only participating readers own sessions. Background
//! consumers retain an immutable baseline, never a session or a reader.

#[cfg(test)]
mod current_save_tests;
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
    annotations::{Annotation, AnnotationColor, AnnotationId, MarkupKind, Quad},
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
    annotation_load: Option<Task<()>>,
    editing_rejection: Option<String>,
    next_serial: u64,
    saved_revision: u64,
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
            annotation_load: None,
            editing_rejection: None,
            next_serial: 1,
            saved_revision: 0,
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
        self.revision != self.saved_revision
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
        if let Some(Ok(annotations)) = &mut self.annotations {
            Arc::make_mut(annotations).retain(|annotation| {
                annotation.id().serial() & (1 << 63) == 0
                    || (annotation.id().serial() & !(1 << 63)) <= self.saved_serial
            });
        }
        self.revision += 1;
        self.saved_revision = self.revision;
        cx.notify();
    }

    pub(crate) fn editing_rejection(&self) -> Option<&str> {
        self.editing_rejection.as_deref()
    }

    pub(crate) fn create_highlight(
        &mut self,
        page: usize,
        quads: Vec<Quad>,
        cx: &mut Context<Self>,
    ) -> Result<AnnotationId, String> {
        if let Some(reason) = &self.editing_rejection {
            return Err(reason.clone());
        }
        if quads.is_empty() {
            return Err("Select text before highlighting.".into());
        }
        let document = self
            .result()
            .and_then(Result::ok)
            .ok_or("Document is not ready.")?;
        if document.page_transform(page).is_none() {
            return Err("Cannot highlight a missing PDF page.".into());
        }
        let Some(Ok(annotations)) = &mut self.annotations else {
            return Err(
                "Document annotations are not ready. Try again when loading finishes.".into(),
            );
        };
        let page_start = annotations.partition_point(|annotation| annotation.page() < page);
        let page_end = annotations.partition_point(|annotation| annotation.page() <= page);
        if let Some(existing) = annotations[page_start..page_end].iter().find(|annotation| {
            annotation.kind() == MarkupKind::Highlight
                && identical_region(annotation.quads(), &quads)
        }) {
            return Ok(existing.id());
        }
        let id = AnnotationId::new(self.epoch, (1 << 63) | self.next_serial);
        self.next_serial += 1;
        let annotation = Annotation::new(
            id,
            page,
            MarkupKind::Highlight,
            quads,
            AnnotationColor::Rgb([1., 0.85, 0.]),
        )
        .with_opacity(0.45);
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
        Arc::make_mut(annotations).insert(insertion, annotation);
        self.revision += 1;
        crate::app::quit_guard::invalidate_approval();
        cx.notify();
        Ok(id)
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

    /// Only additions not included in the current immutable rendering baseline
    /// need an application overlay. Saved/imported appearances are painted once
    /// by Hayro instead.
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
        let snapshot: Vec<_> = self.unsaved_annotations().cloned().collect();
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
            let bytes = document.with_highlights(&snapshot)?;
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
                    this.result = Some(Ok(document));
                    this.saved_revision = revision;
                    this.saved_serial = serial;
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
}
