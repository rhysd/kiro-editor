use crate::edit_diff::{EditDiff, UndoRedo};
use crate::error::Result;
use crate::history::History;
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

    pub fn with_lines<'a, I: Iterator<Item = &'a str>>(lines: I) -> Self {
        Self {
            cx: 0,
            cy: 0,
            file: None,
            row: lines.map(Row::new).collect(),
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

    pub fn cursor(&self) -> (usize, usize) {
        (self.cx, self.cy)
    }

    pub fn edit(&mut self) -> EditTextBuffer<'_> {
        EditTextBuffer::new(self)
    }
}

pub struct EditTextBuffer<'a> {
    buf: &'a mut TextBuffer,
    inserted_undo: bool,
}

impl<'a> EditTextBuffer<'a> {
    fn new(buf: &'a mut TextBuffer) -> Self {
        buf.dirty_start = None;
        Self {
            buf,
            inserted_undo: false,
        }
    }

    fn set_dirty_start(&mut self, line: usize) {
        if let Some(l) = self.buf.dirty_start {
            if l <= line {
                return;
            }
        }
        self.buf.dirty_start = Some(line);
    }

    fn apply_diff(&mut self, diff: &EditDiff, which: UndoRedo) {
        let (x, y) = diff.apply(&mut self.buf.row, which);
        self.buf.set_cursor(x, y);
        self.set_dirty_start(y);
    }

    fn new_diff(&mut self, diff: EditDiff) {
        self.apply_diff(&diff, UndoRedo::Redo);
        self.buf.modified = true;
        self.buf.history.push(diff); // Remember diff for undo/redo
    }

