//! Transient semantic navigation state owned by one document window.

use crate::reader::{JumpEvent, JumpLocation};

gpui_kit::actions!(kver, [NavigateBack, NavigateForward]);

#[derive(Default)]
pub(super) struct Jumplist {
    pub(super) entries: Vec<JumpLocation>,
    pub(super) cursor: usize,
}

impl Jumplist {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.cursor = 0;
    }

    pub(super) fn can_back(&self) -> bool {
        self.cursor > 0
    }

    pub(super) fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    /// A failed restore removes that entry; the next candidate in the same
    /// direction is tried without ever recording a new semantic jump.
    pub(super) fn traverse(
        &mut self,
        direction: i32,
        mut restore: impl FnMut(&JumpLocation) -> bool,
    ) -> bool {
        loop {
            let candidate = if direction < 0 && self.can_back() {
                self.cursor.checked_sub(1)
            } else if direction > 0 && self.can_forward() {
                Some(self.cursor + 1)
            } else {
                None
            };
            let Some(candidate) = candidate else {
                return false;
            };
            if restore(&self.entries[candidate]) {
                self.cursor = candidate;
                return true;
            }
            self.entries.remove(candidate);
            if candidate < self.cursor {
                self.cursor -= 1;
            }
        }
    }

    pub(super) fn record(&mut self, jump: &JumpEvent) {
        if jump.from.equivalent(&jump.to) {
            return;
        }
        if self.entries.is_empty() {
            self.entries.push(jump.from.clone());
        } else if !self.entries[self.cursor].equivalent(&jump.from) {
            // Capture the departure place at this jump, not during scrolling.
            // Preserve the previous semantic destination behind it.
            self.entries.truncate(self.cursor + 1);
            self.entries.push(jump.from.clone());
            self.cursor += 1;
        }
        if self.entries[self.cursor].equivalent(&jump.to) {
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(jump.to.clone());
        self.cursor += 1;
    }
}
