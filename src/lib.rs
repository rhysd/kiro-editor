// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

mod ansi_color;
mod highlight;
mod input;
mod language;
mod row;

use ansi_color::AnsiColor;
use highlight::Highlighting;
pub use input::StdinRawMode;
use input::{InputSeq, KeySeq};
use language::{Indent, Language};
use row::Row;
use std::cmp;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::str;
use std::time::SystemTime;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const HELP_TEXT: &'static str = "HELP: ^S = save | ^Q = quit | ^G = find | ^? = help";

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

#[derive(PartialEq)]
enum StatusMessageKind {
    Info,
    Error,
}

struct StatusMessage {
    text: String,
    timestamp: SystemTime,
    kind: StatusMessageKind,
}

impl StatusMessage {
    fn info<S: Into<String>>(message: S) -> StatusMessage {
        StatusMessage::with_kind(message, StatusMessageKind::Info)
    }

    fn error<S: Into<String>>(message: S) -> StatusMessage {
        StatusMessage::with_kind(message, StatusMessageKind::Error)
    }

    fn with_kind<S: Into<String>>(message: S, kind: StatusMessageKind) -> StatusMessage {
        StatusMessage {
            text: message.into(),
            timestamp: SystemTime::now(),
            kind,
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
    // (x, y) coordinate in `render` text of rows
    rx: usize,
    // Screen size
    screen_rows: usize,
    screen_cols: usize,
    // Lines of text buffer
    row: Vec<Row>,
    // Scroll position (row/col offset)
    rowoff: usize,
    coloff: usize,
    // Message in status line
    message: StatusMessage,
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
}

impl<I: Iterator<Item = io::Result<InputSeq>>> Editor<I> {
    pub fn new(window_size: Option<(usize, usize)>, input: I) -> Editor<I> {
        let (w, h) = window_size.unwrap_or((0, 0));
        Editor {
            input,
            file: None,
            cx: 0,
            cy: 0,
            rx: 0,
            screen_cols: w,
            // Screen height is 1 line less than window height due to status bar
            screen_rows: h.saturating_sub(2),
            row: Vec::with_capacity(h),
            rowoff: 0,
            coloff: 0,
            message: StatusMessage::info(HELP_TEXT),
            modified: false,
            quitting: false,
            finding: FindState::new(),
            lang: Language::Plain,
            hl: Highlighting::default(),
        }
    }

    fn trim_line<'a, S: AsRef<str>>(&self, line: &'a S) -> &'a str {
        let mut line = line.as_ref();
        if line.len() <= self.coloff {
            return "";
        }
        if self.coloff > 0 {
            line = &line[self.coloff..];
        }
        if line.len() > self.screen_cols {
            line = &line[..self.screen_cols]
        }
        line
    }

    fn draw_status_bar<W: Write>(&self, mut buf: W) -> io::Result<()> {
        write!(buf, "\x1b[{}H", self.screen_rows + 1)?;

        buf.write(AnsiColor::Invert.sequence())?;

        let file = if let Some(ref f) = self.file {
            f.display.as_str()
        } else {
            "[No Name]"
        };

        let modified = if self.modified { "(modified) " } else { "" };
        let left = format!("{:<20?} - {} lines {}", file, self.row.len(), modified);
        let left = &left[..cmp::min(left.len(), self.screen_cols)];
        buf.write(left.as_bytes())?; // Left of status bar

        let rest_len = self.screen_cols - left.len();
        if rest_len == 0 {
            return Ok(());
        }

        let right = format!("{} {}/{}", self.lang.name(), self.cy, self.row.len(),);
        if right.len() > rest_len {
            for _ in 0..rest_len {
                buf.write(b" ")?;
            }
            return Ok(());
        }

        for _ in 0..rest_len - right.len() {
            buf.write(b" ")?; // Add spaces at center of status bar
        }
        buf.write(right.as_bytes())?;

        // Defualt argument of 'm' command is 0 so it resets attributes
        buf.write(AnsiColor::Reset.sequence())?;
        Ok(())
    }

