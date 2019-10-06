use crate::error::Result;
use crate::highlight::Highlighting;
use crate::input::{InputSeq, KeySeq};
use crate::row::Row;
use crate::screen::Screen;
use crate::status_bar::StatusBar;
use crate::text_buffer::TextBuffer;
use std::cmp::Ordering;
use std::io::Write;

#[derive(PartialEq)]
pub enum PromptResult {
    Canceled,
    Input(String),
}

// Sized is necessary to move self
pub trait Action: Sized {
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
impl Action for NoAction {
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
    saved: ((usize, usize), (usize, usize)),
    dir: FindDir,
    matched: bool,
    text: Box<str>,
    line_starts: Box<[usize]>,
    current_offset: usize,
}

impl TextSearch {
    fn cleanup_match_highlight<W: Write>(&self, prompt: &mut Prompt<'_, W>) {
        if !self.matched {
            return;
        }
        if let Some(matched_line) = prompt.hl.clear_previous_match() {
            prompt.hl.needs_update = true;
            prompt.screen.set_dirty_start(matched_line);
        }
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
                self.matched = false; // Clear since new input might change input
            }
        }
    }

    fn reject_match_to_current(&mut self) {
        // Reject current cursor position to be matched to search pattern by moving offset to next
        self.current_offset = match self.dir {
            FindDir::Forward => {
                if let Some((idx, _)) = self.text[self.current_offset..].char_indices().nth(1) {
                    self.current_offset + idx
                } else {
                    0 // Wrapped
                }
            }
            FindDir::Back => self.text[..self.current_offset]
                .char_indices()
                .rev()
                .next()
                .map(|(idx, _)| idx)
                .unwrap_or_else(|| self.text.len()),
        };
    }

    fn search<W: Write>(&mut self, input: &str, prompt: &mut Prompt<'_, W>) {
        if let Some(offset) = self.find_at(input, self.current_offset) {
            self.current_offset = offset;
        } else {
            return;
        }

        let (x, y) = self.offset_to_pos(self.current_offset, prompt.buf.rows());
        prompt.buf.set_cursor(x, y);

        let rx = prompt.buf.rows()[y].rx_from_cx(x);
        // Cause do_scroll() to scroll upwards to half a screen above the matching line at
        // next screen redraw
        prompt.screen.rowoff = y.saturating_sub(prompt.screen.rows() / 2);
        prompt.screen.coloff = 0;
        // Set match highlight on the found line
        prompt.hl.set_match(y, rx, rx + input.chars().count());
        // XXX: It updates entire highlights
        prompt.hl.needs_update = true;
        prompt.screen.set_dirty_start(prompt.screen.rowoff);

        self.matched = true;
    }

    fn nearest_line(&self, byte_offset: usize) -> usize {
        fn bsearch_nearest_line(offsets: &[usize], l: usize, r: usize, want: usize) -> usize {
            debug_assert!(l <= r);
            if r - l <= 1 {
                return l; // Fallback to the nearest
            }
            let idx = (l + r) / 2;
            let offset = offsets[idx];
            match want.cmp(&offset) {
                Ordering::Less => bsearch_nearest_line(offsets, l, idx, want),
                Ordering::Equal => idx,
                Ordering::Greater => bsearch_nearest_line(offsets, idx, r, want),
            }
        }

        bsearch_nearest_line(&self.line_starts, 0, self.line_starts.len(), byte_offset)
    }

    fn offset_to_pos(&self, byte_offset: usize, rows: &[Row]) -> (usize, usize) {
        let y = self.nearest_line(byte_offset);
        let y_offset = self.line_starts[y];
        let x_offset = byte_offset - y_offset;
        (rows[y].char_idx_of(x_offset), y)
    }

    fn pos_to_offset(&self, pos: (usize, usize), rows: &[Row]) -> usize {
        let y = pos.1;
        let x = rows[y].byte_idx_of(pos.0);
        self.line_starts[y] + x
    }

    fn find_at(&self, query: &str, off: usize) -> Option<usize> {
        match self.dir {
            FindDir::Forward => {
                // TODO: Use more efficient string search algorithm such as Aho-Corasick
                if let Some(idx) = self.text[off..].find(query) {
                    return Some(off + idx);
                }
                if let Some(idx) = self.text.find(query) {
                    // TODO: This takes O(2 * n) where n is length of text. Worst case is when there is no match.
                    if idx < off {
                        return Some(idx);
                    }
                }
            }
            FindDir::Back => {
                // TODO: Use more efficient string search algorithm such as Aho-Corasick
                if let Some(idx) = self.text[..off].rfind(query) {
                    return Some(idx);
                }
                if let Some(idx) = self.text.rfind(query) {
                    // Considering the case where matched region contains cursor position, we must check last index
                    let last_idx = idx + query.len();
                    // TODO: This takes O(2 * n) where n is length of text. Worst case is when there is no match.
                    if off < last_idx {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }
}

impl Action for TextSearch {
    fn new<W: Write>(prompt: &mut Prompt<'_, W>) -> Self {
        let rows = prompt.buf.rows();
        let cap = rows.iter().fold(0, |acc, row| acc + row.buffer().len() + 1);
        let mut text = String::with_capacity(cap);

        let mut pos = 0;
        let mut line_starts = Vec::with_capacity(rows.len());
        for row in rows {
            line_starts.push(pos);
            text.push_str(row.buffer());
            text.push('\n');
            pos += row.buffer().len() + 1;
        }

        let mut new = Self {
            saved: (
                prompt.buf.cursor(),
                (prompt.screen.rowoff, prompt.screen.coloff),
            ),
            dir: FindDir::Forward,
            matched: false,
            text: text.into_boxed_str(),
            line_starts: line_starts.into_boxed_slice(),
            current_offset: 0, // Set later
        };
        new.current_offset = new.pos_to_offset(prompt.buf.cursor(), rows);

        new
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

        if self.matched {
            // When already matched, it means moving cursor to next/previous match
            self.reject_match_to_current();
        }

        self.search(input, prompt);
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
            Input(_) if self.matched => {
                prompt.screen.set_info_message("Found");
                result
            }
            Input(_) => {
                prompt.screen.set_info_message("Not found");
                result
            }
        };

        if result == Canceled {
            let ((cx, cy), (rowoff, coloff)) = self.saved;
            prompt.buf.set_cursor(cx, cy);
            prompt.screen.rowoff = rowoff;
            prompt.screen.coloff = coloff;
            prompt.screen.set_dirty_start(prompt.screen.rowoff); // Redraw all lines
        }

        Ok(result)
    }
}

struct PromptTemplate<'a> {
    prefix: &'a str,
    suffix: &'a str,
}

impl<'a> PromptTemplate<'a> {
    fn build(&self, input: &str) -> String {
        let cap = self.prefix.len() + self.suffix.len() + input.len();
        let mut buf = String::with_capacity(cap);
        buf.push_str(self.prefix);
        buf.push_str(input);
        buf.push_str(self.suffix);
        buf
    }

    fn cursor_col(&self, input: &str) -> usize {
        self.prefix.chars().count() + input.chars().count() + 1 // Just after the input
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

    fn render_screen(&mut self, input: &str, template: &PromptTemplate<'_>) -> Result<()> {
        self.screen.set_info_message(template.build(input));
        self.sb.update_from_buf(&self.buf);
        self.screen.render(self.buf, &mut self.hl, &self.sb)?;

        let row = self.screen.rows() + 2;
        let col = template.cursor_col(input);
        self.screen.force_set_cursor(row, col)?;

        self.sb.redraw = false;
        Ok(())
    }

    pub fn run<A, S, I>(&mut self, prompt: S, mut input: I) -> Result<PromptResult>
    where
        A: Action,
        S: AsRef<str>,
        I: Iterator<Item = Result<InputSeq>>,
    {
        let mut action = A::new(self);
        let mut buf = String::new();
        let mut canceled = false;

        let template = {
            let mut it = prompt.as_ref().splitn(2, "{}");
            let prefix = it.next().unwrap();
            let suffix = it.next().unwrap();
            PromptTemplate { prefix, suffix }
        };

        self.render_screen("", &template)?;

        while let Some(seq) = input.next() {
            use KeySeq::*;

            if self.screen.maybe_resize(&mut input)? {
                self.screen.set_dirty_start(self.screen.rowoff);
                self.sb.redraw = true;
                self.render_screen(&buf, &template)?;
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
                (Key(b'u'), true) => buf.clear(),
                (Key(b), false) => buf.push(*b as char),
                (Utf8Key(c), false) => buf.push(*c),
                _ => {}
            }

            let should_render = action.on_seq(self, buf.as_str(), seq)?;

            if should_render || prev_len != buf.len() {
                self.render_screen(&buf, &template)?;
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
