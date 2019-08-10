use std::io::{self, Read};
use std::os::unix::io::{AsRawFd, RawFd};

struct InputRawMode {
    fd: RawFd,
    orig: termios::Termios,
}

impl InputRawMode {
    fn new(stdin: &io::Stdin) -> io::Result<InputRawMode> {
        use termios::*;

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
        tcsetattr(fd, TCSAFLUSH, &mut termios)?;

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

        if c.is_control() {
            print!("{}\r\n", c as i32);
        } else {
            print!("{} ({})\r\n", c, c as i32);
        }

        if c == 'q' {
            break;
        }
    }

    Ok(())
}