    fn draw_message_bar<W: Write>(&self, mut buf: W) -> io::Result<()> {
        write!(buf, "\x1b[{}H", self.screen_rows + 2)?;
        if let Ok(d) = SystemTime::now().duration_since(self.message.timestamp) {
            if d.as_secs() < 5 {
                let msg = &self.message.text[..cmp::min(self.message.text.len(), self.screen_cols)];
                if self.message.kind == StatusMessageKind::Error {
                    buf.write(AnsiColor::RedBG.sequence())?;
                    buf.write(msg.as_bytes())?;
                    buf.write(AnsiColor::Reset.sequence())?;
                } else {
                    buf.write(msg.as_bytes())?;
                }
            }
        }
        buf.write(b"\x1b[K")?;
        Ok(())
    }

    fn draw_welcome_message<W: Write>(&self, mut buf: W) -> io::Result<()> {
        let msg_buf = format!("Kilo editor -- version {}", VERSION);
        let welcome = self.trim_line(&msg_buf);
        let padding = (self.screen_cols - welcome.len()) / 2;
        if padding > 0 {
            buf.write(b"~")?;
            for _ in 0..padding - 1 {
                buf.write(b" ")?;
            }
        }
        buf.write(welcome.as_bytes())?;
        Ok(())
    }

    fn draw_rows<W: Write>(&self, mut buf: W) -> io::Result<()> {
        let mut prev_color = AnsiColor::Reset;
        let row_len = self.row.len();

        for y in 0..self.screen_rows {
            let file_row = y + self.rowoff;

            if file_row < row_len && !self.row[file_row].dirty {
                continue;
            }

            // Move cursor to target line
            write!(buf, "\x1b[{}H", y + 1)?;

            if file_row >= row_len {
                if self.row.is_empty() && y == self.screen_rows / 3 {
                    self.draw_welcome_message(&mut buf)?;
                } else {
                    if prev_color != AnsiColor::Reset {
                        buf.write(AnsiColor::Reset.sequence())?;
                        prev_color = AnsiColor::Reset;
                    }
                    buf.write(b"~")?;
                }
            } else {
                // TODO: Support UTF-8
                let row = &self.row[file_row];

                for (b, hl) in row
                    .render
                    .as_bytes()
                    .iter()
                    .cloned()
                    .zip(self.hl.lines[file_row].iter())
                    .skip(self.coloff)
                    .take(self.screen_cols)
                {
                    let color = hl.color();
                    if color != prev_color {
                        buf.write(color.sequence())?;
                        prev_color = color;
                    }
                    buf.write(&[b])?;
                }
            }

            // Erases the part of the line to the right of the cursor. http://vt100.net/docs/vt100-ug/chapter3.html#EL
            buf.write(b"\x1b[K")?;
        }

        if prev_color != AnsiColor::Reset {
            buf.write(AnsiColor::Reset.sequence())?; // Ensure to reset color at end of screen
        }

        Ok(())
    }

    fn redraw_screen(&self) -> io::Result<()> {
        let mut buf = Vec::with_capacity((self.screen_rows + 1) * self.screen_cols);

        // \x1b[: Escape sequence header
        // Hide cursor while updating screen. 'l' is command to set mode http://vt100.net/docs/vt100-ug/chapter3.html#SM
        buf.write(b"\x1b[?25l")?;
        // H: Command to move cursor. Here \x1b[H is the same as \x1b[1;1H
        buf.write(b"\x1b[H")?;

        self.draw_rows(&mut buf)?;
        self.draw_status_bar(&mut buf)?;
        self.draw_message_bar(&mut buf)?;

        // Move cursor
        let cursor_row = self.cy - self.rowoff + 1;
        let cursor_col = self.rx - self.coloff + 1;
        write!(buf, "\x1b[{};{}H", cursor_row, cursor_col)?;

        // Reveal cursor again. 'h' is command to reset mode https://vt100.net/docs/vt100-ug/chapter3.html#RM
        buf.write(b"\x1b[?25h")?;

        let mut stdout = io::stdout();
        stdout.write(&buf)?;
        stdout.flush()
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        self.do_scroll();
        self.hl.update(&self.row, self.rowoff + self.screen_rows);
        self.redraw_screen()?;

        for row in self.row.iter_mut().skip(self.rowoff).take(self.screen_rows) {
            row.dirty = false; // Rendered the row. It's no longer dirty
        }

        Ok(())
    }

