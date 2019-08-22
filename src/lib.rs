// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

#![allow(clippy::unused_io_amount)]
#![allow(clippy::match_overlapping_arm)]
#![allow(clippy::useless_let_if_seq)]

mod ansi_color;
mod highlight;
mod input;
mod language;
mod row;
mod screen;

use highlight::Highlighting;
pub use input::StdinRawMode;
use input::{InputSeq, KeySeq};
use language::{Indent, Language};
use row::Row;
use screen::Screen;
use std::cmp;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::str;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const HELP: &str = r#"
A simplistic terminal text editor for Unix-like systems.

All keymaps as follows.

    Ctrl-Q     : Quit
    Ctrl-S     : Save to file
    Ctrl-P     : Move cursor up
    Ctrl-N     : Move cursor down
    Ctrl-F     : Move cursor right
    Ctrl-B     : Move cursor left
    Ctrl-A     : Move cursor to head of line
    Ctrl-E     : Move cursor to end of line
    Ctrl-V     : Next page
    Alt-V      : Previous page
    Alt-F      : Move cursor to next word
    Alt-B      : Move cursor to previous word
    Alt-<      : Move cursor to top of file
    Alt->      : Move cursor to bottom of file
    Ctrl-H     : Delete character
    Ctrl-D     : Delete next character
    Ctrl-U     : Delete until head of line
    Ctrl-K     : Delete until end of line
    Ctrl-M     : New line
    Ctrl-G     : Search text
    Ctrl-L     : Refresh screen
    Ctrl-?     : Show this help
    UP         : Move cursor up
    DOWN       : Move cursor down
    RIGHT      : Move cursor right
    LEFT       : Move cursor left
    PAGE DOWN  : Next page
    PAGE UP    : Previous page
    HOME       : Move cursor to head of line
    END        : Move cursor to end of line
    DELETE     : Delete next character
    BACKSPACE  : Delete character
    ESC        : Refresh screen
    Ctrl-RIGHT : Move cursor to next word
    Ctrl-LEFT  : Move cursor to previous word
    Alt-RIGHT  : Move cursor to end of line
    Alt-LEFT   : Move cursor to head of line
"#;

// Contain both actual path sequence and display string
struct FilePath {
    path: PathBuf,
    display: String,
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

#[derive(Clone, Copy)]
enum FindDir {
    Back,
    Forward,
}
struct FindState {
    last_match: Option<usize>,
    dir: FindDir,
}

impl FindState {
    fn new() -> FindState {
        FindState {
            last_match: None,
            dir: FindDir::Forward,
        }
    }
}

#[derive(Clone, Copy)]
enum CursorDir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(PartialEq)]
enum AfterKeyPress {
    Quit,
    Nothing,
}

pub struct Editor<I: Iterator<Item = io::Result<InputSeq>>> {
    // VT100 sequence stream represented as Iterator
    input: I,
    // File editor is opening
    file: Option<FilePath>,
    // (x, y) coordinate in internal text buffer of rows
    cx: usize,
    cy: usize,
    // Lines of text buffer
    row: Vec<Row>,
    // Flag set to true when buffer is modified after loading a file
    modified: bool,
    // After first Ctrl-Q
    quitting: bool,
    // Text search state
    finding: FindState,
    // Language which current buffer belongs to
    lang: Language,
    // Syntax highlighting
    hl: Highlighting,
    screen: Screen,
}

