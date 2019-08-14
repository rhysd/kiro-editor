// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

use std::cmp;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::str;
use std::time::SystemTime;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const TAB_STOP: usize = 8;
const HELP_TEXT: &'static str = "HELP: ^S = save | ^Q = quit | ^G = find | ^? = help";

struct StdinRawMode {
    stdin: io::Stdin,
    orig: termios::Termios,
}

// TODO: Separate editor into frontend and backend. In frontend, it handles actual screen and user input.
// It interacts with backend by responding to request from frontend. Frontend focues on core editor
// logic. This is useful when adding a new frontend (e.g. wasm).

impl StdinRawMode {
    fn new() -> io::Result<StdinRawMode> {
        use termios::*;

        let stdin = io::stdin();
        let fd = stdin.as_raw_fd();
        let mut termios = Termios::from_fd(fd)?;
        let orig = termios.clone();

        // Set terminal raw mode. Disable echo back, canonical mode, signals (SIGINT, SIGTSTP) and Ctrl+V.
        termios.c_lflag &= !(ECHO | ICANON | ISIG | IEXTEN);
        // Disable control flow mode (Ctrl+Q/Ctrl+S) and CR-to-NL translation
        termios.c_iflag &= !(IXON | ICRNL | BRKINT | INPCK | ISTRIP);
        // Disable output processing such as \n to \r\n translation
        termios.c_oflag &= !OPOST;
        // Ensure character size is 8bits
        termios.c_cflag |= CS8;
        // Do not wait for next byte with blocking since reading 0 byte is permitted
        termios.c_cc[VMIN] = 0;
        // Set read timeout to 1/10 second it enables 100ms timeout on read()
        termios.c_cc[VTIME] = 1;
        // Apply terminal configurations
        tcsetattr(fd, TCSAFLUSH, &mut termios)?;

        Ok(StdinRawMode { stdin, orig })
    }

    fn input_keys(self) -> InputSequences {
        InputSequences {
            stdin: self,
            next_byte: 0,
        }
    }
}

impl Drop for StdinRawMode {
    fn drop(&mut self) {
        // Restore original terminal mode
        termios::tcsetattr(self.stdin.as_raw_fd(), termios::TCSAFLUSH, &mut self.orig).unwrap();
    }
}

impl Deref for StdinRawMode {
    type Target = io::Stdin;

    fn deref(&self) -> &Self::Target {
        &self.stdin
    }
}

impl DerefMut for StdinRawMode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stdin
    }
}

#[derive(PartialEq, Debug)]
enum InputSeq {
    Unidentified,
    // TODO: Add Utf8Key(char),
    Key(u8, bool), // Char code and ctrl mod
    LeftKey,
    RightKey,
    UpKey,
    DownKey,
    PageUpKey,
    PageDownKey,
    HomeKey,
    EndKey,
    DeleteKey,
    Cursor(usize, usize),
}

struct InputSequences {
    stdin: StdinRawMode,
    next_byte: u8, // Reading sequence sometimes requires looking ahead 1 byte
}

impl InputSequences {
    fn read_byte(&mut self) -> io::Result<u8> {
        // TODO: Regarding 0 as timeout conflicts with Ctrl-`
        let mut one_byte: [u8; 1] = [0];
        self.stdin.read(&mut one_byte)?;
        Ok(one_byte[0])
    }

    fn read_blocking(&mut self) -> io::Result<u8> {
        let mut one_byte: [u8; 1] = [0];
        loop {
            if self.stdin.read(&mut one_byte)? > 0 {
                return Ok(one_byte[0]);
            }
        }
    }

