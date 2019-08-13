// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

use std::cmp;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::str;

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
        InputSequences { stdin: self }
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

// TODO: Add queue to buffer read input to look ahead user input. It is necessary when reading
// \x1b but succeeding byte is not b'['.
struct InputSequences {
    stdin: StdinRawMode,
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
                match self.read()? {
                    b'[' => { /* fall throught */ }
                    0 => return Ok(InputSeq::Key(0x1b, false)),
                    b => return self.decode(b), // TODO: First escape character is squashed. Buffer it
                };

                // Now confirmed \1xb[ which is a header of escape sequence. Eat it until the end
                // of sequence with blocking
                let mut buf = vec![];
                let cmd = loop {
                    let b = self.read_blocking()?;
                    match b {
                        b'R' | b'A' | b'B' | b'C' | b'D' | b'~' | b'F' | b'H' => break b,
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
        let b = self.read()?;
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

struct Row {
    chars: String,
    render: String,
}

impl Row {
    fn new(line: String) -> Row {
        let mut render = String::with_capacity(line.len());
        let mut index = 0;
        for c in line.chars() {
            if c == '\t' {
                loop {
                    render.push(' ');
                    index += 1;
                    if index % TAB_STOP == 0 {
                        break;
                    }
                }
            } else {
                render.push(c);
                index += 1;
            }
        }
        Row {
            chars: line,
            render,
        }
    }

    fn rx_from_cx(&self, cx: usize) -> usize {
        // TODO: Consider UTF-8 character width
        self.chars.chars().take(cx).fold(0, |rx, ch| {
            if ch == '\t' {
                // Proceed TAB_STOP spaces then subtract spaces by mod TAB_STOP
                rx + TAB_STOP - (rx % TAB_STOP)
            } else {
                rx + 1
            }
        })
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
    // (x, y) coordinate in `chars` buffer of rows
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
}

impl Editor {
    fn new(size: Option<(usize, usize)>) -> Editor {
        let (screen_cols, screen_rows) = size.unwrap_or((0, 0));
        Editor {
            cx: 0,
            cy: 0,
            rx: 0,
            screen_cols,
            screen_rows,
            row: Vec::with_capacity(screen_rows),
            rowoff: 0,
            coloff: 0,
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

            // Avoid adding newline at the end of buffer
            if y < self.screen_rows - 1 {
                buf.write(b"\r\n")?;
            }
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

    fn open_file<P: AsRef<Path>>(&mut self, file: P) -> io::Result<()> {
        let file = fs::File::open(file)?;
        for line in io::BufReader::new(file).lines() {
            self.row.push(Row::new(line?));
        }
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

    fn move_cursor(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Up => self.cy = self.cy.saturating_sub(1),
            CursorDir::Left => {
                if self.cx > 0 {
                    self.cx -= 1;
                } else if self.cy > 0 {
                    // When moving to left at top of line, move cursor to end of previous line
                    self.cy -= 1;
                    self.cx = self.row[self.cy].chars.len();
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
                    let len = self.row[self.cy].chars.len();
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
        let len = self.row.get(self.cy).map(|r| r.chars.len()).unwrap_or(0);
        if self.cx > len {
            self.cx = len;
        }
    }

    fn process_sequence(&mut self, seq: InputSeq) -> io::Result<bool> {
        let mut exit = false;
        match seq {
            InputSeq::Key(b'w', false) | InputSeq::UpKey => self.move_cursor(CursorDir::Up),
            InputSeq::Key(b'a', false) | InputSeq::LeftKey => self.move_cursor(CursorDir::Left),
            InputSeq::Key(b's', false) | InputSeq::DownKey => self.move_cursor(CursorDir::Down),
            InputSeq::Key(b'd', false) | InputSeq::RightKey => self.move_cursor(CursorDir::Right),
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
                self.screen_rows = r;
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
            self.redraw_screen()?;
            if self.process_sequence(seq?)? {
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
