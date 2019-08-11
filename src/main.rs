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
    Ascii(u8),
    // Special(SpecialKey),
    // TODO: Add Utf8(char),
}

impl Key {
    fn ctrl(c: u8) -> Key {
        Key::Ascii(c & 0x1f)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Unidentified => write!(f, "Key(Unidentified)"),
            Key::Ascii(code) => {
                let c = *code as char;
                if c.is_control() {
                    write!(f, "Key(0x{:x})", *code)
                } else {
                    write!(f, "Key('{}', 0x{:x})", c, *code)
                }
            }
        }
    }
}

fn main() -> io::Result<()> {
    let mut stdin = StdinRawMode::new()?;
    let mut one_byte: [u8; 1] = [0];

    loop {
        let size = stdin.read(&mut one_byte)?;
        debug_assert!(size == 0 || size == 1);
        let c = if size > 0 { one_byte[0] } else { b'\0' };
        let k = Key::Ascii(c);

        print!("{:?}\r\n", k);

        if k == Key::ctrl(b'q') {
            break;
        }
    }

    Ok(())
}