    fn decode(&mut self, b: u8) -> io::Result<InputSeq> {
        match b {
            // (Maybe) Escape sequence
            0x1b => {
                // Try to read expecting '[' as escape sequence header. Note that, if next input does
                // not arrive within next tick, it means that it is not an escape sequence.
                // TODO?: Should we consider sequences not starting with '['?
                match self.read_byte()? {
                    b'[' => { /* fall throught */ }
                    0 => return Ok(InputSeq::Key(0x1b, false)),
                    b => {
                        self.next_byte = b; // Already read the next byte so remember it
                        return Ok(InputSeq::Key(0x1b, false));
                    }
                };

                // Now confirmed \1xb[ which is a header of escape sequence. Eat it until the end
                // of sequence with blocking
                let mut buf = vec![];
                let cmd = loop {
                    let b = self.read_blocking()?;
                    match b {
                        // Control command chars from http://ascii-table.com/ansi-escape-sequences-vt-100.php
                        b'A' | b'B' | b'C' | b'D' | b'F' | b'H' | b'K' | b'J' | b'R' | b'c'
                        | b'f' | b'g' | b'h' | b'l' | b'm' | b'n' | b'q' | b'y' | b'~' => break b,
                        b'O' => {
                            buf.push(b'O');
                            let b = self.read_blocking()?;
                            match b {
                                b'F' | b'H' => break b, // OF/OH are the same as F/H
                                _ => buf.push(b),
                            };
                        }
                        _ => buf.push(b),
                    }
                };

                let mut args = buf.split(|b| *b == b';');
                match cmd {
                    b'R' => {
                        // https://vt100.net/docs/vt100-ug/chapter3.html#CPR e.g. \x1b[24;80R
                        let mut i = args
                            .map(|b| str::from_utf8(b).ok().and_then(|s| s.parse::<usize>().ok()));
                        match (i.next(), i.next()) {
                            (Some(Some(r)), Some(Some(c))) => Ok(InputSeq::Cursor(r, c)),
                            _ => Ok(InputSeq::Unidentified),
                        }
                    }
                    b'A' => Ok(InputSeq::UpKey),
                    b'B' => Ok(InputSeq::DownKey),
                    b'C' => Ok(InputSeq::RightKey),
                    b'D' => Ok(InputSeq::LeftKey),
                    b'~' => {
                        // e.g. \x1b[5~
                        match args.next() {
                            Some(b"5") => Ok(InputSeq::PageUpKey),
                            Some(b"6") => Ok(InputSeq::PageDownKey),
                            Some(b"1") | Some(b"7") => Ok(InputSeq::HomeKey),
                            Some(b"4") | Some(b"8") => Ok(InputSeq::EndKey),
                            Some(b"3") => Ok(InputSeq::DeleteKey),
                            _ => Ok(InputSeq::Unidentified),
                        }
                    }
                    b'H' => Ok(InputSeq::HomeKey),
                    b'F' => Ok(InputSeq::EndKey),
                    _ => unreachable!(),
                }
            }
            // Ascii key inputs
            0x20..=0x7f => Ok(InputSeq::Key(b, false)),
            // 0x01~0x1f keys are ascii keys with ctrl. Ctrl mod masks key with 0b11111.
            // Here unmask it with 0b1100000. It only works with 0x61~0x7f.
            0x01..=0x1e => Ok(InputSeq::Key(b | 0b1100000, true)),
            // Ctrl-?
            0x1f => Ok(InputSeq::Key(b | 0b0100000, true)),
            _ => Ok(InputSeq::Unidentified), // TODO: 0x80..=0xff => { ... } Handle UTF-8
        }
    }

    fn read_seq(&mut self) -> io::Result<InputSeq> {
        let b = match self.next_byte {
            0 => self.read_byte()?,
            b => {
                self.next_byte = 0; // Next byte was read for looking ahead
                b
            }
        };
        self.decode(b)
    }
}

impl Iterator for InputSequences {
    type Item = io::Result<InputSeq>;

    // Read next byte from stdin with timeout 100ms. If nothing was read, it returns InputSeq::Unidentified.
    // This method never returns None so for loop never ends
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.read_seq())
    }
}

// Contain both actual path sequence and display string
struct FilePath {
    path: PathBuf,
    display: String,
}

impl FilePath {
    fn from<P: AsRef<Path>>(path: P) -> FilePath {
        let path = path.as_ref();
        FilePath {
            path: PathBuf::from(path),
            display: path.to_string_lossy().to_string(),
        }
    }
}

