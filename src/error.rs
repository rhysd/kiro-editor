use std::fmt;
use std::io;

// Deriving Debug is necessary to use .expect() method
#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    TooSmallWindow(usize, usize),
    UnknownWindowSize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            IoError(err) => write!(f, "{}", err),
            TooSmallWindow(w, h) => write!(
                f,
                "Screen {}x{} is too small. At least 1x3 is necessary in width x height",
                w, h
            ),
            UnknownWindowSize => write!(f, "Could not detect terminal window size"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;