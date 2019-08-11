// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

use std::fmt;
use std::io::{self, Read, Write};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;

struct StdinRawMode {
    stdin: io::Stdin,
    orig: termios::Termios,
}

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
        termios.c_cc[VTIME] = 10;
        // Apply terminal configurations
        tcsetattr(fd, TCSAFLUSH, &mut termios)?;

        Ok(StdinRawMode { stdin, orig })
    }

    fn input_keys(self) -> InputKeys {
        InputKeys { stdin: self }
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

enum SpecialKey {
    Left,
    Right,
    Up,
    Down,
}

#[derive(PartialEq)]
enum Key {
    Unidentified,
    // Special(SpecialKey),
    // TODO: Add Utf8(char),
    Ascii(u8, bool), // Char code and ctrl mod
}

impl Key {
    fn decode_ascii(b: u8) -> Key {
        match b {
            0x20..=0x7f => Key::Ascii(b, false),
            // Note: 0x01~0x1f keys are keys with ctrl mod. Ctrl mod masks key with 0b11111.
            // Here unmask it with 0b1100000. It only works with 0x61~0x7f
            0x01..=0x1f => Key::Ascii(b | 0b1100000, true),
            _ => Key::Unidentified,
        }
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Key::Unidentified => write!(f, "Key(Unidentified)"),
            Key::Ascii(ch, true) => write!(f, "Key(Ctrl+{:?}, 0x{:x})", ch as char, ch),
            Key::Ascii(ch, ..) => write!(f, "Key({:?}, 0x{:x})", ch as char, ch),
        }
    }
}

struct InputKeys {
    stdin: StdinRawMode,
}

impl InputKeys {
    fn read_byte_with_timeout(&mut self) -> io::Result<u8> {
        let mut one_byte: [u8; 1] = [0];
        self.stdin.read(&mut one_byte)?;
        Ok(one_byte[0])
    }
}

impl Iterator for InputKeys {
    type Item = io::Result<Key>;

    // Read next byte from stdin with timeout 100ms. If nothing was read, it returns 0.
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.read_byte_with_timeout().map(Key::decode_ascii))
    }
}

struct Editor {
    // Editor state goes here
}

impl Editor {
    fn new() -> Editor {
        Editor {}
    }

    fn write_rows<W: Write>(&self, mut w: W) -> io::Result<()> {
        // Draw screen
        for _ in 0..24 {
            w.write(b"~\r\n")?;
        }
        Ok(())
    }

    fn refresh_screen(&self) -> io::Result<()> {
        let mut stdout = io::BufWriter::new(io::stdout());
        // \x1b[: Escape sequence header
        // 2: Argument of 'J' command to reset entire screen
        // J: Command to erase screen http://vt100.net/docs/vt100-ug/chapter3.html#ED
        stdout.write(b"\x1b[2J")?;
        // H: Command to move cursor. Here \x1b[H is the same as \x1b[1;1H
        stdout.write(b"\x1b[H")?;

        self.write_rows(&mut stdout)?;

        stdout.write(b"\x1b[H")?;
        stdout.flush()
    }

    fn process_keypress(&mut self, key: Key) -> io::Result<bool> {
        match key {
            Key::Ascii(b'q', true) => Ok(true),
            _ => Ok(false),
        }
    }

    fn run<I>(&mut self, input: I) -> io::Result<()>
    where
        I: Iterator<Item = io::Result<Key>>,
    {
        for key in input {
            self.refresh_screen()?;
            if self.process_keypress(key?)? {
                break;
            }
        }
        self.refresh_screen() // Finally refresh screen on exit
    }
}

fn main() -> io::Result<()> {
    Editor::new().run(StdinRawMode::new()?.input_keys())
}