    fn clear_screen(&self) -> io::Result<()> {
        let mut stdout = io::stdout();
        // 2: Argument of 'J' command to reset entire screen
        // J: Command to erase screen http://vt100.net/docs/vt100-ug/chapter3.html#ED
        stdout.write(b"\x1b[2J")?;
        // Set cursor position to left-top corner
        stdout.write(b"\x1b[H")?;
        stdout.flush()
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

        let ref file = if let Some(ref file) = self.file {
            file
        } else {
            return Ok(()); // Canceled
        };

        let f = match fs::File::create(&file.path) {
            Ok(f) => f,
            Err(e) => {
                self.message = StatusMessage::error(format!("Could not save: {}", e));
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
            f.write(b)?;
            f.write(b"\n")?;
            bytes += b.len() + 1;
        }
        f.flush()?;

        self.message = StatusMessage::info(format!("{} bytes written to {}", bytes, &file.display));
        self.modified = false;
        Ok(())
    }

    fn on_incremental_find(&mut self, query: &str, seq: InputSeq, end: bool) -> io::Result<()> {
        use KeySeq::*;

        if self.finding.last_match.is_some() {
            if let Some(y) = self.hl.clear_previous_match() {
                self.row[y].dirty = true;
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
            if let Some(rx) = self.row[y].render.find(query) {
                // XXX: This searches render text, not actual buffer. So it may not work properly on
                // character which is rendered differently (e.g. tab character)
                self.cy = y;
                self.cx = self.row[y].cx_from_rx(rx);
                // Cause do_scroll() to scroll upwards to the matching line at next screen redraw
                self.rowoff = row_len;
                self.finding.last_match = Some(y);
                // This refresh is necessary because highlight must be updated before saving highlights
                // of matched region
                self.refresh_screen()?;
                // Set match highlight on the found line
                self.hl.set_match(y, rx, rx + query.as_bytes().len());
                self.row[y].dirty = true;
                break;
            }
            y = next_line(y, dir, row_len);
        }

        Ok(())
    }

    fn find(&mut self) -> io::Result<()> {
        let (cx, cy, coloff, rowoff) = (self.cx, self.cy, self.coloff, self.rowoff);
        let s = "Search: {} (^F or RIGHT to forward, ^B or LEFT to back, ^G or ESC to cancel)";
        if self.prompt(s, Self::on_incremental_find)?.is_none() {
            // Canceled. Restore cursor position
            self.cx = cx;
            self.cy = cy;
            self.coloff = coloff;
            self.rowoff = rowoff;
            self.set_dirty_rows(self.rowoff); // Redraw all lines
        } else {
            self.message = if self.finding.last_match.is_some() {
                StatusMessage::info("Found")
            } else {
                StatusMessage::error("Not Found")
            };
        }

        self.finding = FindState::new(); // Clear text search state for next time
        Ok(())
    }

    fn set_dirty_rows(&mut self, start: usize) {
        for row in self.row.iter_mut().skip(start).take(self.screen_rows) {
            row.dirty = true;
        }
    }

    fn do_scroll(&mut self) {
        let prev_rowoff = self.rowoff;
        let prev_coloff = self.coloff;

        // Calculate X coordinate to render considering tab stop
        if self.cy < self.row.len() {
            self.rx = self.row[self.cy].rx_from_cx(self.cx);
        } else {
            self.rx = 0;
        }

        // Adjust scroll position when cursor is outside screen
        if self.cy < self.rowoff {
            // Scroll up when cursor is above the top of window
            self.rowoff = self.cy;
        }
        if self.cy >= self.rowoff + self.screen_rows {
            // Scroll down when cursor is below the bottom of screen
            self.rowoff = self.cy - self.screen_rows + 1;
        }
        if self.rx < self.coloff {
            self.coloff = self.rx;
        }
        if self.rx >= self.coloff + self.screen_cols {
            self.coloff = self.rx - self.screen_cols + 1;
        }

        if prev_rowoff != self.rowoff || prev_coloff != self.coloff {
            // If scroll happens, all rows on screen must be updated
            self.set_dirty_rows(self.rowoff);
        }
    }

    fn insert_char(&mut self, ch: char) {
        if self.cy == self.row.len() {
            self.row.push(Row::default());
        }
        self.row[self.cy].insert_char(self.cx, ch);
        self.cx += 1;
        self.modified = true;
        self.hl.needs_update = true;
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
    }

    fn squash_to_previous_line(&mut self) {
        // At top of line, backspace concats current line to previous line
        self.cx = self.row[self.cy - 1].buffer().len(); // Move cursor column to end of previous line
        let row = self.row.remove(self.cy);
        self.cy -= 1; // Move cursor to previous line
        self.row[self.cy].append(row.buffer_str());
        self.modified = true;
        self.hl.needs_update = true;

        self.set_dirty_rows(self.cy + 1);
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
        } else {
            self.squash_to_previous_line();
        }
    }

    fn delete_until_end_of_line(&mut self) {
        if self.cy == self.row.len() {
            return;
        }
        if self.cx == self.row[self.cy].buffer().len() {
            // Do nothing when cursor is at end of line of end of text buffer
            if self.cy == self.row.len() - 1 {
                return;
            }
            // At end of line, concat with next line
            let deleted = self.row.remove(self.cy + 1);
            self.row[self.cy].append(deleted.buffer_str());
            self.set_dirty_rows(self.cy + 1);
        } else {
            self.row[self.cy].truncate(self.cx);
        }
        self.modified = true;
        self.hl.needs_update = true;
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
        }
    }