struct StatusMessage {
    text: String,
    timestamp: SystemTime,
}

impl StatusMessage {
    fn new<S: Into<String>>(message: S) -> StatusMessage {
        StatusMessage {
            text: message.into(),
            timestamp: SystemTime::now(),
        }
    }
}

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

struct Row {
    buf: String,
    render: String,
}

impl Row {
    fn new<S: Into<String>>(line: S) -> Row {
        let mut row = Row {
            buf: line.into(),
            render: "".to_string(),
        };
        row.update_render();
        row
    }

    fn empty() -> Row {
        Row {
            buf: "".to_string(),
            render: "".to_string(),
        }
    }

    fn update_render(&mut self) {
        self.render = String::with_capacity(self.buf.len());
        let mut index = 0;
        for c in self.buf.chars() {
            if c == '\t' {
                loop {
                    self.render.push(' ');
                    index += 1;
                    if index % TAB_STOP == 0 {
                        break;
                    }
                }
            } else {
                self.render.push(c);
                index += 1;
            }
        }
    }

    fn rx_from_cx(&self, cx: usize) -> usize {
        // TODO: Consider UTF-8 character width
        self.buf.chars().take(cx).fold(0, |rx, ch| {
            if ch == '\t' {
                // Proceed TAB_STOP spaces then subtract spaces by mod TAB_STOP
                rx + TAB_STOP - (rx % TAB_STOP)
            } else {
                rx + 1
            }
        })
    }

    fn cx_from_rx(&self, rx: usize) -> usize {
        // TODO: Consider UTF-8 character width
        let mut current_rx = 0;
        for (cx, ch) in self.buf.chars().enumerate() {
            if ch == '\t' {
                current_rx += TAB_STOP - (current_rx % TAB_STOP);
            } else {
                current_rx += 1;
            }
            if current_rx > rx {
                return cx; // Found
            }
        }
        self.buf.len() // Fall back to end of line
    }

    // Note: 'at' is an index of buffer, not render text
    fn insert_char(&mut self, at: usize, c: char) {
        if self.buf.len() <= at {
            self.buf.push(c);
        } else {
            self.buf.insert(at, c);
        }
        self.update_render();
    }

    fn delete_char(&mut self, at: usize) {
        if at < self.buf.len() {
            self.buf.remove(at);
            self.update_render();
        }
    }

    fn append<S: AsRef<str>>(&mut self, s: S) {
        let s = s.as_ref();
        if s.is_empty() {
            return;
        }
        self.buf.push_str(s);
        self.update_render();
    }

    fn truncate(&mut self, at: usize) {
        if at < self.buf.len() {
            self.buf.truncate(at);
            self.update_render();
        }
    }
}

enum CursorDir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(PartialEq)]
enum AfterKeyPress {
    Quit,
    Continue,
}

struct Editor<I: Iterator<Item = io::Result<InputSeq>>> {
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
    dirty: bool,
    // After first Ctrl-Q
    quitting: bool,
    // Text search state
    finding: FindState,
}

impl<I: Iterator<Item = io::Result<InputSeq>>> Editor<I> {
    fn new(window_size: Option<(usize, usize)>, input: I) -> Editor<I> {
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
            message: StatusMessage::new(HELP_TEXT),
            dirty: false,
            quitting: false,
            finding: FindState::new(),
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
        // 'm' sets attributes to text printed after. '7' means inverting color. https://vt100.net/docs/vt100-ug/chapter3.html#SGR
        buf.write(b"\x1b[7m")?;

        let file = if let Some(ref f) = self.file {
            f.display.as_str()
        } else {
            "[No Name]"
        };

        let modified = if self.dirty { "(modified) " } else { "" };
        let left = format!("{:<20?} - {} lines {}", file, self.row.len(), modified);
        let left = &left[..cmp::min(left.len(), self.screen_cols)];
        buf.write(left.as_bytes())?; // Left of status bar

        let rest_len = self.screen_cols - left.len();
        if rest_len == 0 {
            return Ok(());
        }

        let right = format!("{}/{}", self.cy, self.row.len());
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
        buf.write(b"\x1b[m")?;
        buf.write(b"\r\n")?; // Newline for message bar
        Ok(())
    }

