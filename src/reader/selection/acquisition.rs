//! Temporary two-endpoint acquisition; completion writes an ordinary Selection.

use std::sync::Arc;

use super::super::{hint, *};

pub(in crate::reader) enum Acquisition {
    Start,
    End {
        original: Arc<hint::Snapshot<WordTarget>>,
        start: WordTarget,
    },
}

impl ReaderView {
    pub(in crate::reader) fn begin_word_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) {
            return;
        }
        self.begin_word_hints(|_| true, window, cx);
        if self.hints.is_active() {
            self.word_acquisition = Some(Acquisition::Start);
        }
    }

    /// Returns true when selection owns the choice; independent hint consumers
    /// still receive the original Chosen event without selection semantics.
    pub(in crate::reader) fn choose_selection_word(
        &mut self,
        snapshot: Arc<hint::Snapshot<WordTarget>>,
        index: usize,
    ) -> bool {
        let Some(stage) = self.word_acquisition.take() else {
            return false;
        };
        let target = snapshot.labels()[index].target().id().clone();
        match stage {
            Acquisition::Start => {
                self.resume_word_hints(
                    Arc::new(snapshot.subset(|word| word.page() == target.page())),
                    Some(target.clone()),
                );
                self.word_acquisition = Some(Acquisition::End {
                    original: snapshot,
                    start: target,
                });
            }
            Acquisition::End { start, .. } => {
                if start.page() == target.page()
                    && let Some(text) = self.page_text.get(start.page()).cloned()
                {
                    self.selection.select_words(
                        start.page(),
                        text,
                        (start.line(), start.glyphs()),
                        (target.line(), target.glyphs()),
                    );
                }
                self.finish_word_hints();
            }
        }
        true
    }

    pub(in crate::reader) fn back_to_word_start(&mut self) {
        if let Some(Acquisition::End { original, .. }) = self.word_acquisition.take() {
            self.resume_word_hints(original, None);
            self.word_acquisition = Some(Acquisition::Start);
        }
    }
}
