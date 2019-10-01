use crate::error::Result;
use crate::highlight::Highlighting;
use crate::input::{InputSeq, KeySeq};
use crate::screen::Screen;
use crate::status_bar::StatusBar;
use crate::text_buffer::TextBuffer;
use std::io::Write;

#[derive(PartialEq)]
pub enum PromptResult {
    Canceled,
    Input(String),
}

// Sized is necessary to move self
pub trait PromptAction: Sized {
    fn new<W: Write>(prompt: &mut Prompt<'_, W>) -> Self;

    // Returns bool which represents whether screen redraw is necessary
    fn on_seq<W: Write>(
        &mut self,
        _prompt: &mut Prompt<'_, W>,
        _input: &str,
        _seq: InputSeq,
    ) -> Result<bool> {
        Ok(false)
    }

    fn on_end<W: Write>(
        self, // Note: Consumes self
        _prompt: &mut Prompt<'_, W>,
        result: PromptResult,
    ) -> Result<PromptResult> {
        Ok(result)
    }
}

pub struct NoAction;
impl PromptAction for NoAction {
    fn new<W: Write>(_prompt: &mut Prompt<'_, W>) -> Self {
        Self
    }
}

#[derive(Clone, Copy)]
enum FindDir {
    Back,
    Forward,
}

pub struct TextSearch {
    saved_cx: usize,
    saved_cy: usize,
    saved_coloff: usize,
    saved_rowoff: usize,
    dir: FindDir,
    last_match: Option<usize>,
    line: usize,
}

impl TextSearch {
    fn cleanup_match_highlight<W: Write>(&self, prompt: &mut Prompt<'_, W>) {
        if self.last_match.is_none() {
            return;
        }
        if let Some(matched_line) = prompt.hl.clear_previous_match() {
            prompt.hl.needs_update = true;
            prompt.screen.set_dirty_start(matched_line);
        }
    }

    fn next_line<W: Write>(&mut self, prompt: &Prompt<'_, W>) {
        // Wrapping text search at top/bottom of text buffer
        let len = prompt.buf.rows().len();
        self.line = match self.dir {
            FindDir::Forward if self.line == len - 1 => 0,
            FindDir::Forward => self.line + 1,
            FindDir::Back if self.line == 0 => len - 1,
            FindDir::Back => self.line - 1,
        };
    }

    fn handle_seq(&mut self, seq: InputSeq) {
        use KeySeq::*;
        match (seq.key, seq.ctrl) {
            (RightKey, ..) | (DownKey, ..) | (Key(b'f'), true) | (Key(b'n'), true) => {
                self.dir = FindDir::Forward;
            }
            (LeftKey, ..) | (UpKey, ..) | (Key(b'b'), true) | (Key(b'p'), true) => {
                self.dir = FindDir::Back;
            }
            _ => {
                self.last_match = None;
            }
        }
    }

    fn find_in_line<W: Write>(&mut self, input: &str, prompt: &mut Prompt<'_, W>) -> bool {
        let row = &prompt.buf.rows()[self.line];
        let idx = if let Some(byte_idx) = row.buffer().find(input) {
            row.char_idx_of(byte_idx)
        } else {
            return false;
        };

        prompt.buf.set_cursor(idx, self.line);

        let rx = prompt.buf.rows()[self.line].rx_from_cx(prompt.buf.cx());
        // Cause do_scroll() to scroll upwards to half a screen above the matching line at
        // next screen redraw
        prompt.screen.rowoff = self.line.saturating_sub(prompt.screen.rows() / 2);
        prompt.screen.coloff = 0;
        self.last_match = Some(self.line);
        // Set match highlight on the found line
        prompt
            .hl
            .set_match(self.line, rx, rx + input.chars().count());
        // XXX: It updates entire highlights
        prompt.hl.needs_update = true;
        prompt.screen.set_dirty_start(prompt.screen.rowoff);
        true
    }
}

impl PromptAction for TextSearch {
    fn new<W: Write>(prompt: &mut Prompt<'_, W>) -> Self {
        Self {
            saved_cx: prompt.buf.cx(),
            saved_cy: prompt.buf.cy(),
            saved_coloff: prompt.screen.coloff,
            saved_rowoff: prompt.screen.rowoff,
            line: prompt.buf.cy(),
            dir: FindDir::Forward,
            last_match: None,
        }
    }

