use crate::error::Result;
use crate::history::{Change, History};
use crate::language::{Indent, Language};
use crate::row::Row;
use std::cmp;
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

#[derive(Debug)]
enum UndoRedo {
    Undo,
    Redo,
}

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
    history: History,
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
            history: History::default(),
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
            history: History::default(),
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

    fn current_line_is_dirty(&mut self) {
        self.set_dirty_start(self.cy);
    }

    pub fn insert_char(&mut self, ch: char) {
        if self.cy == self.row.len() {
            self.history.push(Change::Newline);
            self.row.push(Row::default());
        }
        self.history.push(Change::InsertChar(self.cx, self.cy, ch));
        self.row[self.cy].insert_char(self.cx, ch);
        self.cx += 1;
        self.modified = true;
        self.current_line_is_dirty();
    }

    pub fn insert_tab(&mut self) {
        match self.lang.indent() {
            Indent::AsIs => self.insert_char('\t'),
            Indent::Fixed(indent) => {
                self.history
                    .push(Change::Insert(self.cx, self.cy, indent.to_owned()));
                self.insert_str(indent);
            }
        }
    }

    pub fn insert_str<S: AsRef<str>>(&mut self, s: S) {
        if self.cy == self.row.len() {
            self.history.push(Change::Newline);
            self.row.push(Row::default());
        }
        let s = s.as_ref();
        self.history
            .push(Change::Insert(self.cx, self.cy, s.to_owned()));
        self.row[self.cy].insert_str(self.cx, s);
        self.cx += s.chars().count();
        self.modified = true;
        self.current_line_is_dirty();
    }

    fn concat_next_line(&mut self) {
        // TODO: Move buffer rather than copy
        let removed = self.row.remove(self.cy + 1);
        self.history
            .push(Change::DeleteLine(self.cy + 1, removed.buffer().to_owned()));
        self.row[self.cy].append(removed.buffer());
        self.history
            .push(Change::Append(self.cy, removed.buffer().to_owned()));
        self.modified = true;
        self.current_line_is_dirty();
    }

    fn squash_to_previous_line(&mut self) {
        // At top of line, backspace concats current line to previous line
        self.cx = self.row[self.cy - 1].len(); // Move cursor column to end of previous line
        self.cy -= 1; // Move cursor to previous line
        self.concat_next_line();
    }

    pub fn delete_char(&mut self) {
        if self.cy == self.row.len() || self.cx == 0 && self.cy == 0 {
            return;
        }
        if self.cx > 0 {
            let idx = self.cx - 1;
            let deleted = self.row[self.cy].char_at(idx);
            self.row[self.cy].delete_char(idx);
            self.history
                .push(Change::DeleteChar(self.cx, self.cy, deleted));
            self.cx -= 1;
            self.modified = true;
            self.current_line_is_dirty();
        } else {
            self.squash_to_previous_line();
        }
    }

    pub fn delete_until_end_of_line(&mut self) {
        if self.cy == self.row.len() {
            return;
        }
        if self.cx == self.row[self.cy].len() {
            // Do nothing when cursor is at end of line of end of text buffer
            if self.cy == self.row.len() - 1 {
                return;
            }
            self.concat_next_line();
        } else if let Some(truncated) = self.row[self.cy].truncate(self.cx) {
            self.history.push(Change::Truncate(self.cy, truncated));
            self.modified = true;
            self.current_line_is_dirty();
        }
    }

    pub fn delete_until_head_of_line(&mut self) {
        if self.cx == 0 && self.cy == 0 || self.cy == self.row.len() {
            return;
        }
        if self.cx == 0 {
            self.squash_to_previous_line();
        } else if let Some(removed) = self.row[self.cy].drain(0, self.cx) {
            self.history.push(Change::Remove(self.cx, self.cy, removed));
            self.cx = 0;
            self.modified = true;
            self.current_line_is_dirty();
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

        if let Some(removed) = self.row[self.cy].drain(x, self.cx) {
            self.history.push(Change::Remove(x, self.cx, removed));
            self.cx = x;
            self.modified = true;
            self.current_line_is_dirty();
        }
    }

    pub fn delete_right_char(&mut self) {
        self.move_cursor_one(CursorDir::Right);
        self.delete_char();
    }

    pub fn insert_line(&mut self) {
        if self.cy >= self.row.len() {
            self.row.push(Row::default());
            self.history.push(Change::Newline);
        } else if self.cx >= self.row[self.cy].len() {
            self.row.insert(self.cy + 1, Row::default());
            self.history
                .push(Change::InsertLine(self.cy + 1, "".to_string()));
        } else {
            let split = self.row[self.cy][self.cx..].to_string();
            if let Some(truncated) = self.row[self.cy].truncate(self.cx) {
                self.history.push(Change::Truncate(self.cy, truncated));
            }
            self.row.insert(self.cy + 1, Row::new(split.clone()));
            self.history.push(Change::InsertLine(self.cy + 1, split));
        }

        self.current_line_is_dirty();

        self.cy += 1;
        self.cx = 0;
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

    pub fn start_undo_point(&mut self) {
        self.history.start_new_change();
    }

    pub fn end_undo_point(&mut self) {
        self.history.end_new_change();
    }

    fn undo_change(&mut self, change: &Change) -> usize {
        match change {
            &Change::InsertChar(x, y, _) => {
                self.row[y].remove_char(x);
                self.cx = x;
                self.cy = y;
                y
            }
            &Change::DeleteChar(x, y, c) => {
                self.row[y].insert_char(x - 1, c);
                self.cx = x;
                self.cy = y;
                y
            }
            &Change::Append(y, ref s) => {
                let count = s.chars().count();
                let len = self.row[y].len();
                self.row[y].remove(len - count, len);
                self.cx = self.row[y].len();
                self.cy = y;
                y
            }
            &Change::Truncate(y, ref s) => {
                self.row[y].append(s);
                self.cx = self.row[y].len() - s.chars().count();
                self.cy = y;
                y
            }
            &Change::Insert(x, y, ref s) => {
                self.row[y].remove(x, s.chars().count());
                self.cx = x;
                self.cy = y;
                y
            }
            &Change::Remove(x, y, ref s) => {
                let count = s.chars().count();
                self.row[y].insert_str(x - count, s);
                self.cx = x;
                self.cy = y;
                y
            }
            &Change::Newline => {
                debug_assert_eq!(self.row[self.row.len() - 1].buffer(), "");
                self.row.pop();
                self.cy = self.row.len();
                self.cx = 0;
                self.cy
            }
            &Change::InsertLine(y, _) => {
                self.row.remove(y);
                self.cx = self.row[y - 1].len();
                self.cy = y - 1;
                y
            }
            &Change::DeleteLine(y, ref s) => {
                self.row.insert(y, Row::new(s));
                self.cx = 0;
                self.cy = y;
                y
            }
        }
    }

    fn redo_change(&mut self, change: &Change) -> usize {
        match change {
            &Change::InsertChar(x, y, c) => {
                self.row[y].insert_char(x, c);
                self.cx = x + 1;
                self.cy = y;
                y
            }
            &Change::DeleteChar(x, y, _) => {
                self.row[y].remove_char(x - 1);
                self.cx = x - 1;
                self.cy = y;
                y
            }
            &Change::Append(y, ref s) => {
                self.cx = self.row[y].len();
                self.cy = y;
                self.row[y].append(s);
                y
            }
            &Change::Truncate(y, ref s) => {
                let count = s.chars().count();
                let len = self.row[y].len();
                self.row[y].truncate(len - count);
                self.cx = len - count;
                self.cy = y;
                y
            }
            &Change::Insert(x, y, ref s) => {
                self.row[y].insert_str(x, s);
                self.cx = x;
                self.cy = y;
                y
            }
            &Change::Remove(x, y, ref s) => {
                self.cx = x - s.chars().count();
                self.row[y].remove(self.cx, x);
                self.cy = y;
                y
            }
            &Change::Newline => {
                self.cx = 0;
                self.cy = self.row.len();
                self.row.push(Row::empty());
                self.cy
            }
            &Change::InsertLine(y, ref s) => {
                self.row.insert(y, Row::new(s));
                self.cx = 0;
                self.cy = y;
                y
            }
            &Change::DeleteLine(y, _) => {
                self.row.remove(y);
                self.cy = y - 1;
                self.cx = self.row[self.cy].len();
                self.cy
            }
        }
    }

    fn undoredo(&mut self, which: UndoRedo) -> bool {
        use UndoRedo::*;

        // Move out self.history. Following `for` statement requires to borrow `self.history` but in body
        // of the loop we need to borrow `self` mutably to update text buffer. It is not allowed by Rust
        // compiler
        let mut history = mem::replace(&mut self.history, Default::default());
        let mut success = false;

        let changes = match which {
            Undo => history.undo(),
            Redo => history.redo(),
        };
        if let Some(changes) = changes {
            debug_assert!(changes.len() > 0);
            match which {
                Undo => {
                    for change in changes.iter().rev() {
                        let y = self.undo_change(change);
                        self.set_dirty_start(y);
                    }
                }
                Redo => {
                    for change in changes.iter() {
                        let y = self.redo_change(change);
                        self.set_dirty_start(y);
                    }
                }
            }
            self.modified = true;
            success = true;
        }

        mem::replace(&mut self.history, history);
        success
    }

    pub fn undo(&mut self) -> bool {
        self.undoredo(UndoRedo::Undo)
    }

    pub fn redo(&mut self) -> bool {
        self.undoredo(UndoRedo::Redo)
    }
}