    fn delete_word(&mut self) {
        if self.cx == 0 || self.cy == self.row.len() {
            return;
        }

        let mut x = self.cx - 1;
        let buf = self.row[self.cy].buffer();
        while x > 0 && buf[x].is_ascii_whitespace() {
            x -= 1;
        }
        // `x - 1` since x should stop at the last non-whitespace character to remove
        while x > 0 && !buf[x - 1].is_ascii_whitespace() {
            x -= 1;
        }

        if x < self.cx {
            self.row[self.cy].remove(x, self.cx);
            self.cx = x;
            self.modified = true;
            self.hl.needs_update = true;
        }
    }

    fn delete_right_char(&mut self) {
        self.move_cursor_one(CursorDir::Right);
        self.delete_char();
    }

    fn insert_line(&mut self) {
        if self.cy >= self.row.len() {
            self.row.push(Row::new(""));
        } else if self.cx >= self.row[self.cy].buffer().len() {
            self.row.insert(self.cy + 1, Row::new(""));
        } else {
            let split = String::from(&self.row[self.cy].buffer_str()[self.cx..]);
            self.row[self.cy].truncate(self.cx);
            self.row.insert(self.cy + 1, Row::new(split));
        }

        self.cy += 1;
        self.cx = 0;
        self.hl.needs_update = true;

        self.set_dirty_rows(self.cy);
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
                    self.cx = self.row[self.cy].buffer().len();
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
                    let len = self.row[self.cy].buffer().len();
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
        let len = self.row.get(self.cy).map(|r| r.buffer().len()).unwrap_or(0);
        if self.cx > len {
            self.cx = len;
        }
    }

