//! Session-local reversible catalog edits, independent of markup subtype and UI.
use super::*;

#[derive(Default)]
pub(super) struct AnnotationHistory {
    undo: Vec<AnnotationEdit>,
    redo: Vec<AnnotationEdit>,
}

struct AnnotationEdit {
    // Edits do not move geometry. Replaying in history order restores this
    // document/spatial position even when neighboring annotations were deleted.
    ix: usize,
    before: Option<Annotation>,
    after: Option<Annotation>,
}

impl DocumentSession {
    pub(crate) fn has_annotation_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub(crate) fn has_annotation_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    pub(super) fn record_edit(
        &mut self,
        ix: usize,
        before: Option<Annotation>,
        after: Option<Annotation>,
        cx: &mut Context<Self>,
    ) {
        self.history.undo.push(AnnotationEdit { ix, before, after });
        self.history.redo.clear();
        self.annotations_changed(cx);
    }

    pub(crate) fn undo_annotation(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.history.undo.pop() else {
            return;
        };
        self.restore_annotation(&edit, false);
        self.history.redo.push(edit);
        self.annotations_changed(cx);
    }

    pub(crate) fn redo_annotation(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.history.redo.pop() else {
            return;
        };
        self.restore_annotation(&edit, true);
        self.history.undo.push(edit);
        self.annotations_changed(cx);
    }

    fn restore_annotation(&mut self, edit: &AnnotationEdit, forward: bool) {
        let (from, to) = if forward {
            (&edit.before, &edit.after)
        } else {
            (&edit.after, &edit.before)
        };
        // Only validated mutations enter history. Loading/reload/discard reset
        // history together with the catalog; Save does neither.
        if let Some(Ok(annotations)) = &mut self.annotations {
            Arc::make_mut(annotations).splice(
                edit.ix..edit.ix + usize::from(from.is_some()),
                to.iter().cloned(),
            );
        }
    }

    pub(super) fn annotations_changed(&mut self, cx: &mut Context<Self>) {
        // Revision is an async invalidation token, never a history cursor.
        self.revision += 1;
        self.update_dirty();
        crate::app::quit_guard::invalidate_approval();
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(super) fn update_dirty(&mut self) {
        // Content equality also handles recolor-back and create-then-delete,
        // not merely undoing to a particular history cursor. Cache for chrome.
        self.dirty = self
            .annotations
            .as_ref()
            .and_then(|catalog| catalog.as_ref().ok())
            .is_some_and(|current| current != &self.saved_annotations);
    }
}
