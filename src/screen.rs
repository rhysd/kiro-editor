use crate::error::{Error, Result};
use crate::highlight::Highlighting;
use crate::input::{InputSeq, KeySeq};
use crate::signal::SigwinchWatcher;
use crate::span::Span;
use crate::status_bar::StatusBar;
use crate::term_color::{Color, TermColor};
use crate::text_buffer::TextBuffer;
use std::cmp;
use std::io::Write;
use std::time::SystemTime;
use unicode_width::UnicodeWidthChar;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const HELP: &str = "\
    Ctrl-Q                        : Quit
    Ctrl-S                        : Save to file
    Ctrl-O                        : Open text buffer
    Ctrl-X                        : Next text buffer
    Alt-X                         : Previous text buffer
    Ctrl-P or UP                  : Move cursor up
    Ctrl-N or DOWN                : Move cursor down
    Ctrl-F or RIGHT               : Move cursor right
    Ctrl-B or LEFT                : Move cursor left
    Ctrl-A or Alt-LEFT or HOME    : Move cursor to head of line
    Ctrl-E or Alt-RIGHT or END    : Move cursor to end of line
    Ctrl-[ or Ctrl-V or PAGE DOWN : Next page
    Ctrl-] or Alt-V or PAGE UP    : Previous page
    Alt-F or Ctrl-RIGHT           : Move cursor to next word
    Alt-B or Ctrl-LEFT            : Move cursor to previous word
    Alt-N or Ctrl-DOWN            : Move cursor to next paragraph
    Alt-P or Ctrl-UP              : Move cursor to previous paragraph
    Alt-<                         : Move cursor to top of file
    Alt->                         : Move cursor to bottom of file
    Ctrl-H or BACKSPACE           : Delete character
    Ctrl-D or DELETE              : Delete next character
    Ctrl-W                        : Delete a word
    Ctrl-J                        : Delete until head of line
    Ctrl-K                        : Delete until end of line
    Ctrl-U                        : Undo last change
    Ctrl-R                        : Redo last undo change
    Ctrl-G                        : Search text
    Ctrl-M                        : New line
    Ctrl-L                        : Refresh screen
    Ctrl-?                        : Show this help";

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
    fn new<S: Into<String>>(message: S, kind: StatusMessageKind) -> StatusMessage {
        StatusMessage {
            text: message.into(),
            timestamp: SystemTime::now(),
            kind,
        }
    }
}

