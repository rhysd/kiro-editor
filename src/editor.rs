use crate::highlight::Highlighting;
use crate::input::{InputSeq, KeySeq};
use crate::language::Language;
use crate::screen::Screen;
use crate::text_buffer::{CursorDir, Lines, TextBuffer};
use std::io::{self, Write};
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

pub struct Editor<I: Iterator<Item = io::Result<InputSeq>>, W: Write> {
    input: I,           // Escape sequences stream represented as Iterator
    quitting: bool,     // After first Ctrl-Q
    finding: FindState, // Text search state
    hl: Highlighting,
    screen: Screen<W>,
    bufs: Vec<TextBuffer>,
    buf_idx: usize,
}

impl<I, W> Editor<I, W>
where
    I: Iterator<Item = io::Result<InputSeq>>,
    W: Write,
{
    pub fn new(
        mut input: I,
        output: W,
        window_size: Option<(usize, usize)>,
    ) -> io::Result<Editor<I, W>> {
        let screen = Screen::new(window_size, &mut input, output)?;
        Ok(Editor {
            input,
            quitting: false,
            finding: FindState::new(),
            hl: Highlighting::default(),
            screen,
            bufs: vec![TextBuffer::default()],
            buf_idx: 0,
        })
    }

    pub fn open<P: AsRef<Path>>(
        mut input: I,
        output: W,
        window_size: Option<(usize, usize)>,
        paths: &[P],
    ) -> io::Result<Editor<I, W>> {
        if paths.is_empty() {
            return Self::new(input, output, window_size);
        }
        let screen = Screen::new(window_size, &mut input, output)?;
        let bufs: Vec<_> = paths
            .iter()
            .map(TextBuffer::open)
            .collect::<io::Result<_>>()?;
        let hl = Highlighting::new(bufs[0].lang(), bufs[0].rows());
        Ok(Editor {
            input,
            quitting: false,
            finding: FindState::new(),
            hl,
            screen,
            bufs,
            buf_idx: 0,
        })
    }

    fn buf(&self) -> &TextBuffer {
        &self.bufs[self.buf_idx]
    }

    fn buf_mut(&mut self) -> &mut TextBuffer {
        &mut self.bufs[self.buf_idx]
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        self.screen.refresh(
            &self.bufs[self.buf_idx],
            &mut self.hl,
            (self.buf_idx + 1, self.bufs.len()),
        )
    }

    fn reset_screen(&mut self) -> io::Result<()> {
        self.screen.set_dirty_start(0);
        self.screen.rowoff = 0;
        self.screen.coloff = 0;
        self.refresh_screen()
    }

    fn open_buffer(&mut self) -> io::Result<()> {
        if let Some(input) = self.prompt(
            "Open: {} (Empty name for new text buffer, ^G or ESC to cancel)",
            |_, _, _, _| Ok(()),
        )? {
            let buf = if input.is_empty() {
                TextBuffer::default()
            } else {
                TextBuffer::open(input)?
            };
            self.hl = Highlighting::new(buf.lang(), buf.rows());
            self.bufs.push(buf);
            self.buf_idx = self.bufs.len() - 1;
            self.reset_screen()
        } else {
            Ok(()) // Canceled
        }
    }

    fn switch_buffer(&mut self, idx: usize) -> io::Result<()> {
        let len = self.bufs.len();
        if len == 1 {
            self.screen.set_info_message("No other buffer is opened");
            return Ok(());
        }

        debug_assert!(idx < len);
        self.buf_idx = idx;
        let buf = self.buf();

        // XXX: Should we put Highlighting instance in TextBuffer rather than Editor?
        // Then we don't need to recreate Highlighting instance for each buffer switch.
        self.hl = Highlighting::new(buf.lang(), buf.rows());
        self.reset_screen()
    }

    fn next_buffer(&mut self) -> io::Result<()> {
        self.switch_buffer(if self.buf_idx == self.bufs.len() - 1 {
            0
        } else {
            self.buf_idx + 1
        })
    }

    fn previous_buffer(&mut self) -> io::Result<()> {
        self.switch_buffer(if self.buf_idx == 0 {
            self.bufs.len() - 1
        } else {
            self.buf_idx - 1
        })
    }

    fn save(&mut self) -> io::Result<()> {
        let mut create = false;
        if !self.buf().has_file() {
            if let Some(input) =
                self.prompt("Save as: {} (^G or ESC to cancel)", |_, _, _, _| Ok(()))?
            {
                if input.is_empty() {}
                let prev_lang = self.buf().lang();
                self.buf_mut().set_file(input);
                self.hl.lang_changed(self.buf().lang());
                if prev_lang != self.buf().lang() {
                    // Render entire screen since highglight updated
                    self.screen.set_dirty_start(self.screen.rowoff);
                }
                create = true;
            }
        }

        match self.buf_mut().save() {
            Ok(msg) => self.screen.set_info_message(msg),
            Err(msg) => {
                self.screen.set_error_message(msg);
                if create {
                    self.buf_mut().set_unnamed();
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

        let row_len = self.buf().rows().len();
        let dir = self.finding.dir;
        let mut y = self
            .finding
            .last_match
            .map(|y| next_line(y, dir, row_len)) // Start from next line on moving to next match
            .unwrap_or_else(|| self.buf().cy());

        for _ in 0..row_len {
            let row = &self.buf().rows()[y];
            if let Some(byte_idx) = row.buffer().find(query) {
                let idx = row.char_idx_of(byte_idx);
                self.buf_mut().set_cursor(idx, y);

                let row = &self.buf().rows()[y]; // Immutable borrow again since self.buf().set_cursor() yields mutable borrow
                let rx = row.rx_from_cx(self.buf().cx());
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
            self.buf().cx(),
            self.buf().cy(),
            self.screen.coloff,
            self.screen.rowoff,
        );
        let s = "Search: {} (^F or RIGHT to forward, ^B or LEFT to back, ^G or ESC to cancel)";
        let input = self.prompt(s, Self::on_incremental_find)?;
        if input.as_ref().map(String::is_empty).unwrap_or(true) {
            // Canceled. Restore cursor position
            self.buf_mut().set_cursor(cx, cy);
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

            match (&seq.key, seq.ctrl) {
                (Unidentified, ..) => continue,
                (Key(b'h'), true) | (Key(0x7f), ..) | (DeleteKey, ..) if !buf.is_empty() => {
                    buf.pop();
                }
                (Key(b'g'), true) | (Key(b'q'), true) | (Key(0x1b), ..) => {
                    finished = true;
                    canceled = true;
                }
                (Key(b'\r'), ..) | (Key(b'm'), true) => {
                    finished = true;
                }
                (Key(b), false) => buf.push(*b as char),
                (Utf8Key(c), false) => buf.push(*c),
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
            .set_info_message(if canceled { "Canceled" } else { "" });
        self.refresh_screen()?;

        Ok(if canceled { None } else { Some(buf) })
    }

    fn handle_quit(&mut self) -> io::Result<AfterKeyPress> {
        let modified = self.bufs.iter().any(|b| b.modified());
        if !modified || self.quitting {
            Ok(AfterKeyPress::Quit)
        } else {
            self.quitting = true;
            self.screen.set_error_message(
                "At least one file has unsaved changes! Press ^Q again to quit or ^S to save",
            );
            Ok(AfterKeyPress::Refresh)
        }
    }

    fn handle_not_mapped(&mut self, seq: InputSeq) {
        self.screen
            .set_error_message(format!("Key '{}' not mapped", seq));
    }

    fn process_keypress(&mut self, s: InputSeq) -> io::Result<AfterKeyPress> {
        use KeySeq::*;

        let rowoff = self.screen.rowoff;
        let rows = self.screen.rows();
        self.buf_mut().dirty = false;

        match &s {
            InputSeq {
                key: Unidentified, ..
            } => return Ok(AfterKeyPress::DoNothing),
            InputSeq { key, alt: true, .. } => match key {
                Key(b'v') => self.buf_mut().move_cursor_page(CursorDir::Up, rowoff, rows),
                Key(b'f') => self.buf_mut().move_cursor_by_word(CursorDir::Right),
                Key(b'b') => self.buf_mut().move_cursor_by_word(CursorDir::Left),
                Key(b'n') => self.buf_mut().move_cursor_paragraph(CursorDir::Down),
                Key(b'p') => self.buf_mut().move_cursor_paragraph(CursorDir::Up),
                Key(b'<') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Up),
                Key(b'>') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Down),
                LeftKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Left),
                RightKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Right),
                _ => self.handle_not_mapped(s),
            },
            InputSeq {
                key, ctrl: true, ..
            } => match key {
                Key(b'p') => self.buf_mut().move_cursor_one(CursorDir::Up),
                Key(b'b') => self.buf_mut().move_cursor_one(CursorDir::Left),
                Key(b'n') => self.buf_mut().move_cursor_one(CursorDir::Down),
                Key(b'f') => self.buf_mut().move_cursor_one(CursorDir::Right),
                Key(b'v') => self
                    .buf_mut()
                    .move_cursor_page(CursorDir::Down, rowoff, rows),
                Key(b'a') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Left),
                Key(b'e') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Right),
                Key(b'd') => self.buf_mut().delete_right_char(),
                Key(b'g') => self.find()?,
                Key(b'h') => self.buf_mut().delete_char(),
                Key(b'k') => self.buf_mut().delete_until_end_of_line(),
                Key(b'u') => self.buf_mut().delete_until_head_of_line(),
                Key(b'w') => self.buf_mut().delete_word(),
                Key(b'l') => self.screen.set_dirty_start(self.screen.rowoff), // Clear
                Key(b's') => self.save()?,
                Key(b'i') => self.buf_mut().insert_tab(),
                Key(b'm') => self.buf_mut().insert_line(),
                Key(b'o') => self.open_buffer()?,
                Key(b'?') => self.show_help()?,
                Key(b'x') => self.next_buffer()?,
                Key(b'z') => self.previous_buffer()?,
                Key(b']') => self
                    .buf_mut()
                    .move_cursor_page(CursorDir::Down, rowoff, rows),
                LeftKey => self.buf_mut().move_cursor_by_word(CursorDir::Left),
                RightKey => self.buf_mut().move_cursor_by_word(CursorDir::Right),
                DownKey => self.buf_mut().move_cursor_paragraph(CursorDir::Down),
                UpKey => self.buf_mut().move_cursor_paragraph(CursorDir::Up),
                Key(b'q') => return self.handle_quit(),
                _ => self.handle_not_mapped(s),
            },
            InputSeq { key, .. } => match key {
                Key(0x1b) => self.buf_mut().move_cursor_page(CursorDir::Up, rowoff, rows), // Clash with Ctrl-[
                Key(0x08) => self.buf_mut().delete_char(), // Backspace
                Key(0x7f) => self.buf_mut().delete_char(), // Delete key is mapped to \x1b[3~
                Key(b'\r') => self.buf_mut().insert_line(),
                Key(b) if !b.is_ascii_control() => self.buf_mut().insert_char(*b as char),
                Utf8Key(c) => self.buf_mut().insert_char(*c),
                UpKey => self.buf_mut().move_cursor_one(CursorDir::Up),
                LeftKey => self.buf_mut().move_cursor_one(CursorDir::Left),
                DownKey => self.buf_mut().move_cursor_one(CursorDir::Down),
                RightKey => self.buf_mut().move_cursor_one(CursorDir::Right),
                PageUpKey => self.buf_mut().move_cursor_page(CursorDir::Up, rowoff, rows),
                PageDownKey => self
                    .buf_mut()
                    .move_cursor_page(CursorDir::Down, rowoff, rows),
                HomeKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Left),
                EndKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Right),
                DeleteKey => self.buf_mut().delete_right_char(),
                Cursor(_, _) => unreachable!(),
                _ => self.handle_not_mapped(s),
            },
        }

        if self.buf().dirty {
            self.hl.needs_update = true;
            self.screen.set_dirty_start(self.buf().cy());
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
        self.buf().lines()
    }

    pub fn screen(&self) -> &'_ Screen<W> {
        &self.screen
    }

    pub fn lang(&self) -> Language {
        self.buf().lang()
    }
}