    fn insert_undo_point(&mut self) {
        if !self.inserted_undo {
            self.buf.history.finish_ongoing_edit();
            self.inserted_undo = true;
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        // Don't add undo point to squash multiple insert_char changes into one undo
        if self.buf.cy == self.buf.row.len() {
            self.new_diff(EditDiff::Newline);
        }
        self.new_diff(EditDiff::InsertChar(self.buf.cx, self.buf.cy, ch));
    }

    pub fn insert_tab(&mut self) {
        self.insert_undo_point();
        match self.buf.lang.indent() {
            Indent::AsIs => self.insert_char('\t'),
            Indent::Fixed(indent) => {
                self.new_diff(EditDiff::Insert(
                    self.buf.cx,
                    self.buf.cy,
                    indent.to_owned(),
                ));
            }
        }
    }

    fn concat_next_line(&mut self) {
        // TODO: Move buffer rather than copy
        let removed = self.buf.row[self.buf.cy + 1].buffer().to_owned();
        self.new_diff(EditDiff::DeleteLine(self.buf.cy + 1, removed.clone()));
        self.new_diff(EditDiff::Append(self.buf.cy, removed));
    }

    fn squash_to_previous_line(&mut self) {
        // Move cursor to previous line
        self.buf.cy -= 1;
        // At top of line, backspace concats current line to previous line
        self.buf.cx = self.buf.row[self.buf.cy].len(); // Move cursor column to end of previous line
        self.concat_next_line();
    }

    pub fn delete_char(&mut self) {
        let (cx, cy) = self.buf.cursor();
        if cy == self.buf.row.len() || cx == 0 && cy == 0 {
            return;
        }
        self.insert_undo_point();
        if cx > 0 {
            let idx = cx - 1;
            let deleted = self.buf.row[cy].char_at(idx);
            self.new_diff(EditDiff::DeleteChar(cx, cy, deleted));
        } else {
            self.squash_to_previous_line();
        }
    }

    pub fn delete_until_end_of_line(&mut self) {
        let (cx, cy) = self.buf.cursor();
        if cy == self.buf.row.len() {
            return;
        }
        self.insert_undo_point();
        let row = &self.buf.row[cy];
        if cx == row.len() {
            // Do nothing when cursor is at end of line of end of text buffer
            if cy == self.buf.row.len() - 1 {
                return;
            }
            self.concat_next_line();
        } else if cx < row.buffer().len() {
            let truncated = row[cx..].to_owned();
            self.new_diff(EditDiff::Truncate(cy, truncated));
        }
    }

    pub fn delete_until_head_of_line(&mut self) {
        let (cx, cy) = self.buf.cursor();
        if cx == 0 && cy == 0 || cy == self.buf.row.len() {
            return;
        }
        self.insert_undo_point();
        if cx == 0 {
            self.squash_to_previous_line();
        } else {
            let removed = self.buf.row[cy][..cx].to_owned();
            self.new_diff(EditDiff::Remove(cx, cy, removed));
        }
    }

    pub fn delete_word(&mut self) {
        let (cx, cy) = self.buf.cursor();
        if cx == 0 || cy == self.buf.row.len() {
            return;
        }
        self.insert_undo_point();

        let mut x = cx - 1;
        let row = &self.buf.row[cy];
        while x > 0 && row.char_at(x).is_ascii_whitespace() {
            x -= 1;
        }
        // `x - 1` since x should stop at the last non-whitespace character to remove
        while x > 0 && !row.char_at(x - 1).is_ascii_whitespace() {
            x -= 1;
        }

        let removed = self.buf.row[cy][x..cx].to_owned();
        self.new_diff(EditDiff::Remove(cx, cy, removed));
    }

    pub fn delete_right_char(&mut self) {
        let (cx, cy) = self.buf.cursor();
        let row = &self.buf.row;
        if cy == row.len() || cy == row.len() - 1 && cx == row[cy].len() {
            // At end of buffer, nothing can be deleted and cursor should not move
            return;
        }
        self.move_cursor_one(CursorDir::Right);
        self.delete_char();
    }

    pub fn insert_line(&mut self) {
        let (cx, cy) = self.buf.cursor();
        self.insert_undo_point();
        if cy >= self.buf.row.len() {
            self.new_diff(EditDiff::Newline);
        } else if cx >= self.buf.row[cy].len() {
            self.new_diff(EditDiff::InsertLine(cy + 1, "".to_string()));
        } else if cx <= self.buf.row[cy].buffer().len() {
            let truncated = self.buf.row[cy][cx..].to_owned();
            self.new_diff(EditDiff::Truncate(cy, truncated.clone()));
            self.new_diff(EditDiff::InsertLine(self.buf.cy + 1, truncated));
        }
    }

    pub fn move_cursor_one(&mut self, dir: CursorDir) {
        let (cx, cy) = self.buf.cursor();
        match dir {
            CursorDir::Up => self.buf.cy = cy.saturating_sub(1),
            CursorDir::Left => {
                if cx > 0 {
                    self.buf.cx -= 1;
                } else if cy > 0 {
                    // When moving to left at top of line, move cursor to end of previous line
                    self.buf.cy -= 1;
                    self.buf.cx = self.buf.row[cy].len();
                }
            }
            CursorDir::Down => {
                // Allow to move cursor until next line to the last line of file to enable to add a
                // new line at the end.
                if cy < self.buf.row.len() {
                    self.buf.cy += 1;
                }
            }
            CursorDir::Right => {
                if cy < self.buf.row.len() {
                    let len = self.buf.row[cy].len();
                    if cx < len {
                        // Allow to move cursor until next col to the last col of line to enable to
                        // add a new character at the end of line.
                        self.buf.cx += 1;
                    } else if self.buf.cx >= len {
                        // When moving to right at the end of line, move cursor to top of next line.
                        self.buf.cy += 1;
                        self.buf.cx = 0;
                    }
                }
            }
        };

        // Snap cursor to end of line when moving up/down from longer line
        let len = self.buf.row.get(self.buf.cy).map(Row::len).unwrap_or(0);
        if self.buf.cx > len {
            self.buf.cx = len;
        }
    }

    pub fn move_cursor_page(&mut self, dir: CursorDir, rowoff: usize, num_rows: usize) {
        self.buf.cy = match dir {
            CursorDir::Up => rowoff, // Top of screen
            CursorDir::Down => {
                cmp::min(rowoff + num_rows - 1, self.buf.row.len()) // Bottom of screen
            }
            _ => unreachable!(),
        };
        for _ in 0..num_rows {
            self.move_cursor_one(dir);
        }
    }

    pub fn move_cursor_to_buffer_edge(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Left => self.buf.cx = 0,
            CursorDir::Right => {
                if self.buf.cy < self.buf.row.len() {
                    self.buf.cx = self.buf.row[self.buf.cy].len();
                }
            }
            CursorDir::Up => self.buf.cy = 0,
            CursorDir::Down => self.buf.cy = self.buf.row.len(),
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
        let mut prev = CharKind::new_at(&self.buf.row, self.buf.cx, self.buf.cy);
        self.move_cursor_one(dir);
        let mut current = CharKind::new_at(&self.buf.row, self.buf.cx, self.buf.cy);

        loop {
            if self.buf.cy == 0 && self.buf.cx == 0 || self.buf.cy == self.buf.row.len() {
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
            current = CharKind::new_at(&self.buf.row, self.buf.cx, self.buf.cy);
        }
    }

    pub fn move_cursor_paragraph(&mut self, dir: CursorDir) {
        debug_assert!(dir != CursorDir::Left && dir != CursorDir::Right);
        loop {
            self.move_cursor_one(dir);
            if self.buf.cy == 0
                || self.buf.cy == self.buf.row.len()
                || self.buf.row[self.buf.cy - 1].buffer().is_empty()
                    && !self.buf.row[self.buf.cy].buffer().is_empty()
            {
                break;
            }
        }
    }

    fn after_undoredo(&mut self, state: Option<(usize, usize, usize)>) -> bool {
        match state {
            Some((x, y, s)) => {
                self.buf.set_cursor(x, y);
                self.set_dirty_start(s);
                true
            }
            None => false,
        }
    }

    pub fn undo(&mut self) -> bool {
        let state = self.buf.history.undo(&mut self.buf.row);
        self.after_undoredo(state)
    }

    pub fn redo(&mut self) -> bool {
        let state = self.buf.history.redo(&mut self.buf.row);
        self.after_undoredo(state)
    }
}