impl<I: Iterator<Item = io::Result<InputSeq>>> Editor<I> {
    pub fn new(window_size: Option<(usize, usize)>, mut input: I) -> io::Result<Editor<I>> {
        let screen = Screen::new(window_size, &mut input)?;
        Ok(Editor {
            input,
            file: None,
            cx: 0,
            cy: 0,
            row: vec![],
            modified: false,
            quitting: false,
            finding: FindState::new(),
            lang: Language::Plain,
            hl: Highlighting::default(),
            // Screen height is 1 line less than window height due to status bar
            screen,
        })
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        self.screen.refresh(
            &self.row,
            self.file
                .as_ref()
                .map(|f| f.display.as_str())
                .unwrap_or("[No Name]"),
            self.modified,
            self.lang.name(),
            self.cx,
            self.cy,
            &mut self.hl,
        )
    }

    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        if path.exists() {
            let file = fs::File::open(path)?;
            self.row = io::BufReader::new(file)
                .lines()
                .map(|r| Ok(Row::new(r?)))
                .collect::<io::Result<_>>()?;
            self.modified = false;
        } else {
            // When the path does not exist, consider it as a new file
            self.row = vec![];
            self.modified = true;
        }
        self.lang = Language::detect(path);
        self.hl = Highlighting::new(self.lang, self.row.iter());
        self.file = Some(FilePath::from(path));
        Ok(())
    }

    fn save(&mut self) -> io::Result<()> {
        let mut create = false;
        if self.file.is_none() {
            if let Some(input) =
                self.prompt("Save as: {} (^G or ESC to cancel)", |_, _, _, _| Ok(()))?
            {
                let file = FilePath::from_string(input);
                self.lang = Language::detect(&file.path);
                self.hl.lang_changed(self.lang);
                self.file = Some(file);
                create = true;
            }
        }

        let file = if let Some(file) = &self.file {
            file
        } else {
            return Ok(()); // Canceled
        };

        let f = match fs::File::create(&file.path) {
            Ok(f) => f,
            Err(e) => {
                self.screen
                    .set_error_message(format!("Could not save: {}", e));
                if create {
                    self.file = None; // Could not make file. Back to unnamed buffer
                }
                return Ok(()); // This is not a fatal error
            }
        };
        let mut f = io::BufWriter::new(f);
        let mut bytes = 0;
        for line in self.row.iter() {
            let b = line.buffer();
            write!(f, "{}\n", b)?;
            bytes += b.as_bytes().len() + 1;
        }
        f.flush()?;

        self.screen
            .set_info_message(format!("{} bytes written to {}", bytes, &file.display));
        self.modified = false;
        Ok(())
    }

    fn on_incremental_find(&mut self, query: &str, seq: InputSeq, end: bool) -> io::Result<()> {
        use KeySeq::*;

        if self.finding.last_match.is_some() {
            if let Some(matched_line) = self.hl.clear_previous_match() {
                self.screen.set_dirty_start(matched_line);
            }
        }

        if end {
            return Ok(());
        }

        match (seq.key, seq.ctrl, seq.alt) {
            (RightKey, ..) | (DownKey, ..) | (Key(b'f'), true, ..) | (Key(b'n'), true, ..) => {
                self.finding.dir = FindDir::Forward
            }
            (LeftKey, ..) | (UpKey, ..) | (Key(b'b'), true, ..) | (Key(b'p'), true, ..) => {
                self.finding.dir = FindDir::Back
            }
            _ => self.finding = FindState::new(),
        }

        fn next_line(y: usize, dir: FindDir, len: usize) -> usize {
            // Wrapping text search at top/bottom of text buffer
            match dir {
                FindDir::Forward if y == len - 1 => 0,
                FindDir::Forward => y + 1,
                FindDir::Back if y == 0 => len - 1,
                FindDir::Back => y - 1,
            }
        }

        let row_len = self.row.len();
        let dir = self.finding.dir;
        let mut y = self
            .finding
            .last_match
            .map(|y| next_line(y, dir, row_len)) // Start from next line on moving to next match
            .unwrap_or(self.cy);

        for _ in 0..row_len {
            if let Some(byte_idx) = self.row[y].render.find(query) {
                let rx = self.row[y].render[..byte_idx].chars().count();
                // XXX: This searches render text, not actual buffer. So it may not work properly on
                // character which is rendered differently (e.g. tab character)
                self.cy = y;
                self.cx = self.row[y].cx_from_rx(rx);
                // Cause do_scroll() to scroll upwards to the matching line at next screen redraw
                self.screen.rowoff = row_len;
                self.finding.last_match = Some(y);
                // This refresh is necessary because highlight must be updated before saving highlights
                // of matched region
                self.refresh_screen()?;
                // Set match highlight on the found line
                self.hl.set_match(y, rx, rx + query.chars().count());
                self.screen.set_dirty_start(y);
                break;
            }
            y = next_line(y, dir, row_len);
        }

        Ok(())
    }

    fn find(&mut self) -> io::Result<()> {
        let (cx, cy, coloff, rowoff) = (self.cx, self.cy, self.screen.coloff, self.screen.rowoff);
        let s = "Search: {} (^F or RIGHT to forward, ^B or LEFT to back, ^G or ESC to cancel)";
        if self.prompt(s, Self::on_incremental_find)?.is_none() {
            // Canceled. Restore cursor position
            self.cx = cx;
            self.cy = cy;
            self.screen.coloff = coloff;
            self.screen.rowoff = rowoff;
            self.screen.set_dirty_start(self.screen.rowoff); // Redraw all lines
        } else {
            if self.finding.last_match.is_some() {
                self.screen.set_info_message("Found");
            } else {
                self.screen.set_error_message("Not Found");
            }
        }

        self.finding = FindState::new(); // Clear text search state for next time
        Ok(())
    }

    fn show_help(&mut self) -> io::Result<()> {
        self.screen.draw_help()?;

        // Consume any key
        while let Some(seq) = self.input.next() {
            if seq?.key != KeySeq::Unidentified {
                break;
            }
        }

        // Redraw screen
        self.screen.set_dirty_start(self.screen.rowoff);
        Ok(())
    }

    fn insert_char(&mut self, ch: char) {
        if self.cy == self.row.len() {
            self.row.push(Row::default());
        }
        self.row[self.cy].insert_char(self.cx, ch);
        self.cx += 1;
        self.modified = true;
        self.hl.needs_update = true;
        self.screen.set_dirty_start(self.cy);
    }

    fn insert_tab(&mut self) {
        match self.lang.indent() {
            Indent::AsIs => self.insert_char('\t'),
            Indent::Fixed(indent) => self.insert_str(indent),
        }
    }

    fn insert_str<S: AsRef<str>>(&mut self, s: S) {
        if self.cy == self.row.len() {
            self.row.push(Row::default());
        }
        let s = s.as_ref();
        self.row[self.cy].insert_str(self.cx, s);
        self.cx += s.as_bytes().len();
        self.modified = true;
        self.hl.needs_update = true;
        self.screen.set_dirty_start(self.cy);
    }

    fn squash_to_previous_line(&mut self) {
        // At top of line, backspace concats current line to previous line
        self.cx = self.row[self.cy - 1].len(); // Move cursor column to end of previous line
        let row = self.row.remove(self.cy);
        self.cy -= 1; // Move cursor to previous line
        self.row[self.cy].append(row.buffer()); // TODO: Move buffer rather than copy
        self.modified = true;
        self.hl.needs_update = true;
        self.screen.set_dirty_start(self.cy);
    }

    fn delete_char(&mut self) {
        if self.cy == self.row.len() || self.cx == 0 && self.cy == 0 {
            return;
        }
        if self.cx > 0 {
            self.row[self.cy].delete_char(self.cx - 1);
            self.cx -= 1;
            self.modified = true;
            self.hl.needs_update = true;
            self.screen.set_dirty_start(self.cy);
        } else {
            self.squash_to_previous_line();
        }
    }

    fn delete_until_end_of_line(&mut self) {
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
        self.hl.needs_update = true;
        self.screen.set_dirty_start(self.cy);
    }

    fn delete_until_head_of_line(&mut self) {
        if self.cx == 0 && self.cy == 0 || self.cy == self.row.len() {
            return;
        }
        if self.cx == 0 {
            self.squash_to_previous_line();
        } else {
            self.row[self.cy].remove(0, self.cx);
            self.cx = 0;
            self.modified = true;
            self.hl.needs_update = true;
            self.screen.set_dirty_start(self.cy);
        }
    }

    fn delete_word(&mut self) {
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
            self.hl.needs_update = true;
            self.screen.set_dirty_start(self.cy);
        }
    }

    fn delete_right_char(&mut self) {
        self.move_cursor_one(CursorDir::Right);
        self.delete_char();
    }

    fn insert_line(&mut self) {
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
        self.hl.needs_update = true;
        self.screen.set_dirty_start(self.cy);
    }

    fn move_cursor_one(&mut self, dir: CursorDir) {
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

    fn move_cursor_per_page(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Up => {
                self.cy = self.screen.rowoff; // Set cursor to top of screen
                for _ in 0..self.screen.rows() {
                    self.move_cursor_one(CursorDir::Up);
                }
            }
            CursorDir::Down => {
                // Set cursor to bottom of screen considering end of buffer
                self.cy = cmp::min(self.screen.rowoff + self.screen.rows() - 1, self.row.len());
                for _ in 0..self.screen.rows() {
                    self.move_cursor_one(CursorDir::Down)
                }
            }
            _ => unreachable!(),
        }
    }

    fn move_cursor_to_buffer_edge(&mut self, dir: CursorDir) {
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

    fn move_cursor_by_word(&mut self, dir: CursorDir) {
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

    fn prompt<S, F>(&mut self, prompt: S, mut incremental_callback: F) -> io::Result<Option<String>>
    where
        S: AsRef<str>,
        F: FnMut(&mut Self, &str, InputSeq, bool) -> io::Result<()>,
    {
        let mut buf = String::new();
        let mut canceled = false;
        let prompt = prompt.as_ref();
        self.screen.set_info_message(prompt.replacen("{}", "", 1));
        self.refresh_screen()?;

        while let Some(seq) = self.input.next() {
            use KeySeq::*;

            let seq = seq?;
            let mut finished = false;
            match (&seq.key, seq.ctrl, seq.alt) {
                (&Unidentified, ..) => continue,
                (&Key(b'h'), true, ..) | (&Key(0x7f), ..) | (&DeleteKey, ..) if !buf.is_empty() => {
                    buf.pop();
                }
                (&Key(b'g'), true, ..) | (&Key(b'q'), true, ..) | (&Key(0x1b), ..) => {
                    finished = true;
                    canceled = true;
                }
                (&Key(b'\r'), ..) | (&Key(b'm'), true, ..) => {
                    finished = true;
                }
                (&Key(b), false, ..) => buf.push(b as char),
                (&Utf8Key(c), false, ..) => buf.push(c),
                _ => {}
            }

            incremental_callback(self, buf.as_str(), seq, finished)?;
            if finished {
                break;
            }
            self.screen.set_info_message(prompt.replacen("{}", &buf, 1));
            self.refresh_screen()?;
        }

        self.screen
            .set_info_message(if canceled { "" } else { "Canceled" });
        self.refresh_screen()?;

        Ok(if canceled || buf.is_empty() {
            None
        } else {
            Some(buf)
        })
    }

    fn handle_quit(&mut self) -> io::Result<AfterKeyPress> {
        if !self.modified || self.quitting {
            Ok(AfterKeyPress::Quit)
        } else {
            self.quitting = true;
            self.screen.set_error_message(
                "File has unsaved changes! Press ^Q again to quit or ^S to save",
            );
            Ok(AfterKeyPress::Nothing)
        }
    }

    fn process_keypress(&mut self, s: InputSeq) -> io::Result<AfterKeyPress> {
        use KeySeq::*;

        match (s.key, s.ctrl, s.alt) {
            (Key(b'p'), true, false) => self.move_cursor_one(CursorDir::Up),
            (Key(b'b'), true, false) => self.move_cursor_one(CursorDir::Left),
            (Key(b'n'), true, false) => self.move_cursor_one(CursorDir::Down),
            (Key(b'f'), true, false) => self.move_cursor_one(CursorDir::Right),
            (Key(b'v'), true, false) => self.move_cursor_per_page(CursorDir::Down),
            (Key(b'a'), true, false) => self.move_cursor_to_buffer_edge(CursorDir::Left),
            (Key(b'e'), true, false) => self.move_cursor_to_buffer_edge(CursorDir::Right),
            (Key(b'd'), true, false) => self.delete_right_char(),
            (Key(b'g'), true, false) => self.find()?,
            (Key(b'h'), true, false) => self.delete_char(),
            (Key(b'k'), true, false) => self.delete_until_end_of_line(),
            (Key(b'u'), true, false) => self.delete_until_head_of_line(),
            (Key(b'w'), true, false) => self.delete_word(),
            (Key(b'l'), true, false) => self.screen.set_dirty_start(self.screen.rowoff), // Clear
            (Key(b's'), true, false) => self.save()?,
            (Key(b'i'), true, false) => self.insert_tab(),
            (Key(b'm'), true, false) => self.insert_line(),
            (Key(b'?'), true, false) => self.show_help()?,
            (Key(b'v'), false, true) => self.move_cursor_per_page(CursorDir::Up),
            (Key(b'f'), false, true) => self.move_cursor_by_word(CursorDir::Right),
            (Key(b'b'), false, true) => self.move_cursor_by_word(CursorDir::Left),
            (Key(b'<'), false, true) => self.move_cursor_to_buffer_edge(CursorDir::Up),
            (Key(b'>'), false, true) => self.move_cursor_to_buffer_edge(CursorDir::Down),
            (Key(0x08), false, false) => self.delete_char(), // Backspace
            (Key(0x7f), false, false) => self.delete_char(), // Delete key is mapped to \x1b[3~
            (Key(0x1b), false, false) => self.screen.set_dirty_start(self.screen.rowoff), // Clear on ESC
            (Key(b'\r'), false, false) => self.insert_line(),
            (Key(byte), false, false) if !byte.is_ascii_control() => self.insert_char(byte as char),
            (Key(b'q'), true, ..) => return self.handle_quit(),
            (UpKey, false, false) => self.move_cursor_one(CursorDir::Up),
            (LeftKey, false, false) => self.move_cursor_one(CursorDir::Left),
            (DownKey, false, false) => self.move_cursor_one(CursorDir::Down),
            (RightKey, false, false) => self.move_cursor_one(CursorDir::Right),
            (PageUpKey, false, false) => self.move_cursor_per_page(CursorDir::Up),
            (PageDownKey, false, false) => self.move_cursor_per_page(CursorDir::Down),
            (HomeKey, false, false) => self.move_cursor_to_buffer_edge(CursorDir::Left),
            (EndKey, false, false) => self.move_cursor_to_buffer_edge(CursorDir::Right),
            (DeleteKey, false, false) => self.delete_right_char(),
            (LeftKey, true, false) => self.move_cursor_by_word(CursorDir::Left),
            (RightKey, true, false) => self.move_cursor_by_word(CursorDir::Right),
            (LeftKey, false, true) => self.move_cursor_to_buffer_edge(CursorDir::Left),
            (RightKey, false, true) => self.move_cursor_to_buffer_edge(CursorDir::Right),
            (Utf8Key(c), ..) => self.insert_char(c),
            (Unidentified, ..) => unreachable!(),
            (Cursor(_, _), ..) => unreachable!(),
            (key, ctrl, alt) => {
                let m = match (ctrl, alt) {
                    (true, true) => "C-M-",
                    (true, false) => "C-",
                    (false, true) => "M-",
                    (false, false) => "",
                };
                self.screen
                    .set_error_message(format!("Key '{}{}' not mapped", m, key));
            }
        }

        self.quitting = false;
        Ok(AfterKeyPress::Nothing)
    }

    pub fn run(&mut self) -> io::Result<()> {
        self.refresh_screen()?;

        while let Some(seq) = self.input.next() {
            let seq = seq?;
            if seq.key == KeySeq::Unidentified {
                continue; // Ignore
            }
            if self.process_keypress(seq)? == AfterKeyPress::Quit {
                break;
            }
            self.refresh_screen()?; // Update screen after keypress
        }

        self.screen.clear() // Finally clear screen on exit
    }
}
