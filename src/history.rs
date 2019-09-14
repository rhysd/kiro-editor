use crate::edit_diff::EditDiff;
use std::collections::VecDeque;
use std::mem;
use std::ops::Deref;

const MAX_ENTRIES: usize = 1000;

pub type Edit = Vec<EditDiff>;

pub struct HistoryEntry {
    index: usize,
    edit: Edit,
}

impl Deref for HistoryEntry {
    type Target = Edit;

    fn deref(&self) -> &Self::Target {
        &self.edit
    }
}

#[derive(Default)]
pub struct History {
    index: usize,
    ongoing: Edit,
    entries: VecDeque<Edit>,
}

impl History {
    pub fn push(&mut self, diff: EditDiff) {
        self.ongoing.push(diff);
    }

    pub fn finish_ongoing_edit(&mut self) {
        debug_assert!(self.entries.len() <= MAX_ENTRIES);
        if self.ongoing.is_empty() {
            return;
        }

        let diffs = mem::replace(&mut self.ongoing, vec![]);

        if self.entries.len() == MAX_ENTRIES {
            self.entries.pop_front();
            self.index -= 1;
        }

        if self.index < self.entries.len() {
            // When new change is added after undo, remove diffs after current point
            self.entries.truncate(self.index);
        }

        self.index += 1;
        self.entries.push_back(diffs);
    }

    fn move_out_entry(&mut self, index: usize) -> HistoryEntry {
        HistoryEntry {
            index,
            edit: mem::replace(&mut self.entries[index], vec![]),
        }
    }

    pub fn undo(&mut self) -> Option<HistoryEntry> {
        if self.index == 0 {
            return None;
        }
        self.finish_ongoing_edit();
        self.index -= 1;
        Some(self.move_out_entry(self.index))
    }

    pub fn redo(&mut self) -> Option<HistoryEntry> {
        if self.index == self.entries.len() {
            return None;
        }
        self.finish_ongoing_edit();
        self.index += 1;
        Some(self.move_out_entry(self.index - 1))
    }

    pub fn finish_undoredo(&mut self, entry: HistoryEntry) {
        mem::replace(&mut self.entries[entry.index], entry.edit); // Replace back
    }
}