    fn on_seq<W: Write>(
        &mut self,
        prompt: &mut Prompt<'_, W>,
        input: &str,
        seq: InputSeq,
    ) -> Result<bool> {
        self.cleanup_match_highlight(prompt);
        self.handle_seq(seq);

        if input.is_empty() {
            return Ok(false);
        }

        if self.last_match.is_some() {
            self.next_line(prompt); // Avoid matching to current position
        }

        // TODO: Use more efficient string search algorithm such as Aho-Corasick
        for _ in 0..prompt.buf.rows().len() {
            if self.find_in_line(input, prompt) {
                break;
            }
            self.next_line(prompt);
        }

        Ok(true)
    }

    fn on_end<W: Write>(
        self,
        prompt: &mut Prompt<'_, W>,
        result: PromptResult,
    ) -> Result<PromptResult> {
        self.cleanup_match_highlight(prompt);

        use PromptResult::*;
        let result = match &result {
            Canceled => Canceled,
            Input(i) if i.is_empty() => Canceled,
            Input(_) if self.last_match.is_some() => {
                prompt.screen.set_info_message("Found");
                result
            }
            Input(_) => {
                prompt.screen.set_info_message("Not found");
                result
            }
        };

        if result == Canceled {
            prompt.buf.set_cursor(self.saved_cx, self.saved_cy);
            prompt.screen.coloff = self.saved_coloff;
            prompt.screen.rowoff = self.saved_rowoff;
            prompt.screen.set_dirty_start(prompt.screen.rowoff); // Redraw all lines
        }

        Ok(result)
    }
}

pub struct Prompt<'a, W: Write> {
    screen: &'a mut Screen<W>,
    buf: &'a mut TextBuffer,
    hl: &'a mut Highlighting,
    sb: &'a mut StatusBar,
    empty_is_cancel: bool,
}

impl<'a, W: Write> Prompt<'a, W> {
    pub fn new<'s: 'a, 'tb: 'a, 'h: 'a, 'sb: 'a>(
        screen: &'s mut Screen<W>,
        buf: &'tb mut TextBuffer,
        hl: &'h mut Highlighting,
        sb: &'sb mut StatusBar,
        empty_is_cancel: bool,
    ) -> Self {
        Self {
            screen,
            buf,
            hl,
            sb,
            empty_is_cancel,
        }
    }

    fn render_screen(&mut self) -> Result<()> {
        self.sb.update_from_buf(&self.buf);
        self.screen.render(self.buf, &mut self.hl, &self.sb)?;
        self.sb.redraw = false;
        Ok(())
    }

    pub fn run<A, S, I>(&mut self, prompt: S, mut input: I) -> Result<PromptResult>
    where
        A: PromptAction,
        S: AsRef<str>,
        I: Iterator<Item = Result<InputSeq>>,
    {
        let mut action = A::new(self);
        let mut buf = String::new();
        let mut canceled = false;
        let prompt = prompt.as_ref();
        self.screen.set_info_message(prompt.replacen("{}", "", 1));
        self.render_screen()?;

        while let Some(seq) = input.next() {
            use KeySeq::*;

            if self.screen.maybe_resize(&mut input)? {
                self.screen.set_dirty_start(self.screen.rowoff);
                self.sb.redraw = true;
                self.screen.set_info_message(prompt.replacen("{}", &buf, 1));
                self.render_screen()?;
                continue;
            }

            let seq = seq?;
            let prev_len = buf.len();

            match (&seq.key, seq.ctrl) {
                (Unidentified, ..) => continue,
                (Key(b'h'), true) | (Key(0x7f), ..) | (DeleteKey, ..) if !buf.is_empty() => {
                    buf.pop();
                }
                (Key(b'g'), true) | (Key(b'q'), true) | (Key(0x1b), ..) => {
                    canceled = true;
                    break;
                }
                (Key(b'\r'), ..) | (Key(b'm'), true) => break,
                (Key(b), false) => buf.push(*b as char),
                (Utf8Key(c), false) => buf.push(*c),
                _ => {}
            }

            let should_render = action.on_seq(self, buf.as_str(), seq)?;

            if should_render || prev_len != buf.len() {
                self.screen.set_info_message(prompt.replacen("{}", &buf, 1));
                self.render_screen()?;
            }
        }

        let result = if canceled || self.empty_is_cancel && buf.is_empty() {
            self.screen.set_info_message("Canceled");
            PromptResult::Canceled
        } else {
            self.screen.unset_message();
            self.sb.redraw = true;
            PromptResult::Input(buf)
        };

        action.on_end(self, result)
    }
}
