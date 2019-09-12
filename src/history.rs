use std::collections::VecDeque;
use std::mem;

const MAX_ENTRIES: usize = 1000;

#[derive(Debug)]
pub enum Change {
    InsertChar(usize, usize, char),
    DeleteChar(usize, usize, char),
    Insert(usize, usize, String),
    Append(usize, String),
    Truncate(usize, String),
    Remove(usize, usize, String),
    Newline,
    InsertLine(usize, String),
    DeleteLine(usize, String),
}

type Changes = Vec<Change>;

#[derive(Default)]
pub struct History {
    index: usize, // Always points *next* to the last element of entries which represents last change
    entries: VecDeque<Changes>,
    ongoing: Option<Changes>,
}

impl History {
    pub fn start_new_change(&mut self) {
        self.ongoing = Some(vec![]);
    }

    pub fn end_new_change(&mut self) {
        debug_assert!(self.entries.len() <= MAX_ENTRIES);
        if let Some(changes) = mem::replace(&mut self.ongoing, None) {
            if changes.is_empty() {
                self.ongoing = None;
                return;
            }

            if self.entries.len() == MAX_ENTRIES {
                self.entries.pop_front();
                self.index -= 1;
            }

            if self.index < self.entries.len() {
                // When new change is added after undo, remove changes after current point
                self.entries.truncate(self.index);
            }

            self.index += 1;
            self.entries.push_back(changes);
        }
    }

    pub fn push(&mut self, change: Change) {
        if let Some(ongoing) = &mut self.ongoing {
            ongoing.push(change);
        }
    }

    pub fn undo(&mut self) -> Option<&'_ [Change]> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        Some(&self.entries[self.index])
    }

    pub fn redo(&mut self) -> Option<&'_ [Change]> {
        if self.index == self.entries.len() {
            return None;
        }
        self.index += 1;
        Some(&self.entries[self.index - 1])
    }
}
