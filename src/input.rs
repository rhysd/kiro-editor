use crate::error::Result;
use std::fmt;
use std::io::{self, Read};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use std::str;

pub struct StdinRawMode {
    stdin: io::Stdin,
    orig: termios::Termios,
}

// TODO: Separate editor into frontend and backend. In frontend, it handles actual screen and user input.
// It interacts with backend by responding to request from frontend. Frontend focues on core editor
// logic. This is useful when adding a new frontend (e.g. wasm).

impl StdinRawMode {
    pub fn new() -> Result<StdinRawMode> {
        use termios::*;

        let stdin = io::stdin();
        let fd = stdin.as_raw_fd();
        let mut termios = Termios::from_fd(fd)?;
        let orig = termios;

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
        tcsetattr(fd, TCSAFLUSH, &termios)?;

        Ok(StdinRawMode { stdin, orig })
    }

    pub fn input_keys(self) -> InputSequences {
        InputSequences { stdin: self }
    }
}

impl Drop for StdinRawMode {
    fn drop(&mut self) {
        // Restore original terminal mode
        termios::tcsetattr(self.stdin.as_raw_fd(), termios::TCSAFLUSH, &self.orig).unwrap();
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

#[derive(PartialEq, Debug, Clone)]
pub enum KeySeq {
    Unidentified,
    Utf8Key(char),
    Key(u8), // Char code and ctrl mod
    LeftKey,
    RightKey,
    UpKey,
    DownKey,
    PageUpKey,
    PageDownKey,
    HomeKey,
    EndKey,
    DeleteKey,
    Cursor(usize, usize), // Pseudo key (x, y)
}

impl fmt::Display for KeySeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use KeySeq::*;
        match self {
            Unidentified => write!(f, "UNKNOWN"),
            Key(b' ') => write!(f, "SPACE"),
            Key(b) if b.is_ascii_control() => write!(f, "\\x{:x}", b),
            Key(b) => write!(f, "{}", *b as char),
            Utf8Key(c) => write!(f, "{}", c),
            LeftKey => write!(f, "LEFT"),
            RightKey => write!(f, "RIGHT"),
            UpKey => write!(f, "UP"),
            DownKey => write!(f, "DOWN"),
            PageUpKey => write!(f, "PAGEUP"),
            PageDownKey => write!(f, "PAGEDOWN"),
            HomeKey => write!(f, "HOME"),
            EndKey => write!(f, "END"),
            DeleteKey => write!(f, "DELETE"),
            Cursor(r, c) => write!(f, "CURSOR({},{})", r, c),
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct InputSeq {
    pub key: KeySeq,
    pub ctrl: bool,
    pub alt: bool,
}

impl InputSeq {
    pub fn new(key: KeySeq) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
        }
    }

    pub fn ctrl(key: KeySeq) -> Self {
        Self {
            key,
            ctrl: true,
            alt: false,
        }
    }
}

impl fmt::Display for InputSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            write!(f, "C-")?;
        }
        if self.alt {
            write!(f, "M-")?;
        }
        write!(f, "{}", self.key)
    }
}

pub struct InputSequences {
    stdin: StdinRawMode,
}

impl InputSequences {
    fn read_byte(&mut self) -> Result<Option<u8>> {
        let mut one_byte: [u8; 1] = [0];
        Ok(if self.stdin.read(&mut one_byte)? == 0 {
            None
        } else {
            Some(one_byte[0])
        })
    }

