use std::io::{self, Read};
use std::os::unix::io::{AsRawFd, RawFd};
use termios::{self, Termios};

struct InputRawMode {
    fd: RawFd,
    orig: Termios,
}

impl InputRawMode {
    fn new(stdin: &io::Stdin) -> io::Result<InputRawMode> {
        let fd = stdin.as_raw_fd();
        let mut termios = Termios::from_fd(fd)?;
        let orig = termios.clone();

        // Set terminal raw mode. Disable echo back and canonical mode.
        termios.c_lflag &= !(termios::ECHO | termios::ICANON);
        termios::tcsetattr(fd, termios::TCSAFLUSH, &mut termios)?;

        Ok(InputRawMode { fd, orig })
    }
}

impl Drop for InputRawMode {
    fn drop(&mut self) {
        // Restore original terminal mode
        termios::tcsetattr(self.fd, termios::TCSAFLUSH, &mut self.orig).unwrap();
    }
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();

    let _raw = InputRawMode::new(&stdin)?;

    for b in stdin.bytes() {
        let c = b? as char;
        println!("c: {:?}", c);
        if c == 'q' {
            break;
        }
    }

    Ok(())
}
