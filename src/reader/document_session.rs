//! Canonical-path session membership, shared baseline loading, and lifetime.
//! The registry is weak: only participating readers own sessions. Background
//! consumers retain an immutable baseline, never a session or a reader.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Task, WeakEntity};

use crate::pdf;

#[derive(Default)]
struct Sessions(HashMap<PathBuf, WeakEntity<DocumentSession>>);

impl Global for Sessions {}

pub(super) struct DocumentSession {
    path: PathBuf,
    result: Option<Result<Arc<pdf::Document>, String>>,
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
}
