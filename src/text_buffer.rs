use crate::edit_diff::{EditDiff, UndoRedo};
use crate::error::Result;
use crate::language::{Indent, Language};
use crate::row::Row;
use std::cmp;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::mem;
use std::path::{Path, PathBuf};
use std::slice;

// Contain both actual path sequence and display string
pub struct FilePath {
    pub path: PathBuf,
    pub display: String,
}

impl FilePath {
    fn from<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        FilePath {
            path: PathBuf::from(path),
            display: path.to_string_lossy().to_string(),
        }
    }

    fn from_string<S: Into<String>>(s: S) -> Self {
        let display = s.into();
        FilePath {
            path: PathBuf::from(&display),
            display,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum CursorDir {
    Left,
    Right,
    Up,
    Down,
}

pub struct Lines<'a>(slice::Iter<'a, Row>);

impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|r| r.buffer())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.as_slice().len();
        (len, Some(len))
    }
}

const MAX_ENTRIES: usize = 1000;

pub type Edit = Vec<EditDiff>;

pub struct TextBuffer {
    // (x, y) coordinate in internal text buffer of rows
    cx: usize,
    cy: usize,
    // File editor is opening
    file: Option<FilePath>,
    // Lines of text buffer
    row: Vec<Row>,
    // Flag set to true when buffer is modified after loading a file
    modified: bool,
    // Language which current buffer belongs to
    lang: Language,
    // History per undo point for undo/redo
    history_index: usize,
    history: VecDeque<Edit>,
    ongoing_edit: Edit,
    // Flag to require screen update
    // TODO: Merge with Screen's dirty_start field by using RenderContext struct
    pub dirty_start: Option<usize>,
}

impl TextBuffer {
    pub fn empty() -> Self {
        Self {
            cx: 0,
            cy: 0,
            file: None,
            row: vec![Row::empty()], // Ensure that every text ends with newline
            modified: false,
            lang: Language::Plain,
            history_index: 0,
            history: VecDeque::new(),
            ongoing_edit: vec![],
            dirty_start: Some(0), // Ensure to render first screen
        }
    }

    pub fn with_lines<'a, I: Iterator<Item = &'a str>>(lines: I) -> Self {
        Self {
            cx: 0,
            cy: 0,
            file: None,
            row: lines.map(Row::new).collect(),
            modified: false,
            lang: Language::Plain,
            history_index: 0,
            history: VecDeque::new(),
            ongoing_edit: vec![],
            dirty_start: Some(0), // Ensure to render first screen
        }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = Some(FilePath::from(path));
        if !path.exists() {
            // When the path does not exist, consider it as a new file
            let mut buf = Self::empty();
            buf.file = file;
            buf.modified = true;
            buf.lang = Language::detect(path);
            return Ok(buf);
        }

        let row = io::BufReader::new(File::open(path)?)
            .lines()
            .map(|r| Ok(Row::new(r?)))
            .collect::<Result<_>>()?;

        Ok(Self {
            cx: 0,
            cy: 0,
            file,
            row,
            modified: false,
            lang: Language::detect(path),
            history_index: 0,
            history: VecDeque::new(),
            ongoing_edit: vec![],
            dirty_start: Some(0),
        })
    }

    fn set_dirty_start(&mut self, line: usize) {
        if let Some(l) = self.dirty_start {
            if l <= line {
                return;
            }
        }
        self.dirty_start = Some(line);
    }

    fn apply_diff(&mut self, diff: &EditDiff, which: UndoRedo) {
        let (x, y) = diff.apply(&mut self.row, which);
        self.set_cursor(x, y);
        self.set_dirty_start(y);
    }

    fn new_diff(&mut self, diff: EditDiff) {
        self.apply_diff(&diff, UndoRedo::Redo);
        self.modified = true;
        self.ongoing_edit.push(diff); // Remember diff for undo/redo
    }

