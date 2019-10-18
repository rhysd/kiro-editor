use std::fmt;
use std::io;
use std::time::SystemTimeError;

// Deriving Debug is necessary to use .expect() method
#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    SystemTimeError(SystemTimeError),
    TooSmallWindow(usize, usize),
    UnknownWindowSize,
    InvalidUtf8Input(Vec<u8>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            IoError(err) => write!(f, "{}", err),
            SystemTimeError(err) => write!(f, "{}", err),
            TooSmallWindow(w, h) => write!(
                f,
                "Screen {}x{} is too small. At least 1x3 is necessary in width x height",
                w, h
            ),
            UnknownWindowSize => write!(f, "Could not detect terminal window size"),
            InvalidUtf8Input(seq) => {
                write!(f, "Cannot handle non-UTF8 multi-byte input sequence: ")?;
                for byte in seq.iter() {
                    write!(f, "\\x{:x}", byte)?;
                }
                Ok(())
            }
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<SystemTimeError> for Error {
    fn from(err: SystemTimeError) -> Error {
        Error::SystemTimeError(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
