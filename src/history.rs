use crate::edit_diff::{EditDiff, UndoRedo};
use crate::span::Span;
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

    pub fn finish_ongoing_edit(&mut self) {
        debug_assert!(self.entries.len() <= MAX_ENTRIES);
        if self.ongoing.is_empty() {
            return; // Do nothing when no change was added
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

    fn apply_diffs<'a, I: Iterator<Item = &'a EditDiff>>(
        diffs: I,
        which: UndoRedo,
        rows: &mut Vec<Span>,
    ) -> (usize, usize, usize) {
        diffs.fold((0, 0, usize::max_value()), |(_, _, dirty_start), diff| {
            let (x, y) = diff.apply(rows, which);
            (x, y, cmp::min(dirty_start, y))
        })
    }

    pub fn undo(&mut self, rows: &mut Vec<Span>) -> Option<(usize, usize, usize)> {
        self.finish_ongoing_edit();
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        let i = self.entries[self.index].iter().rev();
        Some(Self::apply_diffs(i, UndoRedo::Undo, rows))
    }

    pub fn redo(&mut self, rows: &mut Vec<Span>) -> Option<(usize, usize, usize)> {
        self.finish_ongoing_edit();
        if self.index == self.entries.len() {
            return None;
        }
        self.index += 1;
        let i = self.entries[self.index - 1].iter();
        Some(Self::apply_diffs(i, UndoRedo::Redo, rows))
    }
}
