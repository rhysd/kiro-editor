use crate::language::{Indent, Language};
use crate::row::Row;
use std::cmp;
use std::fs::File;
use std::io::{self, BufRead, Write};
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

#[derive(Default)]
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
    // Flag to require screen update
    pub dirty: bool,
}

impl TextBuffer {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref();
        let mut buf = Self::default();

        if path.exists() {
            let file = File::open(path)?;
            buf.row = io::BufReader::new(file)
                .lines()
                .map(|r| Ok(Row::new(r?)))
                .collect::<io::Result<_>>()?;
            buf.modified = false;
        } else {
            // When the path does not exist, consider it as a new file
            buf.modified = true;
        }
        buf.lang = Language::detect(path);
        buf.file = Some(FilePath::from(path));
        Ok(buf)
    }

    pub fn insert_char(&mut self, ch: char) {
        if self.cy == self.row.len() {
            self.row.push(Row::default());
        }
        self.row[self.cy].insert_char(self.cx, ch);
        self.cx += 1;
        self.modified = true;
        self.dirty = true;
    }

    pub fn insert_tab(&mut self) {
        match self.lang.indent() {
            Indent::AsIs => self.insert_char('\t'),
            Indent::Fixed(indent) => self.insert_str(indent),
        }
    }

    pub fn insert_str<S: AsRef<str>>(&mut self, s: S) {
        if self.cy == self.row.len() {
            self.row.push(Row::default());
        }
        let s = s.as_ref();
        self.row[self.cy].insert_str(self.cx, s);
        self.cx += s.as_bytes().len();
        self.modified = true;
        self.dirty = true;
    }

    pub fn squash_to_previous_line(&mut self) {
        // At top of line, backspace concats current line to previous line
        self.cx = self.row[self.cy - 1].len(); // Move cursor column to end of previous line
        let row = self.row.remove(self.cy);
        self.cy -= 1; // Move cursor to previous line
        self.row[self.cy].append(row.buffer()); // TODO: Move buffer rather than copy
        self.modified = true;
        self.dirty = true;
    }

    pub fn delete_char(&mut self) {
        if self.cy == self.row.len() || self.cx == 0 && self.cy == 0 {
            return;
        }
        if self.cx > 0 {
            self.row[self.cy].delete_char(self.cx - 1);
            self.cx -= 1;
            self.modified = true;
            self.dirty = true;
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
            // At end of line, concat with next line
            let deleted = self.row.remove(self.cy + 1);
            self.row[self.cy].append(deleted.buffer()); // TODO: Move buffer rather than copy
        } else {
            self.row[self.cy].truncate(self.cx);
        }
        self.modified = true;
        self.dirty = true;
    }

    pub fn delete_until_head_of_line(&mut self) {
        if self.cx == 0 && self.cy == 0 || self.cy == self.row.len() {
            return;
        }
        if self.cx == 0 {
            self.squash_to_previous_line();
        } else {
            self.row[self.cy].remove(0, self.cx);
            self.cx = 0;
            self.modified = true;
            self.dirty = true;
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

        if x < self.cx {
            self.row[self.cy].remove(x, self.cx);
            self.cx = x;
            self.modified = true;
            self.dirty = true;
        }
    }

    pub fn delete_right_char(&mut self) {
        self.move_cursor_one(CursorDir::Right);
        self.delete_char();
    }

    pub fn insert_line(&mut self) {
        if self.cy >= self.row.len() {
            self.row.push(Row::default());
        } else if self.cx >= self.row[self.cy].len() {
            self.row.insert(self.cy + 1, Row::default());
        } else {
            let split = self.row[self.cy][self.cx..].to_string();
            self.row[self.cy].truncate(self.cx);
            self.row.insert(self.cy + 1, Row::new(split));
        }

        self.cy += 1;
        self.cx = 0;
        self.dirty = true;
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

    pub fn save(&mut self) -> Result<String, String> {
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
}