fn get_window_size<I, W>(input: I, mut output: W) -> Result<(usize, usize)>
where
    I: Iterator<Item = Result<InputSeq>>,
    W: Write,
{
    if let Some(s) = term_size::dimensions_stdout() {
        return Ok(s);
    }

    // By moving cursor at the bottom-right corner by 'B' and 'C' commands, get the size of
    // current screen. \x1b[9999;9999H is not available since it does not guarantee cursor
    // stops on the corner. Finally command 'n' queries cursor position.
    output.write(b"\x1b[9999C\x1b[9999B\x1b[6n")?;
    output.flush()?;

    // Wait for response from terminal discarding other sequences
    for seq in input {
        if let KeySeq::Cursor(r, c) = seq?.key {
            return Ok((c, r));
        }
    }

    Err(Error::UnknownWindowSize) // Give up
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum DrawMessage {
    Open,
    Close,
    Update,
    DoNothing,
}

impl DrawMessage {
    fn fold(self, rhs: Self) -> Self {
        // Folding new state into current state. For example, when current state is Open and new state
        // is Close, applying Open then Close means DoNothing.
        use DrawMessage::*;
        match (self, rhs) {
            (Open, Open) => unreachable!(),
            (Open, Close) => DoNothing,
            (Open, Update) => Open,
            (Close, Open) => Update,
            (Close, Close) => unreachable!(),
            (Close, Update) => unreachable!(),
            (Update, Open) => unreachable!(),
            (Update, Close) => Close,
            (Update, Update) => Update,
            (lhs, DoNothing) => lhs,
            (DoNothing, rhs) => rhs,
        }
    }
}

pub struct Screen<W: Write> {
    output: W,
    // X coordinate in `render` text of rows
    rx: usize,
    // Screen size
    num_cols: usize,
    num_rows: usize,
    message: Option<StatusMessage>,
    draw_message: DrawMessage,
    // Dirty line which requires rendering update. After this line must be updated since
    // updating line may affect highlights of succeeding lines
    dirty_start: Option<usize>,
    // Watch resize signal
    sigwinch: SigwinchWatcher,
    term_color: TermColor,
    pub cursor_moved: bool,
    pub rowoff: usize, // Row scroll offset
    pub coloff: usize, // Column scroll offset
}

impl<W: Write> Screen<W> {
    pub fn new<I>(size: Option<(usize, usize)>, input: I, mut output: W) -> Result<Self>
    where
        I: Iterator<Item = Result<InputSeq>>,
    {
        let (w, h) = if let Some(s) = size {
            s
        } else {
            get_window_size(input, &mut output)?
        };

        if w == 0 || h < 3 {
            return Err(Error::TooSmallWindow(w, h));
        }

        // Enter alternate screen buffer to restore previous screen on quit
        // https://www.xfree86.org/current/ctlseqs.html#The%20Alternate%20Screen%20Buffer
        output.write(b"\x1b[?47h")?;

        Ok(Self {
            output,
            rx: 0,
            num_cols: w,
            // Screen height is 1 line less than window height due to status bar
            num_rows: h.saturating_sub(2),
            message: Some(StatusMessage::new(
                "Ctrl-? for help",
                StatusMessageKind::Info,
            )),
            draw_message: DrawMessage::Open,
            dirty_start: Some(0), // Render entire screen at first paint
            sigwinch: SigwinchWatcher::new()?,
            term_color: TermColor::from_env(),
            cursor_moved: true,
            rowoff: 0,
            coloff: 0,
        })
    }

    fn write_flush(&mut self, bytes: &[u8]) -> Result<()> {
        self.output.write(bytes)?;
        self.output.flush()?;
        Ok(())
    }

    fn trim_line<'a, S: AsRef<str>>(&self, line: &'a S) -> String {
        let line = line.as_ref();
        if line.len() <= self.coloff {
            return "".to_string();
        }
        line.chars().skip(self.coloff).take(self.num_cols).collect()
    }

    fn draw_status_bar<B: Write>(&self, mut buf: B, status_bar: &StatusBar) -> Result<()> {
        write!(buf, "\x1b[{}H", self.rows() + 1)?;

        buf.write(self.term_color.sequence(Color::Invert))?;

        let left = status_bar.left();
        // TODO: Handle multi-byte chars correctly
        let left = &left[..cmp::min(left.len(), self.num_cols)];
        buf.write(left.as_bytes())?; // Left of status bar

        let rest_len = self.num_cols - left.len();
        if rest_len == 0 {
            return Ok(());
        }

        let right = status_bar.right();
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

        // Default argument of 'm' command is 0 so it resets attributes
        buf.write(self.term_color.sequence(Color::Reset))?;
        Ok(())
    }

    fn draw_message_bar<B: Write>(&self, mut buf: B, message: &StatusMessage) -> Result<()> {
        // TODO: Handle multi-byte chars correctly
        let text = &message.text[..cmp::min(message.text.len(), self.num_cols)];

        write!(buf, "\x1b[{}H", self.num_rows + 2)?;

        if message.kind == StatusMessageKind::Error {
            buf.write(self.term_color.sequence(Color::RedBG))?;
        }

        buf.write(text.as_bytes())?;

        if message.kind != StatusMessageKind::Info {
            buf.write(self.term_color.sequence(Color::Reset))?;
        }

        buf.write(b"\x1b[K")?;
        Ok(())
    }

    fn draw_welcome_message<B: Write>(&self, mut buf: B) -> Result<()> {
        let msg_buf = format!("Kiro editor -- version {}", VERSION);
        let welcome = self.trim_line(&msg_buf);
        let padding = (self.num_cols - welcome.len()) / 2;
        if padding > 0 {
            buf.write(self.term_color.sequence(Color::NonText))?;
            buf.write(b"~")?;
            buf.write(self.term_color.sequence(Color::Reset))?;
            for _ in 0..padding - 1 {
                buf.write(b" ")?;
            }
        }
        buf.write(welcome.as_bytes())?;
        Ok(())
    }

    fn draw_rows<B: Write>(
        &self,
        mut buf: B,
        dirty_start: usize,
        rows: &[Span],
        hl: &Highlighting,
    ) -> Result<()> {
        let row_len = rows.len();

        buf.write(self.term_color.sequence(Color::Reset))?;

        for y in 0..self.rows() {
            let file_row = y + self.rowoff;

            if file_row < dirty_start {
                continue;
            }

            // H: Command to move cursor. Here \x1b[H is the same as \x1b[1;1H
            write!(buf, "\x1b[{}H", y + 1)?;

            let buf_is_empty = rows.is_empty() || rows.len() == 1 && rows[0].len() == 0;
            if file_row >= row_len {
                if buf_is_empty && y == self.rows() / 3 {
                    self.draw_welcome_message(&mut buf)?;
                } else {
                    buf.write(self.term_color.sequence(Color::NonText))?;
                    buf.write(b"~")?;
                }
            } else {
                let row = &rows[file_row];

                let mut col = 0;
                let mut prev_color = Color::Reset;
                for (c, hl) in row.render_text().chars().zip(hl.lines[file_row].iter()) {
                    col += c.width_cjk().unwrap_or(1);
                    if col <= self.coloff {
                        continue;
                    } else if col > self.num_cols + self.coloff {
                        break;
                    }

                    let color = hl.color();
                    if color != prev_color {
                        if prev_color.has_bg_color() {
                            buf.write(self.term_color.sequence(Color::Reset))?;
                        }
                        buf.write(self.term_color.sequence(color))?;
                        prev_color = color;
                    }

                    write!(buf, "{}", c)?;
                }
            }

            // Ensure to end with reset color sequence. Otherwise, when background color is highlighted
            // at the end of line, highlight will continue to the end of last column in terminal window.
            buf.write(self.term_color.sequence(Color::Reset))?;

            // Erases the part of the line to the right of the cursor. http://vt100.net/docs/vt100-ug/chapter3.html#EL
            buf.write(b"\x1b[K")?;
        }

        Ok(())
    }

    fn redraw(
        &mut self,
        text_buf: &TextBuffer,
        hl: &Highlighting,
        status_bar: &StatusBar,
    ) -> Result<()> {
        let cursor_row = text_buf.cy() - self.rowoff + 1;
        let cursor_col = self.rx - self.coloff + 1;
        let draw_message = self.draw_message;

        if self.dirty_start.is_none()
            && !status_bar.redraw
            && draw_message == DrawMessage::DoNothing
        {
            if self.cursor_moved {
                write!(self.output, "\x1b[{};{}H", cursor_row, cursor_col)?;
                self.output.flush()?;
            }
            return Ok(());
        }

        // \x1b[: Escape sequence header
        // Hide cursor while updating screen. 'l' is command to set mode http://vt100.net/docs/vt100-ug/chapter3.html#SM
        // This command must be flushed at first otherwise cursor may move before being hidden
        self.write_flush(b"\x1b[?25l")?;

        let mut buf = Vec::with_capacity((self.rows() + 2) * self.num_cols);
        if let Some(s) = self.dirty_start {
            self.draw_rows(&mut buf, s, text_buf.rows(), hl)?;
        }

        // When message bar opens/closes, position of status bar is changed
        if status_bar.redraw
            || draw_message == DrawMessage::Open
            || draw_message == DrawMessage::Close
        {
            self.draw_status_bar(&mut buf, status_bar)?;
        }

        // When closing message bar, nothing to do since status bar will overwrite old message bar
        if draw_message == DrawMessage::Update || draw_message == DrawMessage::Open {
            if let Some(message) = &self.message {
                self.draw_message_bar(&mut buf, message)?;
            }
        }

        // Move cursor even if cursor_moved is false since cursor is moved by draw_* methods
        write!(buf, "\x1b[{};{}H", cursor_row, cursor_col)?;

        // Reveal cursor again. 'h' is command to reset mode https://vt100.net/docs/vt100-ug/chapter3.html#RM
        buf.write(b"\x1b[?25h")?;

        self.write_flush(&buf)?;

        Ok(())
    }

    fn next_coloff(&self, want_stop: usize, row: &Span) -> usize {
        let mut coloff = 0;
        for c in row.render_text().chars() {
            coloff += c.width_cjk().unwrap_or(1);
            if coloff >= want_stop {
                // Screen cannot start from at the middle of double-width character
                break;
            }
        }
        coloff
    }

    fn do_scroll(&mut self, rows: &[Span], cx: usize, cy: usize) {
        let prev_rowoff = self.rowoff;
        let prev_coloff = self.coloff;

        // Calculate X coordinate to render considering tab stop
        if cy < rows.len() {
            self.rx = rows[cy].rx_from_cx(cx);
        } else {
            self.rx = 0;
        }

        // Adjust scroll position when cursor is outside screen
        if cy < self.rowoff {
            // Scroll up when cursor is above the top of window
            self.rowoff = cy;
        }
        if cy >= self.rowoff + self.rows() {
            // Scroll down when cursor is below the bottom of screen
            self.rowoff = cy - self.rows() + 1;
        }
        if self.rx < self.coloff {
            self.coloff = self.rx;
        }
        if self.rx >= self.coloff + self.num_cols {
            self.coloff = self.next_coloff(self.rx - self.num_cols + 1, &rows[cy]);
        }

        if prev_rowoff != self.rowoff || prev_coloff != self.coloff {
            // If scroll happens, all rows on screen must be updated
            // TODO: Improve rendering on scrolling up/down using scroll region commands \x1b[M/\x1b[D.
            // But scroll down region command was implemented in tmux recently and not included in
            // stable release: https://github.com/tmux/tmux/commit/45f4ff54850ff9b448070a96b33e63451f973e33
            self.set_dirty_start(self.rowoff);
        }
    }

    fn update_message_bar(&mut self) -> Result<()> {
        if let Some(m) = &self.message {
            if SystemTime::now().duration_since(m.timestamp)?.as_secs() > 5 {
                self.unset_message();
            }
        }
        if self.draw_message == DrawMessage::Close {
            self.set_dirty_start(self.num_rows); // Closing message bar reveals one more line
        }
        Ok(())
    }

    pub fn render(
        &mut self,
        buf: &TextBuffer,
        hl: &mut Highlighting,
        status_bar: &StatusBar,
    ) -> Result<()> {
        self.do_scroll(buf.rows(), buf.cx(), buf.cy());
        self.update_message_bar()?; // This must be updated here since it affects area of highlighting
        hl.update(buf.rows(), self.rowoff + self.rows());
        self.redraw(buf, hl, status_bar)?;
        // Clear state
        self.dirty_start = None;
        self.cursor_moved = false;
        self.draw_message = DrawMessage::DoNothing;
        Ok(())
    }

    pub fn draw_help(&mut self) -> Result<()> {
        let help: Vec<_> = HELP
            .split('\n')
            .skip_while(|s| !s.contains(':'))
            .map(str::trim_start)
            .collect();
        let rows = self.rows();

        let vertical_margin = if help.len() < rows {
            (rows - help.len()) / 2
        } else {
            0
        };
        let help_max_width = help.iter().map(|l| l.len()).max().unwrap();;
        let left_margin = if help_max_width < self.num_cols {
            (self.num_cols - help_max_width) / 2
        } else {
            0
        };

        let mut buf = Vec::with_capacity(rows * self.num_cols);

        for y in 0..vertical_margin {
            write!(buf, "\x1b[{}H", y + 1)?;
            buf.write(b"\x1b[K")?;
        }

        let left_pad = " ".repeat(left_margin);
        let help_height = cmp::min(vertical_margin + help.len(), rows);
        for y in vertical_margin..help_height {
            let idx = y - vertical_margin;
            write!(buf, "\x1b[{}H", y + 1)?;
            buf.write(left_pad.as_bytes())?;

            let help = &help[idx][..cmp::min(help[idx].len(), self.num_cols)];
            buf.write(self.term_color.sequence(Color::Cyan))?;
            let mut cols = help.split(':');
            if let Some(col) = cols.next() {
                buf.write(col.as_bytes())?;
            }
            buf.write(self.term_color.sequence(Color::Reset))?;
            if let Some(col) = cols.next() {
                write!(buf, ":{}", col)?;
            }

            buf.write(b"\x1b[K")?;
        }

        for y in help_height..rows {
            write!(buf, "\x1b[{}H", y + 1)?;
            buf.write(b"\x1b[K")?;
        }

        self.write_flush(&buf)
    }

    pub fn set_dirty_start(&mut self, start: usize) {
        if let Some(s) = self.dirty_start {
            if s < start {
                return;
            }
        }
        self.dirty_start = Some(start);
    }

    pub fn maybe_resize<I>(&mut self, input: I) -> Result<bool>
    where
        I: Iterator<Item = Result<InputSeq>>,
    {
        if !self.sigwinch.notified() {
            return Ok(false); // Did not receive signal
        }

        let (w, h) = get_window_size(input, &mut self.output)?;
        self.num_rows = h.saturating_sub(2);
        self.num_cols = w;
        self.dirty_start = Some(0);
        Ok(true)
    }

    fn set_message(&mut self, m: Option<StatusMessage>) {
        let op = match (&self.message, &m) {
            (Some(p), Some(n)) if p.text == n.text => DrawMessage::DoNothing,
            (Some(_), Some(_)) => DrawMessage::Update,
            (None, Some(_)) => DrawMessage::Open,
            (Some(_), None) => DrawMessage::Close,
            (None, None) => DrawMessage::DoNothing,
        };
        // Folding is necessary to consider that message is set multiple times within one tick
        self.draw_message = self.draw_message.fold(op);
        self.message = m;
    }

    pub fn set_info_message<S: Into<String>>(&mut self, message: S) {
        self.set_message(Some(StatusMessage::new(message, StatusMessageKind::Info)));
    }

    pub fn set_error_message<S: Into<String>>(&mut self, message: S) {
        self.set_message(Some(StatusMessage::new(message, StatusMessageKind::Error)));
    }

    pub fn unset_message(&mut self) {
        self.set_message(None);
    }

    pub fn rows(&self) -> usize {
        if self.message.is_some() {
            self.num_rows
        } else {
            self.num_rows + 1
        }
    }

    pub fn cols(&self) -> usize {
        self.num_cols
    }

    pub fn message_text(&self) -> &'_ str {
        self.message.as_ref().map(|m| m.text.as_str()).unwrap_or("")
    }

    pub fn force_set_cursor(&mut self, row: usize, col: usize) -> Result<()> {
        write!(self.output, "\x1b[{};{}H", row, col)?;
        self.output.flush()?;
        Ok(())
    }
}

impl<W: Write> Drop for Screen<W> {
    fn drop(&mut self) {
        // Back to normal screen buffer from alternate screen buffer
        // https://www.xfree86.org/current/ctlseqs.html#The%20Alternate%20Screen%20Buffer
        // Note that we used \x1b[2J\x1b[H previously but it did not erase screen.
        self.write_flush(b"\x1b[?47l\x1b[H")
            .expect("Back to normal screen buffer");
    }
}
