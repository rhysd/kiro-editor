use crate::edit_diff::{EditDiff, UndoRedo};
use crate::row::Row;
use std::cmp;
use std::collections::VecDeque;
use std::mem;

const MAX_ENTRIES: usize = 1000;

pub type Edit = Vec<EditDiff>;

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

    pub fn finish_ongoing_edit(&mut self) -> bool {
        debug_assert!(self.entries.len() <= MAX_ENTRIES);
        if self.ongoing.is_empty() {
            return false; // Do nothing when no change was added
        }

        let diffs = mem::take(&mut self.ongoing);

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
        true
    }

    fn apply_diffs<'a, I: Iterator<Item = &'a EditDiff>>(
        diffs: I,
        which: UndoRedo,
        rows: &mut Vec<Row>,
    ) -> (usize, usize, usize) {
        diffs.fold((0, 0, usize::max_value()), |(_, _, dirty_start), diff| {
            let (x, y) = diff.apply(rows, which);
            (x, y, cmp::min(dirty_start, y))
        })
    }

    pub fn undo(&mut self, rows: &mut Vec<Row>) -> Option<(usize, usize, usize, bool)> {
        let edited = self.finish_ongoing_edit();
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        let i = self.entries[self.index].iter().rev();
        let (x, y, dirty_start) = Self::apply_diffs(i, UndoRedo::Undo, rows);
        Some((x, y, dirty_start, edited))
    }

    pub fn redo(&mut self, rows: &mut Vec<Row>) -> Option<(usize, usize, usize, bool)> {
        let edited = self.finish_ongoing_edit();
        if self.index == self.entries.len() {
            return None;
        }
        self.index += 1;
        let i = self.entries[self.index - 1].iter();
        let (x, y, dirty_start) = Self::apply_diffs(i, UndoRedo::Redo, rows);
        Some((x, y, dirty_start, edited))
    }
}
