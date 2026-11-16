//! Temporary two-endpoint acquisition; completion writes an ordinary Selection.

use std::sync::Arc;

use super::super::{hint, *};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::reader) enum Targeting {
    Words,
    Lines,
}

pub(in crate::reader) struct Acquisition {
    pub mode: Targeting,
    pub stage: Stage,
}

pub(in crate::reader) enum Stage {
    Start,
    End {
        original: Arc<hint::Snapshot<WordTarget>>,
        start: WordTarget,
    },
}

impl ReaderView {
    pub(in crate::reader) fn begin_selection(
        &mut self,
        mode: Targeting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) {
            return;
        }
        self.begin_word_hints(|_| true, window, cx);
        if self.hints.is_active() {
            self.acquisition = Some(Acquisition {
                mode,
                stage: Stage::Start,
            });
        }
    }

    /// Returns true when selection owns the choice; independent hint consumers
    /// still receive the original Chosen event without selection semantics.
    pub(in crate::reader) fn choose_selection_target(
        &mut self,
        snapshot: Arc<hint::Snapshot<WordTarget>>,
        index: usize,
    ) -> bool {
        let Some(Acquisition { mode, stage }) = self.acquisition.take() else {
            return false;
        };
        let target = snapshot.labels()[index].target().id().clone();
        match stage {
            Stage::Start => {
                self.resume_word_hints(
                    Arc::new(snapshot.subset(|word| word.page() == target.page())),
                    Some(target.clone()),
                );
                self.acquisition = Some(Acquisition {
                    mode,
                    stage: Stage::End {
                        original: snapshot,
                        start: target,
                    },
                });
            }
            Stage::End { start, .. } => {
                if start.page() == target.page()
                    && let Some(text) = self.page_text.get(start.page()).cloned()
                {
                    match mode {
                        Targeting::Words => self.selection.select_words(
                            start.page(),
                            text,
                            (start.line(), start.glyphs()),
                            (target.line(), target.glyphs()),
                        ),
                        Targeting::Lines => self.selection.select_lines(
                            start.page(),
                            text,
                            start.line(),
                            target.line(),
                        ),
                    };
                }
                self.finish_hints();
            }
        }
        true
    }

    pub(in crate::reader) fn back_to_selection_start(&mut self) {
        if let Some(Acquisition {
            mode,
            stage: Stage::End { original, .. },
        }) = self.acquisition.take()
        {
            self.resume_word_hints(original, None);
            self.acquisition = Some(Acquisition {
                mode,
                stage: Stage::Start,
            });
        }
    }
}
