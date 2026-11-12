//! Canonical-path session membership, shared baseline loading, and lifetime.
//! The registry is weak: only participating readers own sessions. Background
//! consumers retain an immutable baseline, never a session or a reader.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Task, WeakEntity};

use crate::pdf::{self, annotations::Annotation};

static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct Sessions(HashMap<PathBuf, WeakEntity<DocumentSession>>);

impl Global for Sessions {}

pub(crate) struct DocumentSession {
    path: PathBuf,
    result: Option<Result<Arc<pdf::Document>, String>>,
    epoch: u64,
    revision: u64,
    annotations: Option<Result<Arc<Vec<Annotation>>, String>>,
    annotation_load: Option<Task<()>>,
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
        if let Some(session) = cx
            .global::<Sessions>()
            .0
            .get(&path)
            .and_then(WeakEntity::upgrade)
            && !session.read(cx).result.as_ref().is_some_and(Result::is_err)
        {
            return session;
        }
        let load_path = path.clone();
        let session = cx.new(|cx| Self::new(load_path, cx));
        let registry = &mut cx.global_mut::<Sessions>().0;
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
            let mut annotations = Vec::new();
            for page in 0..document.pages().len() {
                annotations.extend(document.page_annotations(page, epoch)?);
            }
            Ok(Arc::new(annotations))
        });
        self.annotation_load = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.epoch != epoch {
                    return;
                }
                this.annotations = Some(result);
                this.annotation_load = None;
                cx.notify();
            });
        }));
    }
}
