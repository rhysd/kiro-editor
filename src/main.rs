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
    fn read(&mut self) -> io::Result<u8> {
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
                match self.read()? {
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
            0x01..=0x1f => Ok(InputSeq::Key(b | 0b1100000, true)),
            _ => Ok(InputSeq::Unidentified), // TODO: 0x80..=0xff => { ... } Handle UTF-8
        }
    }

    fn read_seq(&mut self) -> io::Result<InputSeq> {
        let b = match self.next_byte {
            0 => self.read()?,
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
    fn new(text: String) -> StatusMessage {
        StatusMessage {
            text,
            timestamp: SystemTime::now(),
        }
    }
}

struct Row {
    buf: String,
    render: String,
}

impl Row {
    fn new(line: String) -> Row {
        let mut row = Row {
            buf: line,
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

    // Note: 'at' is an index of buffer, not render text
    fn insert_char(&mut self, at: usize, c: char) {
        if self.buf.len() < at {
            self.buf.push(c);
        } else {
            self.buf.insert(at, c);
        }
        self.update_render();
    }
}

enum CursorDir {
    Left,
    Right,
    Up,
    Down,
}

struct Editor {
    // Editor state goes here
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
}

impl Editor {
    fn new(window_size: Option<(usize, usize)>) -> Editor {
        let (w, h) = window_size.unwrap_or((0, 0));
        Editor {
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
            message: StatusMessage::new("HELP: Ctrl-S = save | Ctrl-Q = quit".to_string()),
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

        let left = format!("{:<20?} - {} lines", file, self.row.len());
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
        Ok(())
    }

    fn save(&mut self) -> io::Result<()> {
        let ref file = if let Some(ref file) = self.file {
            file
        } else {
            self.message = StatusMessage::new("No file name".to_string());
            return Ok(());
        };

        let mut f = io::BufWriter::new(fs::File::create(&file.path)?);
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
        Ok(())
    }

    fn scroll(&mut self) {
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

    fn process_keypress(&mut self, seq: InputSeq) -> io::Result<bool> {
        let mut exit = false;
        match seq {
            InputSeq::UpKey => self.move_cursor(CursorDir::Up),
            InputSeq::LeftKey => self.move_cursor(CursorDir::Left),
            InputSeq::DownKey => self.move_cursor(CursorDir::Down),
            InputSeq::RightKey => self.move_cursor(CursorDir::Right),
            InputSeq::PageUpKey => {
                self.cy = self.rowoff; // Set cursor to top of screen
                for _ in 0..self.screen_rows {
                    self.move_cursor(CursorDir::Up);
                }
            }
            InputSeq::PageDownKey => {
                // Set cursor to bottom of screen considering end of buffer
                self.cy = cmp::min(self.rowoff + self.screen_rows - 1, self.row.len());
                for _ in 0..self.screen_rows {
                    self.move_cursor(CursorDir::Down)
                }
            }
            InputSeq::HomeKey => self.cx = 0,
            InputSeq::EndKey => {
                if self.cy < self.row.len() {
                    self.cx = self.screen_cols - 1;
                }
            }
            InputSeq::DeleteKey => unimplemented!("delete key press"),
            InputSeq::Key(b'q', true) => exit = true,
            InputSeq::Key(b'\r', false) => unimplemented!(),
            InputSeq::Key(b'h', true) | InputSeq::Key(0x08, false) | InputSeq::Key(0x1f, false) => {
                // On Ctrl-h or Backspace, remove char at cursor. Note that Delete key is mapped to \x1b[3~
                unimplemented!();
            }
            InputSeq::Key(b'l', true) | InputSeq::Key(0x1b, false) => {
                // Our editor refresh screen after any key
            }
            InputSeq::Key(b's', true) => self.save()?,
            InputSeq::Key(b, false) => self.insert_char(b as char),
            _ => {}
        }
        Ok(exit)
    }

    fn ensure_screen_size<I>(&mut self, mut input: I) -> io::Result<I>
    where
        I: Iterator<Item = io::Result<InputSeq>>,
    {
        if self.screen_cols > 0 && self.screen_rows > 0 {
            return Ok(input);
        }

        // By moving cursor at the bottom-right corner by 'B' and 'C' commands, get the size of
        // current screen. \x1b[9999;9999H is not available since it does not guarantee cursor
        // stops on the corner. Finaly command 'n' queries cursor position.
        let mut stdout = io::stdout();
        stdout.write(b"\x1b[9999C\x1b[9999B\x1b[6n")?;
        stdout.flush()?;

        // Wait for response from terminal discarding other sequences
        for seq in &mut input {
            if let InputSeq::Cursor(r, c) = seq? {
                self.screen_cols = c;
                self.screen_rows = r.saturating_sub(2);
                break;
            }
        }

        Ok(input)
    }

    fn run<I>(&mut self, input: I) -> io::Result<()>
    where
        I: Iterator<Item = io::Result<InputSeq>>,
    {
        let input = self.ensure_screen_size(input)?;

        for seq in input {
            self.scroll();
            self.refresh_screen()?;
            if self.process_keypress(seq?)? {
                break;
            }
        }

        self.clear_screen() // Finally clear screen on exit
    }
}

fn main() -> io::Result<()> {
    let mut editor = Editor::new(term_size::dimensions_stdout());
    if let Some(arg) = std::env::args().skip(1).next() {
        editor.open_file(arg)?;
    }
    editor.run(StdinRawMode::new()?.input_keys())
}