    fn draw_message_bar<W: Write>(&self, mut buf: W) -> io::Result<()> {
        if let Ok(d) = SystemTime::now().duration_since(self.message.timestamp) {
            if d.as_secs() < 5 {
                let msg = &self.message.text[..cmp::min(self.message.text.len(), self.screen_cols)];
                buf.write(msg.as_bytes())?;
            }
        }
        buf.write(b"\x1b[K")?;
        Ok(())
    }

    fn draw_rows<W: Write>(&self, mut buf: W) -> io::Result<()> {
        for y in 0..self.screen_rows {
            let file_row = y + self.rowoff;
            if file_row >= self.row.len() {
                if self.row.is_empty() && y == self.screen_rows / 3 {
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
                } else {
                    buf.write(b"~")?;
                }
            } else {
                let line = self.trim_line(&self.row[file_row].render);
                buf.write(line.as_bytes())?;
            }

            // Erases the part of the line to the right of the cursor. http://vt100.net/docs/vt100-ug/chapter3.html#EL
            buf.write(b"\x1b[K")?;
            buf.write(b"\r\n")?; // Finally go to next line.
        }
        Ok(())
    }

    fn refresh_screen(&self) -> io::Result<()> {
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

    fn clear_screen(&self) -> io::Result<()> {
        let mut stdout = io::stdout();
        // 2: Argument of 'J' command to reset entire screen
        // J: Command to erase screen http://vt100.net/docs/vt100-ug/chapter3.html#ED
        stdout.write(b"\x1b[2J")?;
        // Set cursor position to left-top corner
        stdout.write(b"\x1b[H")?;
        stdout.flush()
    }

    fn open_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        let file = fs::File::open(path)?;
        for line in io::BufReader::new(file).lines() {
            self.row.push(Row::new(line?));
        }
        self.file = Some(FilePath::from(path));
        self.dirty = false;
        Ok(())
    }

    fn save(&mut self) -> io::Result<()> {
        let mut create = false;
        if self.file.is_none() {
            if let Some(input) =
                self.prompt("Save as: {} (^G or ESC to cancel)", |_, _, _, _| {})?
            {
                self.file = Some(FilePath {
                    path: PathBuf::from(&input),
                    display: input,
                });
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
                self.message = StatusMessage::new(format!("Could not save: {}", e));
                if create {
                    self.file = None; // Could not make file. Back to unnamed buffer
                }
                return Ok(()); // This is not a fatal error
            }
        };
        let mut f = io::BufWriter::new(f);
        let mut bytes = 0;
        for line in self.row.iter() {
            let b = line.buf.as_bytes();
            f.write(b)?;
            f.write(b"\n")?;
            bytes += b.len() + 1;
        }
        f.flush()?;

        let msg = format!("{} bytes written to {}", bytes, &file.display);
        self.message = StatusMessage::new(msg);
        self.dirty = false;
        Ok(())
    }

    fn on_incremental_find(&mut self, query: &str, key: InputSeq, end: bool) {
        if end {
            return;
        }

        match key {
            InputSeq::RightKey
            | InputSeq::DownKey
            | InputSeq::Key(b'f', true)
            | InputSeq::Key(b'n', true) => self.finding.dir = FindDir::Forward,
            InputSeq::LeftKey
            | InputSeq::UpKey
            | InputSeq::Key(b'b', true)
            | InputSeq::Key(b'p', true) => self.finding.dir = FindDir::Back,
            _ => self.finding = FindState::new(),
        }

        let row_len = self.row.len();
        let mut y = self.finding.last_match.unwrap_or(self.cy);
        for _ in 0..row_len {
            // Wrapping text search at top/bottom of text buffer
            y = match self.finding.dir {
                FindDir::Forward if y == row_len - 1 => 0,
                FindDir::Forward => y + 1,
                FindDir::Back if y == 0 => row_len - 1,
                FindDir::Back => y - 1,
            };

            let row = &self.row[y];
            if let Some(rx) = row.render.find(query) {
                self.cy = y;
                self.cx = row.cx_from_rx(rx);
                // Cause setup_scroll() to scroll upwards to the matching line at next screen redraw
                self.rowoff = self.row.len();
                self.finding.last_match = Some(y);
                break;
            }
        }
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
        } else {
            let msg = if self.finding.last_match.is_some() {
                "Found"
            } else {
                "Not Found"
            };
            self.message = StatusMessage::new(msg);
        }