    fn move_cursor_per_page(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Up => {
                self.cy = self.rowoff; // Set cursor to top of screen
                for _ in 0..self.screen_rows {
                    self.move_cursor_one(CursorDir::Up);
                }
            }
            CursorDir::Down => {
                // Set cursor to bottom of screen considering end of buffer
                self.cy = cmp::min(self.rowoff + self.screen_rows - 1, self.row.len());
                for _ in 0..self.screen_rows {
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
                    self.cx = self.row[self.cy].buffer().len();
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
            fn new_at(rows: &Vec<Row>, x: usize, y: usize) -> Self {
                rows.get(y)
                    .and_then(|r| r.buffer().get(x))
                    .map(|b| {
                        if b.is_ascii_whitespace() {
                            CharKind::Space
                        } else if *b == b'_' || b.is_ascii_alphanumeric() {
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
        self.message = StatusMessage::info(prompt.replacen("{}", "", 1));
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
                (&Key(b), ..) => buf.push(b as char),
                _ => {}
            }

            incremental_callback(self, buf.as_str(), seq, finished)?;
            if finished {
                break;
            }
            self.message = StatusMessage::info(prompt.replacen("{}", &buf, 1));
            self.refresh_screen()?;
        }

        self.message = StatusMessage::info(if canceled { "" } else { "Canceled" });
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
            self.message = StatusMessage::error(
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
            (Key(b'l'), true, false) => self.set_dirty_rows(self.rowoff), // Clear
            (Key(b's'), true, false) => self.save()?,
            (Key(b'i'), true, false) => self.insert_tab(),
            (Key(b'?'), true, false) => self.message = StatusMessage::info(HELP_TEXT),
            (Key(b'v'), false, true) => self.move_cursor_per_page(CursorDir::Up),
            (Key(b'f'), false, true) => self.move_cursor_by_word(CursorDir::Right),
            (Key(b'b'), false, true) => self.move_cursor_by_word(CursorDir::Left),
            (Key(b'<'), false, true) => self.move_cursor_to_buffer_edge(CursorDir::Up),
            (Key(b'>'), false, true) => self.move_cursor_to_buffer_edge(CursorDir::Down),
            (Key(0x08), false, false) => self.delete_char(), // Backspace
            (Key(0x7f), false, false) => self.delete_char(), // Delete key is mapped to \x1b[3~
            (Key(0x1b), false, false) => self.set_dirty_rows(self.rowoff), // Clear on ESC
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
            (Unidentified, ..) => unreachable!(),
            (Cursor(_, _), ..) => unreachable!(),
            (key, ctrl, alt) => {
                let m = match (ctrl, alt) {
                    (true, true) => "C-M-",
                    (true, false) => "C-",
                    (false, true) => "M-",
                    (false, false) => "",
                };
                self.message = StatusMessage::error(format!("Key '{}{}' not mapped", m, key))
            }
        }

        self.quitting = false;
        Ok(AfterKeyPress::Nothing)
    }

    fn ensure_screen_size(&mut self) -> io::Result<()> {
        if self.screen_cols > 0 && self.screen_rows > 0 {
            return Ok(());
        }

        // By moving cursor at the bottom-right corner by 'B' and 'C' commands, get the size of
        // current screen. \x1b[9999;9999H is not available since it does not guarantee cursor
        // stops on the corner. Finaly command 'n' queries cursor position.
        let mut stdout = io::stdout();
        stdout.write(b"\x1b[9999C\x1b[9999B\x1b[6n")?;
        stdout.flush()?;

        // Wait for response from terminal discarding other sequences
        for seq in &mut self.input {
            if let KeySeq::Cursor(r, c) = seq?.key {
                self.screen_cols = c;
                self.screen_rows = r.saturating_sub(2);
                break;
            }
        }

        Ok(())
    }

    pub fn run(&mut self) -> io::Result<()> {
        self.ensure_screen_size()?;

        // Render first screen
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

        self.clear_screen() // Finally clear screen on exit
    }
}
