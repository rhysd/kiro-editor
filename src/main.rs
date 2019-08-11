use std::fmt;
use std::io::{self, Read};
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

    fn input_keys(&mut self) -> InputKeys {
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

struct InputKeys<'a> {
    stdin: &'a mut StdinRawMode,
}

impl<'a> InputKeys<'a> {
    fn read_next_byte(&mut self) -> io::Result<u8> {
        let mut one_byte: [u8; 1] = [0];
        self.stdin.read(&mut one_byte)?;
        Ok(one_byte[0])
    }
}

impl<'a> Iterator for InputKeys<'a> {
    type Item = io::Result<Key>;

    // Read next byte from stdin with timeout 100ms. If nothing was read, it returns 0.
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.read_next_byte().map(Key::decode_ascii))
    }
}

struct Editor {
    stdin: StdinRawMode,
}

impl Editor {
    fn new() -> io::Result<Editor> {
        StdinRawMode::new().map(|stdin| Editor { stdin })
    }

    fn run(&mut self) -> io::Result<()> {
        for input in self.stdin.input_keys() {
            let key = input?;
            print!("{:?}\r\n", key);
            if key == Key::Ascii(b'q', true) {
                break; // Exit successfully
            }
        }
        Ok(())
    }
}

fn main() -> io::Result<()> {
    Editor::new()?.run()
}