    pub fn insert_char(&mut self, ch: char) {
        if self.cy == self.row.len() {
            self.new_diff(EditDiff::Newline);
        }
        self.new_diff(EditDiff::InsertChar(self.cx, self.cy, ch));
    }

    pub fn insert_tab(&mut self) {
        match self.lang.indent() {
            Indent::AsIs => self.insert_char('\t'),
            Indent::Fixed(indent) => {
                self.new_diff(EditDiff::Insert(self.cx, self.cy, indent.to_owned()));
            }
        }
    }

    fn concat_next_line(&mut self) {
        // TODO: Move buffer rather than copy
        let removed = self.row[self.cy + 1].buffer().to_owned();
        self.new_diff(EditDiff::DeleteLine(self.cy + 1, removed.clone()));
        self.new_diff(EditDiff::Append(self.cy, removed));
    }

    fn squash_to_previous_line(&mut self) {
        // Move cursor to previous line
        self.cy -= 1;
        // At top of line, backspace concats current line to previous line
        self.cx = self.row[self.cy].len(); // Move cursor column to end of previous line
        self.concat_next_line();
    }

    pub fn delete_char(&mut self) {
        if self.cy == self.row.len() || self.cx == 0 && self.cy == 0 {
            return;
        }
        if self.cx > 0 {
            let idx = self.cx - 1;
            let deleted = self.row[self.cy].char_at(idx);
            self.new_diff(EditDiff::DeleteChar(self.cx, self.cy, deleted));
        } else {
            self.squash_to_previous_line();
        }
    }

    pub fn delete_until_end_of_line(&mut self) {
        if self.cy == self.row.len() {
            return;
        }
        let row = &self.row[self.cy];
        if self.cx == row.len() {
            // Do nothing when cursor is at end of line of end of text buffer
            if self.cy == self.row.len() - 1 {
                return;
            }
            self.concat_next_line();
        } else if self.cx < row.buffer().len() {
            let truncated = row[self.cx..].to_owned();
            self.new_diff(EditDiff::Truncate(self.cy, truncated));
        }
    }

    pub fn delete_until_head_of_line(&mut self) {
        if self.cx == 0 && self.cy == 0 || self.cy == self.row.len() {
            return;
        }
        if self.cx == 0 {
            self.squash_to_previous_line();
        } else {
            let removed = self.row[self.cy][..self.cx].to_owned();
            self.new_diff(EditDiff::Remove(self.cx, self.cy, removed));
        }
    }

    pub fn delete_word(&mut self) {
        if self.cx == 0 || self.cy == self.row.len() {
            return;
        }

        let mut x = self.cx - 1;
        let row = &self.row[self.cy];
        while x > 0 && row.char_at(x).is_ascii_whitespace() {
            x -= 1;
        }
        // `x - 1` since x should stop at the last non-whitespace character to remove
        while x > 0 && !row.char_at(x - 1).is_ascii_whitespace() {
            x -= 1;
        }

        let removed = self.row[self.cy][x..self.cx].to_owned();
        self.new_diff(EditDiff::Remove(self.cx, self.cy, removed));
    }

    pub fn delete_right_char(&mut self) {
        self.move_cursor_one(CursorDir::Right);
        self.delete_char();
    }

    pub fn insert_line(&mut self) {
        if self.cy >= self.row.len() {
            self.new_diff(EditDiff::Newline);
        } else if self.cx >= self.row[self.cy].len() {
            self.new_diff(EditDiff::InsertLine(self.cy + 1, "".to_string()));
        } else if self.cx <= self.row[self.cy].buffer().len() {
            let truncated = self.row[self.cy][self.cx..].to_owned();
            self.new_diff(EditDiff::Truncate(self.cy, truncated.clone()));
            self.new_diff(EditDiff::InsertLine(self.cy + 1, truncated));
        }
    }