        self.finding = FindState::new(); // Clear text search state for next time
        Ok(())
    }

    fn setup_scroll(&mut self) {
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
    }

    fn insert_char(&mut self, ch: char) {
        if self.cy == self.row.len() {
            self.row.push(Row::empty());
        }
        self.row[self.cy].insert_char(self.cx, ch);
        self.cx += 1;
        self.dirty = true;
    }

    fn delete_char(&mut self) {
        if self.cy == self.row.len() || self.cx == 0 && self.cy == 0 {
            return;
        }
        if self.cx > 0 {
            self.row[self.cy].delete_char(self.cx - 1);
            self.cx -= 1;
        } else {
            // At top of line, backspace concats current line to previous line
            self.cx = self.row[self.cy - 1].buf.len(); // Move cursor column to end of previous line
            let row = self.row.remove(self.cy);
            self.cy -= 1; // Move cursor to previous line
            self.row[self.cy].append(row.buf);
        }
        self.dirty = true;
    }

    fn insert_line(&mut self) {
        if self.cy >= self.row.len() {
            self.row.push(Row::new(""));
        } else if self.cx >= self.row[self.cy].buf.len() {
            self.row.insert(self.cy + 1, Row::new(""));
        } else {
            let split = String::from(&self.row[self.cy].buf[self.cx..]);
            self.row[self.cy].truncate(self.cx);
            self.row.insert(self.cy + 1, Row::new(split));
        }
        self.cy += 1;
        self.cx = 0;
    }

    fn move_cursor(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Up => self.cy = self.cy.saturating_sub(1),
            CursorDir::Left => {
                if self.cx > 0 {
                    self.cx -= 1;
                } else if self.cy > 0 {
                    // When moving to left at top of line, move cursor to end of previous line
                    self.cy -= 1;
                    self.cx = self.row[self.cy].buf.len();
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
                    let len = self.row[self.cy].buf.len();
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
        let len = self.row.get(self.cy).map(|r| r.buf.len()).unwrap_or(0);
        if self.cx > len {
            self.cx = len;
        }
    }

    fn prompt<S, F>(&mut self, prompt: S, mut incremental_callback: F) -> io::Result<Option<String>>
    where
        S: AsRef<str>,
        F: FnMut(&mut Self, &str, InputSeq, bool),
    {
        let mut buf = String::new();
        let prompt = prompt.as_ref();
        self.message = StatusMessage::new(prompt.replacen("{}", "", 1));
        self.setup_scroll();
        self.refresh_screen()?;

        while let Some(seq) = self.input.next() {
            let key = seq?;
            match key {
                InputSeq::Unidentified => continue,
                InputSeq::Key(b'h', true) | InputSeq::Key(0x7f, false) | InputSeq::DeleteKey
                    if !buf.is_empty() =>
                {
                    buf.pop();
                }
                k @ InputSeq::Key(b'g', true) | k @ InputSeq::Key(0x1b, false) => {
                    self.message = StatusMessage::new("Canceled.");
                    incremental_callback(self, buf.as_str(), k, true);
                    return Ok(None);
                }
                k @ InputSeq::Key(b'\r', false) | k @ InputSeq::Key(b'm', true) => {
                    incremental_callback(self, buf.as_str(), k, true);
                    break;
                }
                InputSeq::Key(b, false) => {
                    buf.push(b as char);
                }
                _ => {}
            }

            incremental_callback(self, buf.as_str(), key, false);

            self.message = StatusMessage::new(prompt.replacen("{}", &buf, 1));
            self.setup_scroll();
            self.refresh_screen()?;
        }

        let input = if buf.is_empty() { None } else { Some(buf) };

        self.message = StatusMessage::new("");
        Ok(input)
    }

    fn process_keypress(&mut self, seq: InputSeq) -> io::Result<AfterKeyPress> {
        match seq {
            InputSeq::Key(b'p', true) | InputSeq::UpKey => self.move_cursor(CursorDir::Up),
            InputSeq::Key(b'b', true) | InputSeq::LeftKey => self.move_cursor(CursorDir::Left),
            InputSeq::Key(b'n', true) | InputSeq::DownKey => self.move_cursor(CursorDir::Down),
            InputSeq::Key(b'f', true) | InputSeq::RightKey => self.move_cursor(CursorDir::Right),
            InputSeq::PageUpKey => {
                self.cy = self.rowoff; // Set cursor to top of screen
                for _ in 0..self.screen_rows {
                    self.move_cursor(CursorDir::Up);
                }
            }
            InputSeq::Key(b'v', true) | InputSeq::PageDownKey => {
                // Set cursor to bottom of screen considering end of buffer
                self.cy = cmp::min(self.rowoff + self.screen_rows - 1, self.row.len());
                for _ in 0..self.screen_rows {
                    self.move_cursor(CursorDir::Down)
                }
            }
            InputSeq::Key(b'a', true) | InputSeq::HomeKey => self.cx = 0,
            InputSeq::Key(b'e', true) | InputSeq::EndKey => {
                if self.cy < self.row.len() {
                    self.cx = self.screen_cols - 1;
                }
            }
            InputSeq::Key(b'd', true) | InputSeq::DeleteKey => {
                self.move_cursor(CursorDir::Right);
                self.delete_char();
            }
            InputSeq::Key(b'g', true) => self.find()?,
            InputSeq::Key(b'q', true) => {
                if !self.dirty || self.quitting {
                    return Ok(AfterKeyPress::Quit);
                } else {
                    self.quitting = true;
                    self.message =
                        StatusMessage::new("File has unsaved changes! Press ^Q again to quit");
                    return Ok(AfterKeyPress::Continue);
                }
            }
            InputSeq::Key(b'\r', false) | InputSeq::Key(b'm', true) => self.insert_line(),
            InputSeq::Key(b'h', true) | InputSeq::Key(0x08, false) | InputSeq::Key(0x7f, false) => {
                // On Ctrl-h or Backspace, remove char at cursor. Note that Delete key is mapped to \x1b[3~
                self.delete_char();
            }
            InputSeq::Key(b'l', true) | InputSeq::Key(0x1b, false) => {
                // Our editor refresh screen after any key
            }
            InputSeq::Key(b'?', true) => {
                self.message = StatusMessage::new(HELP_TEXT);
            }
            InputSeq::Key(b's', true) => self.save()?,
            InputSeq::Key(b, false) => self.insert_char(b as char),
            InputSeq::Key(..) => { /* ignore other key inputs */ }
            _ => unreachable!(),
        }

        self.quitting = false;
        Ok(AfterKeyPress::Continue)
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
            if let InputSeq::Cursor(r, c) = seq? {
                self.screen_cols = c;
                self.screen_rows = r.saturating_sub(2);
                break;
            }
        }

        Ok(())
    }

    fn run(&mut self) -> io::Result<()> {
        self.ensure_screen_size()?;

        // Render first screen
        self.setup_scroll();
        self.refresh_screen()?;

        while let Some(seq) = self.input.next() {
            let seq = seq?;
            if seq == InputSeq::Unidentified {
                continue; // Ignore
            }
            if self.process_keypress(seq)? == AfterKeyPress::Quit {
                break;
            }
            self.setup_scroll();
            self.refresh_screen()?; // Update screen after keypress
        }

        self.clear_screen() // Finally clear screen on exit
    }
}

fn main() -> io::Result<()> {
    let input = StdinRawMode::new()?.input_keys();
    let mut editor = Editor::new(term_size::dimensions_stdout(), input);
    if let Some(arg) = std::env::args().skip(1).next() {
        editor.open_file(arg)?;
    }
    editor.run()
}
