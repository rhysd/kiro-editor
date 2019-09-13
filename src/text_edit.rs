use crate::row::Row;

#[derive(Debug, Clone, Copy)]
pub enum UndoRedo {
    Undo,
    Redo,
}

#[derive(Debug)]
pub enum EditDiff {
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

pub type Edit = Vec<EditDiff>;

impl EditDiff {
    pub fn apply(&self, rows: &mut Vec<Row>, which: UndoRedo) -> (usize, usize) {
        use UndoRedo::*;
        match *self {
            EditDiff::InsertChar(x, y, c) => match which {
                Undo => {
                    rows[y].remove_char(x);
                    (x, y)
                }
                Redo => {
                    rows[y].insert_char(x, c);
                    (x + 1, y)
                }
            },
            EditDiff::DeleteChar(x, y, c) => match which {
                Undo => {
                    rows[y].insert_char(x - 1, c);
                    (x, y)
                }
                Redo => {
                    rows[y].remove_char(x - 1);
                    (x - 1, y)
                }
            },
            EditDiff::Append(y, ref s) => match which {
                Undo => {
                    let count = s.chars().count();
                    let len = rows[y].len();
                    rows[y].remove(len - count, len);
                    let x = rows[y].len();
                    (x, y)
                }
                Redo => {
                    let x = rows[y].len();
                    rows[y].append(s);
                    (x, y)
                }
            },
            EditDiff::Truncate(y, ref s) => match which {
                Undo => {
                    rows[y].append(s);
                    let x = rows[y].len() - s.chars().count();
                    (x, y)
                }
                Redo => {
                    let count = s.chars().count();
                    let len = rows[y].len();
                    rows[y].truncate(len - count);
                    (len - count, y)
                }
            },
            EditDiff::Insert(x, y, ref s) => match which {
                Undo => {
                    rows[y].remove(x, s.chars().count());
                    (x, y)
                }
                Redo => {
                    rows[y].insert_str(x, s);
                    (x, y)
                }
            },
            EditDiff::Remove(x, y, ref s) => match which {
                Undo => {
                    let count = s.chars().count();
                    rows[y].insert_str(x - count, s);
                    (x, y)
                }
                Redo => {
                    let next_x = x - s.chars().count();
                    rows[y].remove(next_x, x);
                    (next_x, y)
                }
            },
            EditDiff::Newline => match which {
                Undo => {
                    debug_assert_eq!(rows[rows.len() - 1].buffer(), "");
                    rows.pop();
                    let y = rows.len();
                    (0, y)
                }
                Redo => {
                    let y = rows.len();
                    rows.push(Row::empty());
                    (0, y)
                }
            },
            EditDiff::InsertLine(y, ref s) => match which {
                Undo => {
                    rows.remove(y);
                    let x = rows[y - 1].len();
                    let y = y - 1;
                    (x, y)
                }
                Redo => {
                    rows.insert(y, Row::new(s));
                    (0, y)
                }
            },
            EditDiff::DeleteLine(y, ref s) => match which {
                Undo => {
                    if y == rows.len() {
                        rows.push(Row::new(s));
                    } else {
                        rows.insert(y, Row::new(s));
                    }
                    (0, y)
                }
                Redo => {
                    if y == rows.len() - 1 {
                        rows.pop();
                    } else {
                        rows.remove(y);
                    }
                    (rows[y - 1].len(), y - 1)
                }
            },
        }
    }
}
