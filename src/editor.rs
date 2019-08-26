use crate::highlight::Highlighting;
use crate::input::{InputSeq, KeySeq};
use crate::language::Language;
use crate::screen::Screen;
use crate::text_buffer::{CursorDir, Lines, TextBuffer};
use std::io;
use std::path::Path;
use std::str;

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

#[derive(PartialEq)]
enum AfterKeyPress {
    Quit,
    Refresh,
    DoNothing,
}

pub struct Editor<I: Iterator<Item = io::Result<InputSeq>>> {
    input: I,           // Escape sequences stream represented as Iterator
    quitting: bool,     // After first Ctrl-Q
    finding: FindState, // Text search state
    hl: Highlighting,
    screen: Screen,
    buf: TextBuffer,
}

impl<I: Iterator<Item = io::Result<InputSeq>>> Editor<I> {
    pub fn new(mut input: I) -> io::Result<Editor<I>> {
        let screen = Screen::new(&mut input)?;
        Ok(Editor {
            input,
            quitting: false,
            finding: FindState::new(),
            hl: Highlighting::default(),
            screen,
            buf: TextBuffer::new(),
        })
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        self.screen.refresh(
            self.buf.rows(),
            self.buf.filename(),
            self.buf.modified(),
            self.buf.lang().name(),
            (self.buf.cx(), self.buf.cy()),
            &mut self.hl,
        )
    }

    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        self.buf = TextBuffer::open(path)?;
        self.hl = Highlighting::new(self.buf.lang(), self.buf.rows().iter()); // TODO: Use &[Row] instead of Iterator<Row>
        Ok(())
    }

    fn save(&mut self) -> io::Result<()> {
        let mut create = false;
        if !self.buf.has_file() {
            if let Some(input) =
                self.prompt("Save as: {} (^G or ESC to cancel)", |_, _, _, _| Ok(()))?
            {
                let prev_lang = self.buf.lang();
                self.buf.set_file(input);
                self.hl.lang_changed(self.buf.lang());
                if prev_lang != self.buf.lang() {
                    // Render entire screen since highglight updated
                    self.screen.set_dirty_start(self.screen.rowoff);
                }
                create = true;
            }
        }

        match self.buf.save() {
            Ok(msg) => self.screen.set_info_message(msg),
            Err(msg) => {
                self.screen.set_error_message(msg);
                if create {
                    self.buf.set_unnamed();
                }
            }
        }

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

        match (seq.key, seq.ctrl) {
            (RightKey, ..) | (DownKey, ..) | (Key(b'f'), true) | (Key(b'n'), true) => {
                self.finding.dir = FindDir::Forward
            }
            (LeftKey, ..) | (UpKey, ..) | (Key(b'b'), true) | (Key(b'p'), true) => {
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

        let row_len = self.buf.rows().len();
        let dir = self.finding.dir;
        let mut y = self
            .finding
            .last_match
            .map(|y| next_line(y, dir, row_len)) // Start from next line on moving to next match
            .unwrap_or_else(|| self.buf.cy());

        for _ in 0..row_len {
            let row = &self.buf.rows()[y];
            if let Some(byte_idx) = row.buffer().find(query) {
                let idx = row.char_idx_of(byte_idx);
                self.buf.set_cursor(idx, y);

                let row = &self.buf.rows()[y]; // Immutable borrow again since self.buf.set_cursor() yields mutable borrow
                let rx = row.rx_from_cx(self.buf.cx());
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
        let (cx, cy, coloff, rowoff) = (
            self.buf.cx(),
            self.buf.cy(),
            self.screen.coloff,
            self.screen.rowoff,
        );
        let s = "Search: {} (^F or RIGHT to forward, ^B or LEFT to back, ^G or ESC to cancel)";
        if self.prompt(s, Self::on_incremental_find)?.is_none() {
            // Canceled. Restore cursor position
            self.buf.set_cursor(cx, cy);
            self.screen.coloff = coloff;
            self.screen.rowoff = rowoff;
            self.screen.set_dirty_start(self.screen.rowoff); // Redraw all lines
        } else if self.finding.last_match.is_some() {
            self.screen.set_info_message("Found");
        } else {
            self.screen.set_error_message("Not Found");
        }

        self.finding = FindState::new(); // Clear text search state for next time
        Ok(())
    }

    fn show_help(&mut self) -> io::Result<()> {
        self.screen.draw_help()?;

        // Consume any key
        while let Some(seq) = self.input.next() {
            if self.screen.maybe_resize(&mut self.input)? {
                // XXX: Status bar is not redrawn
                self.screen.draw_help()?;
            }
            if seq?.key != KeySeq::Unidentified {
                break;
            }
        }

        // Redraw screen
        self.screen.set_dirty_start(self.screen.rowoff);
        Ok(())
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

            if self.screen.maybe_resize(&mut self.input)? {
                self.refresh_screen()?;
            }

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
        if !self.buf.modified() || self.quitting {
            Ok(AfterKeyPress::Quit)
        } else {
            self.quitting = true;
            self.screen.set_error_message(
                "File has unsaved changes! Press ^Q again to quit or ^S to save",
            );
            Ok(AfterKeyPress::Refresh)
        }
    }

    fn process_keypress(&mut self, s: InputSeq) -> io::Result<AfterKeyPress> {
        use KeySeq::*;

        let rowoff = self.screen.rowoff;
        let rows = self.screen.rows();
        self.buf.dirty = false;

        match (s.key, s.ctrl, s.alt) {
            (Unidentified, ..) => return Ok(AfterKeyPress::DoNothing),
            (Key(b'p'), true, false) => self.buf.move_cursor_one(CursorDir::Up),
            (Key(b'b'), true, false) => self.buf.move_cursor_one(CursorDir::Left),
            (Key(b'n'), true, false) => self.buf.move_cursor_one(CursorDir::Down),
            (Key(b'f'), true, false) => self.buf.move_cursor_one(CursorDir::Right),
            (Key(b'v'), true, false) => self.buf.move_cursor_page(CursorDir::Down, rowoff, rows),
            (Key(b'a'), true, false) => self.buf.move_cursor_to_buffer_edge(CursorDir::Left),
            (Key(b'e'), true, false) => self.buf.move_cursor_to_buffer_edge(CursorDir::Right),
            (Key(b'd'), true, false) => self.buf.delete_right_char(),
            (Key(b'g'), true, false) => self.find()?,
            (Key(b'h'), true, false) => self.buf.delete_char(),
            (Key(b'k'), true, false) => self.buf.delete_until_end_of_line(),
            (Key(b'u'), true, false) => self.buf.delete_until_head_of_line(),
            (Key(b'w'), true, false) => self.buf.delete_word(),
            (Key(b'l'), true, false) => self.screen.set_dirty_start(self.screen.rowoff), // Clear
            (Key(b's'), true, false) => self.save()?,
            (Key(b'i'), true, false) => self.buf.insert_tab(),
            (Key(b'm'), true, false) => self.buf.insert_line(),
            (Key(b'?'), true, false) => self.show_help()?,
            (Key(0x1b), false, false) => self.buf.move_cursor_page(CursorDir::Up, rowoff, rows), // Clash with Ctrl-[
            (Key(b']'), true, false) => self.buf.move_cursor_page(CursorDir::Down, rowoff, rows),
            (Key(b'v'), false, true) => self.buf.move_cursor_page(CursorDir::Up, rowoff, rows),
            (Key(b'f'), false, true) => self.buf.move_cursor_by_word(CursorDir::Right),
            (Key(b'b'), false, true) => self.buf.move_cursor_by_word(CursorDir::Left),
            (Key(b'n'), false, true) => self.buf.move_cursor_paragraph(CursorDir::Down),
            (Key(b'p'), false, true) => self.buf.move_cursor_paragraph(CursorDir::Up),
            (Key(b'<'), false, true) => self.buf.move_cursor_to_buffer_edge(CursorDir::Up),
            (Key(b'>'), false, true) => self.buf.move_cursor_to_buffer_edge(CursorDir::Down),
            (Key(0x08), false, false) => self.buf.delete_char(), // Backspace
            (Key(0x7f), false, false) => self.buf.delete_char(), // Delete key is mapped to \x1b[3~
            (Key(b'\r'), false, false) => self.buf.insert_line(),
            (Key(b), false, false) if !b.is_ascii_control() => self.buf.insert_char(b as char),
            (Key(b'q'), true, ..) => return self.handle_quit(),
            (UpKey, false, false) => self.buf.move_cursor_one(CursorDir::Up),
            (LeftKey, false, false) => self.buf.move_cursor_one(CursorDir::Left),
            (DownKey, false, false) => self.buf.move_cursor_one(CursorDir::Down),
            (RightKey, false, false) => self.buf.move_cursor_one(CursorDir::Right),
            (PageUpKey, false, false) => self.buf.move_cursor_page(CursorDir::Up, rowoff, rows),
            (PageDownKey, false, false) => self.buf.move_cursor_page(CursorDir::Down, rowoff, rows),
            (HomeKey, false, false) => self.buf.move_cursor_to_buffer_edge(CursorDir::Left),
            (EndKey, false, false) => self.buf.move_cursor_to_buffer_edge(CursorDir::Right),
            (DeleteKey, false, false) => self.buf.delete_right_char(),
            (LeftKey, true, false) => self.buf.move_cursor_by_word(CursorDir::Left),
            (RightKey, true, false) => self.buf.move_cursor_by_word(CursorDir::Right),
            (DownKey, true, false) => self.buf.move_cursor_paragraph(CursorDir::Down),
            (UpKey, true, false) => self.buf.move_cursor_paragraph(CursorDir::Up),
            (LeftKey, false, true) => self.buf.move_cursor_to_buffer_edge(CursorDir::Left),
            (RightKey, false, true) => self.buf.move_cursor_to_buffer_edge(CursorDir::Right),
            (Utf8Key(c), ..) => self.buf.insert_char(c),
            (Cursor(_, _), ..) => unreachable!(),
            (key, ctrl, alt) => {
                let modifier = match (ctrl, alt) {
                    (true, true) => "C-M-",
                    (true, false) => "C-",
                    (false, true) => "M-",
                    (false, false) => "",
                };
                let msg = format!("Key '{}{}' not mapped", modifier, key);
                self.screen.set_error_message(msg);
            }
        }

        if self.buf.dirty {
            self.hl.needs_update = true;
            self.screen.set_dirty_start(self.buf.cy());
        }
        self.quitting = false;
        Ok(AfterKeyPress::Refresh)
    }

    pub fn edit(&mut self) -> io::Result<()> {
        self.refresh_screen()?; // First paint

        while let Some(seq) = self.input.next() {
            if self.screen.maybe_resize(&mut self.input)? {
                self.refresh_screen()?;
            }

            match self.process_keypress(seq?)? {
                AfterKeyPress::DoNothing => continue,
                AfterKeyPress::Refresh => self.refresh_screen()?,
                AfterKeyPress::Quit => break,
            }
        }

        self.screen.clear() // Finally clear screen on exit
    }

    pub fn lines(&self) -> Lines<'_> {
        self.buf.lines()
    }

    pub fn screen(&self) -> &'_ Screen {
        &self.screen
    }

    pub fn lang(&self) -> Language {
        self.buf.lang()
    }
}
