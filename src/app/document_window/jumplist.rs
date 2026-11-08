//! Transient semantic navigation state owned by one document window.

use crate::reader::{JumpEvent, JumpLocation};

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
