use crate::error::Result;
use crate::highlight::Highlighting;
use crate::input::{InputSeq, KeySeq};
use crate::language::Language;
use crate::prompt::{self, Prompt, PromptResult};
use crate::screen::Screen;
use crate::status_bar::StatusBar;
use crate::text_buffer::{CursorDir, Lines, TextBuffer};
use std::io::Write;
use std::path::Path;
use std::str;

pub struct Editor<I: Iterator<Item = Result<InputSeq>>, W: Write> {
    input: I,       // Escape sequences stream represented as Iterator
    quitting: bool, // After first Ctrl-Q
    hl: Highlighting,
    screen: Screen<W>,
    bufs: Vec<TextBuffer>,
    buf_idx: usize,
    status_bar: StatusBar,
}

impl<I, W> Editor<I, W>
where
    I: Iterator<Item = Result<InputSeq>>,
    W: Write,
{
    pub fn new(
        mut input: I,
        output: W,
        window_size: Option<(usize, usize)>,
    ) -> Result<Editor<I, W>> {
        let screen = Screen::new(window_size, &mut input, output)?;
        let buf = TextBuffer::empty();
        let status_bar = StatusBar::from_buffer(&buf, (1, 1));
        Ok(Editor {
            input,
            quitting: false,
            hl: Highlighting::default(),
            screen,
            bufs: vec![buf],
            buf_idx: 0,
            status_bar,
        })
    }

    pub fn open<P: AsRef<Path>>(
        mut input: I,
        output: W,
        window_size: Option<(usize, usize)>,
        paths: &[P],
    ) -> Result<Editor<I, W>> {
        if paths.is_empty() {
            return Self::new(input, output, window_size);
        }
        let screen = Screen::new(window_size, &mut input, output)?;
        let bufs: Vec<_> = paths.iter().map(TextBuffer::open).collect::<Result<_>>()?;
        let hl = Highlighting::new(bufs[0].lang(), bufs[0].rows());
        let status_bar = StatusBar::from_buffer(&bufs[0], (1, bufs.len()));
        Ok(Editor {
            input,
            quitting: false,
            hl,
            screen,
            bufs,
            buf_idx: 0,
            status_bar,
        })
    }

    fn buf(&self) -> &TextBuffer {
        &self.bufs[self.buf_idx]
    }

    fn buf_mut(&mut self) -> &mut TextBuffer {
        &mut self.bufs[self.buf_idx]
    }

    fn refresh_status_bar(&mut self) {
        self.status_bar
            .set_buf_pos((self.buf_idx + 1, self.bufs.len()));
        self.status_bar.update_from_buf(&self.bufs[self.buf_idx]);
    }

    fn refresh_screen(&mut self) -> Result<()> {
        self.refresh_status_bar();
        self.screen
            .refresh(&self.bufs[self.buf_idx], &mut self.hl, &self.status_bar)?;
        self.status_bar.redraw = false;
        Ok(())
    }

    fn reset_screen(&mut self) -> Result<()> {
        self.screen.set_dirty_start(0);
        self.screen.rowoff = 0;
        self.screen.coloff = 0;
        self.refresh_screen()
    }

    fn open_buffer(&mut self) -> Result<()> {
        if let PromptResult::Input(input) = self.prompt_new(
            "Open: {} (Empty name for new text buffer, ^G or ESC to cancel)",
            false,
        )? {
            let buf = if input.is_empty() {
                TextBuffer::empty()
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

    fn switch_buffer(&mut self, idx: usize) -> Result<()> {
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

    fn next_buffer(&mut self) -> Result<()> {
        self.switch_buffer(if self.buf_idx == self.bufs.len() - 1 {
            0
        } else {
            self.buf_idx + 1
        })
    }

    fn previous_buffer(&mut self) -> Result<()> {
        self.switch_buffer(if self.buf_idx == 0 {
            self.bufs.len() - 1
        } else {
            self.buf_idx - 1
        })
    }

    fn prompt_new<S: AsRef<str>>(
        &mut self,
        prompt: S,
        empty_is_cancel: bool,
    ) -> Result<PromptResult> {
        Prompt::new(
            &mut self.screen,
            &mut self.bufs[self.buf_idx],
            &mut self.hl,
            &mut self.status_bar,
            empty_is_cancel,
        )
        .run::<prompt::NoAction, _, _>(prompt, &mut self.input)
    }

    fn save(&mut self) -> Result<()> {
        let mut create = false;
        if !self.buf().has_file() {
            if let PromptResult::Input(input) = self.prompt_new(
                "Open: {} (Empty name for new text buffer, ^G or ESC to cancel)",
                true,
            )? {
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

    fn find(&mut self) -> Result<()> {
        let prompt = "Search: {} (^F or RIGHT to forward, ^B or LEFT to back, ^G or ESC to cancel)";
        Prompt::new(
            &mut self.screen,
            &mut self.bufs[self.buf_idx],
            &mut self.hl,
            &mut self.status_bar,
            true,
        )
        .run::<prompt::TextSearch, _, _>(prompt, &mut self.input)?;
        Ok(())
    }

    fn show_help(&mut self) -> Result<()> {
        self.screen.draw_help()?;

        // Consume any key
        while let Some(seq) = self.input.next() {
            if self.screen.maybe_resize(&mut self.input)? {
                self.screen.draw_help()?;
                self.status_bar.redraw = true;
            }
            if seq?.key != KeySeq::Unidentified {
                break;
            }
        }

        // Redraw screen
        self.screen.set_dirty_start(self.screen.rowoff);
        Ok(())
    }

    fn handle_quit(&mut self) -> Result<bool> {
        let modified = self.bufs.iter().any(|b| b.modified());
        if !modified || self.quitting {
            Ok(true)
        } else {
            self.quitting = true;
            self.screen.set_error_message(
                "At least one file has unsaved changes! Press ^Q again to quit or ^S to save",
            );
            Ok(false)
        }
    }

    fn handle_not_mapped(&mut self, seq: InputSeq) {
        self.screen
            .set_error_message(format!("Key '{}' not mapped", seq));
    }

    fn redraw_screen(&mut self) -> Result<()> {
        self.screen.set_dirty_start(self.screen.rowoff);
        self.screen.unset_message();
        self.status_bar.redraw = true;
        self.refresh_screen()
    }

    fn process_keypress(&mut self, s: InputSeq) -> Result<bool> {
        use KeySeq::*;

        let rowoff = self.screen.rowoff;
        let rows = self.screen.rows();
        let (prev_cx, prev_cy) = (self.buf().cx(), self.buf().cy());
        self.buf_mut().dirty_start = None;

        match &s {
            InputSeq {
                key: Unidentified, ..
            } => return Ok(false),
            InputSeq { key, alt: true, .. } => match key {
                Key(b'v') => self.buf_mut().move_cursor_page(CursorDir::Up, rowoff, rows),
                Key(b'f') => self.buf_mut().move_cursor_by_word(CursorDir::Right),
                Key(b'b') => self.buf_mut().move_cursor_by_word(CursorDir::Left),
                Key(b'n') => self.buf_mut().move_cursor_paragraph(CursorDir::Down),
                Key(b'p') => self.buf_mut().move_cursor_paragraph(CursorDir::Up),
                Key(b'x') => self.previous_buffer()?,
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
                Key(b'j') => self.buf_mut().delete_until_head_of_line(),
                Key(b'w') => self.buf_mut().delete_word(),
                Key(b'l') => {
                    self.screen.set_dirty_start(self.screen.rowoff); // Clear
                    self.screen.unset_message();
                    self.status_bar.redraw = true;
                }
                Key(b's') => self.save()?,
                Key(b'i') => self.buf_mut().insert_tab(),
                Key(b'm') => self.buf_mut().insert_line(),
                Key(b'o') => self.open_buffer()?,
                Key(b'?') => self.show_help()?,
                Key(b'x') => self.next_buffer()?,
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

        if let Some(line) = self.buf().dirty_start {
            self.hl.needs_update = true;
            self.screen.set_dirty_start(line);
        }
        if self.buf().cx() != prev_cx || self.buf().cy() != prev_cy {
            self.screen.cursor_moved = true;
        }
        self.quitting = false;
        Ok(false)
    }

    pub fn edit(&mut self) -> Result<()> {
        self.refresh_screen()?; // First paint

        while let Some(seq) = self.input.next() {
            if self.screen.maybe_resize(&mut self.input)? {
                self.redraw_screen()?;
            }

            if self.process_keypress(seq?)? {
                break;
            }

            self.refresh_screen()?;
        }

        Ok(())
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