    pub fn move_cursor_one(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Up => self.cy = self.cy.saturating_sub(1),
            CursorDir::Left => {
                if self.cx > 0 {
                    self.cx -= 1;
                } else if self.cy > 0 {
                    // When moving to left at top of line, move cursor to end of previous line
                    self.cy -= 1;
                    self.cx = self.row[self.cy].len();
                }
            }
            CursorDir::Down => {
                // Allow to move cursor until next line to the last line of file to enable to add a
                // new line at the end.
                if self.cy < self.row.len() {
                    self.cy += 1;
                }
            }
            CursorDir::Right => {
                if self.cy < self.row.len() {
                    let len = self.row[self.cy].len();
                    if self.cx < len {
                        // Allow to move cursor until next col to the last col of line to enable to
                        // add a new character at the end of line.
                        self.cx += 1;
                    } else if self.cx >= len {
                        // When moving to right at the end of line, move cursor to top of next line.
                        self.cy += 1;
                        self.cx = 0;
                    }
                }
            }
        };

        // Snap cursor to end of line when moving up/down from longer line
        let len = self.row.get(self.cy).map(Row::len).unwrap_or(0);
        if self.cx > len {
            self.cx = len;
        }
    }

    pub fn move_cursor_page(&mut self, dir: CursorDir, rowoff: usize, num_rows: usize) {
        self.cy = match dir {
            CursorDir::Up => rowoff, // Top of screen
            CursorDir::Down => {
                cmp::min(rowoff + num_rows - 1, self.row.len()) // Bottom of screen
            }
            _ => unreachable!(),
        };
        for _ in 0..num_rows {
            self.move_cursor_one(dir);
        }
    }

    pub fn move_cursor_to_buffer_edge(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Left => self.cx = 0,
            CursorDir::Right => {
                if self.cy < self.row.len() {
                    self.cx = self.row[self.cy].len();
                }
            }
            CursorDir::Up => self.cy = 0,
            CursorDir::Down => self.cy = self.row.len(),
        }
    }

    pub fn move_cursor_by_word(&mut self, dir: CursorDir) {
        #[derive(PartialEq)]
        enum CharKind {
            Ident,
            Punc,
            Space,
        }

        impl CharKind {
            fn new_at(rows: &[Row], x: usize, y: usize) -> Self {
                rows.get(y)
                    .and_then(|r| r.char_at_checked(x))
                    .map(|c| {
                        if c.is_ascii_whitespace() {
                            CharKind::Space
                        } else if c == '_' || c.is_ascii_alphanumeric() {
                            CharKind::Ident
                        } else {
                            CharKind::Punc
                        }
                    })
                    .unwrap_or(CharKind::Space)
            }
        }

        fn at_word_start(left: &CharKind, right: &CharKind) -> bool {
            match (left, right) {
                (&CharKind::Space, &CharKind::Ident)
                | (&CharKind::Space, &CharKind::Punc)
                | (&CharKind::Punc, &CharKind::Ident)
                | (&CharKind::Ident, &CharKind::Punc) => true,
                _ => false,
            }
        }

        self.move_cursor_one(dir);
        let mut prev = CharKind::new_at(&self.row, self.cx, self.cy);
        self.move_cursor_one(dir);
        let mut current = CharKind::new_at(&self.row, self.cx, self.cy);

        loop {
            if self.cy == 0 && self.cx == 0 || self.cy == self.row.len() {
                return;
            }

            match dir {
                CursorDir::Right if at_word_start(&prev, &current) => return,
                CursorDir::Left if at_word_start(&current, &prev) => {
                    self.move_cursor_one(CursorDir::Right); // Adjust cursor position to start of word
                    return;
                }
                _ => {}
            }

            prev = current;
            self.move_cursor_one(dir);
            current = CharKind::new_at(&self.row, self.cx, self.cy);
        }
    }

    pub fn move_cursor_paragraph(&mut self, dir: CursorDir) {
        debug_assert!(dir != CursorDir::Left && dir != CursorDir::Right);
        loop {
            self.move_cursor_one(dir);
            if self.cy == 0
                || self.cy == self.row.len()
                || self.row[self.cy - 1].buffer().is_empty()
                    && !self.row[self.cy].buffer().is_empty()
            {
                break;
            }
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.row
    }

    pub fn has_file(&self) -> bool {
        self.file.is_some()
    }

    pub fn filename(&self) -> &str {
        self.file
            .as_ref()
            .map(|f| f.display.as_str())
            .unwrap_or("[No Name]")
    }

    pub fn modified(&self) -> bool {
        self.modified
    }

    pub fn lang(&self) -> Language {
        self.lang
    }

    pub fn cx(&self) -> usize {
        self.cx
    }

    pub fn cy(&self) -> usize {
        self.cy
    }

    pub fn lines(&self) -> Lines<'_> {
        Lines(self.row.iter())
    }

    pub fn set_file<S: Into<String>>(&mut self, file_path: S) {
        let file = FilePath::from_string(file_path);
        self.lang = Language::detect(&file.path);
        self.file = Some(file);
    }

    pub fn set_unnamed(&mut self) {
        self.file = None;
    }

    pub fn save(&mut self) -> std::result::Result<String, String> {
        let file = if let Some(file) = &self.file {
            file
        } else {
            return Ok("".to_string()); // Canceled
        };

        let f = match File::create(&file.path) {
            Ok(f) => f,
            Err(e) => return Err(format!("Could not save: {}", e)),
        };
        let mut f = io::BufWriter::new(f);
        let mut bytes = 0;
        for line in self.row.iter() {
            let b = line.buffer();
            writeln!(f, "{}", b).map_err(|e| format!("Could not write to file: {}", e))?;
            bytes += b.as_bytes().len() + 1;
        }
        f.flush()
            .map_err(|e| format!("Could not flush to file: {}", e))?;

        self.modified = false;
        Ok(format!("{} bytes written to {}", bytes, &file.display))
    }

    pub fn set_cursor(&mut self, x: usize, y: usize) {
        self.cx = x;
        self.cy = y;
    }

    pub fn finish_undo_point(&mut self) {
        debug_assert!(self.history.len() <= MAX_ENTRIES);
        if self.ongoing_edit.is_empty() {
            return;
        }

        let diffs = mem::replace(&mut self.ongoing_edit, vec![]);

        if self.history.len() == MAX_ENTRIES {
            self.history.pop_front();
            self.history_index -= 1;
        }

        if self.history_index < self.history.len() {
            // When new change is added after undo, remove diffs after current point
            self.history.truncate(self.history_index);
        }

        self.history_index += 1;
        self.history.push_back(diffs);
    }

    fn undoredo(&mut self, which: UndoRedo) -> bool {
        use UndoRedo::*;

        if !self.ongoing_edit.is_empty() {
            self.finish_undo_point();
        }

        let index = match which {
            Undo if self.history_index == 0 => return false,
            Undo => {
                self.history_index -= 1;
                self.history_index
            }
            Redo if self.history_index == self.history.len() => return false,
            Redo => {
                self.history_index += 1;
                self.history_index - 1
            }
        };

        let diffs = mem::replace(&mut self.history[index], vec![]); // Move out for borrowing
        debug_assert!(!diffs.is_empty());

        match which {
            Undo => {
                for diff in diffs.iter().rev() {
                    self.apply_diff(diff, which);
                }
            }
            Redo => {
                for diff in diffs.iter() {
                    self.apply_diff(diff, which);
                }
            }
        }

        mem::replace(&mut self.history[index], diffs); // Replace back
        true
    }

    pub fn undo(&mut self) -> bool {
        self.undoredo(UndoRedo::Undo)
    }

    pub fn redo(&mut self) -> bool {
        self.undoredo(UndoRedo::Redo)
    }
}