    fn decode_escape_sequence(&mut self) -> Result<InputSeq> {
        use KeySeq::*;

        // Try to read expecting '[' as escape sequence header. Note that, if next input does
        // not arrive within next tick, it means that it is not an escape sequence.
        // TODO?: Should we consider sequences not starting with '['?
        match self.read_byte()? {
            Some(b'[') => { /* fall thought */ }
            Some(b) if b.is_ascii_control() => return Ok(InputSeq::new(Key(0x1b))), // Ignore control characters after ESC
            Some(b) => {
                // Alt key is sent as ESC prefix (e.g. Alt-A => \x1b\x61
                // https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Alt-and-Meta-Keys
                let mut seq = self.decode(b)?;
                seq.alt = true;
                return Ok(seq);
            }
            None => return Ok(InputSeq::new(Key(0x1b))),
        };

        // Now confirmed \1xb[ which is a header of escape sequence. Eat it until the end
        // of sequence with blocking
        let mut buf = vec![];
        let cmd = loop {
            if let Some(b) = self.read_byte()? {
                match b {
                    // Control command chars from http://ascii-table.com/ansi-escape-sequences-vt-100.php
                    b'A' | b'B' | b'C' | b'D' | b'F' | b'H' | b'K' | b'J' | b'R' | b'c' | b'f'
                    | b'g' | b'h' | b'l' | b'm' | b'n' | b'q' | b't' | b'y' | b'~' => break b,
                    _ => buf.push(b),
                }
            } else {
                // Unknown escape sequence ignored
                return Ok(InputSeq::new(Unidentified));
            }
        };

        fn parse_bytes_as_usize(b: &[u8]) -> Option<usize> {
            str::from_utf8(b).ok().and_then(|s| s.parse().ok())
        }

        let mut args = buf.split(|b| *b == b';');
        match cmd {
            b'R' => {
                // https://vt100.net/docs/vt100-ug/chapter3.html#CPR e.g. \x1b[24;80R
                let mut i = args.filter_map(parse_bytes_as_usize);
                match (i.next(), i.next()) {
                    (Some(r), Some(c)) => Ok(InputSeq::new(Cursor(r, c))),
                    _ => Ok(InputSeq::new(Unidentified)),
                }
            }
            // e.g. <LEFT> => \x1b[C
            // e.g. C-<LEFT> => \x1b[1;5C
            b'A' | b'B' | b'C' | b'D' => {
                let key = match cmd {
                    b'A' => UpKey,
                    b'B' => DownKey,
                    b'C' => RightKey,
                    b'D' => LeftKey,
                    _ => unreachable!(),
                };
                let ctrl = args.next() == Some(b"1") && args.next() == Some(b"5");
                let alt = false;
                Ok(InputSeq { key, ctrl, alt })
            }
            b'~' => {
                // e.g. \x1b[5~
                match args.next() {
                    Some(b"5") => Ok(InputSeq::new(PageUpKey)),
                    Some(b"6") => Ok(InputSeq::new(PageDownKey)),
                    Some(b"1") | Some(b"7") => Ok(InputSeq::new(HomeKey)),
                    Some(b"4") | Some(b"8") => Ok(InputSeq::new(EndKey)),
                    Some(b"3") => Ok(InputSeq::new(DeleteKey)),
                    _ => Ok(InputSeq::new(Unidentified)),
                }
            }
            b'H' | b'F' => {
                // C-HOME => \x1b[1;5H
                let key = match cmd {
                    b'H' => HomeKey,
                    b'F' => EndKey,
                    _ => unreachable!(),
                };
                let ctrl = args.next() == Some(b"1") && args.next() == Some(b"5");
                let alt = false;
                Ok(InputSeq { key, ctrl, alt })
            }
            _ => unreachable!(),
        }
    }

    fn decode_utf8(&mut self, b: u8) -> Result<InputSeq> {
        use KeySeq::*;

        // TODO: Use arrayvec crate
        let mut buf = [0; 4];
        buf[0] = b;
        let mut len = 1;

        loop {
            if let Some(b) = self.read_byte()? {
                buf[len] = b;
                len += 1;
            } else {
                return Ok(InputSeq::new(Unidentified));
            }

            if let Ok(s) = str::from_utf8(&buf) {
                return Ok(InputSeq::new(Utf8Key(s.chars().next().unwrap())));
            }

            if len == 4 {
                return Ok(InputSeq::new(Unidentified));
            }
        }
    }

    fn decode(&mut self, b: u8) -> Result<InputSeq> {
        use KeySeq::*;
        match b {
            // (Maybe) Escape sequence. Ctrl-{ is not available due to this
            0x1b => self.decode_escape_sequence(),
            // Ctrl-SPACE and Ctrl-?. 0x40, 0x3f, 0x60, 0x5f are not available
            0x00 | 0x1f => Ok(InputSeq::ctrl(Key(b | 0b0010_0000))),
            // Ctrl-\ and Ctrl-]. 0x3c, 0x3d, 0x7c, 0x7d are not available
            0x01c | 0x01d => Ok(InputSeq::ctrl(Key(b | 0b0100_0000))),
            // 0x00~0x1f keys are ascii keys with ctrl. Ctrl mod masks key with 0b11111.
            // Here unmask it with 0b1100000. It only works with 0x61~0x7f.
            0x00..=0x1f => Ok(InputSeq::ctrl(Key(b | 0b0110_0000))),
            // Ascii key inputs
            0x20..=0x7f => Ok(InputSeq::new(Key(b))),
            0x80..=0xff => self.decode_utf8(b),
        }
    }

    fn read_seq(&mut self) -> Result<InputSeq> {
        if let Some(b) = self.read_byte()? {
            self.decode(b)
        } else {
            Ok(InputSeq::new(KeySeq::Unidentified))
        }
    }
}

impl Iterator for InputSequences {
    type Item = Result<InputSeq>;

    // Read next byte from stdin with timeout 100ms. If nothing was read, it returns InputSeq::Unidentified.
    // This method never returns None so for loop never ends
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.read_seq())
    }
}
