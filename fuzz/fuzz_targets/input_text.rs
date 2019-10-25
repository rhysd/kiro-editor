#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate kiro_editor;

use kiro_editor::{Editor, Error, InputSeq, KeySeq, Result};
use std::io::{self, Write};
use std::str;

// TODO: Fuzz stdin input

// TODO: Do not quit immediately. Instead, try each edit operations once.
struct ImmediatelyQuit;

impl Iterator for ImmediatelyQuit {
    type Item = Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Ok(InputSeq {
            key: KeySeq::Key(b'q'),
            ctrl: true,
            alt: false,
        }))
    }
}

struct Discard;

impl Write for Discard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = str::from_utf8(data) {
        // Editor may cause an error when the text contains invalid characters
        match Editor::with_lines(s.lines(), ImmediatelyQuit, Discard, Some((80, 24))) {
            Ok(mut editor) => editor.edit().unwrap(), // Editor must quit successfully
            Err(Error::ControlCharInText(_)) => { /* Do nothing since it is a possible error */ }
            Err(err) => assert!(false, "{:?}", err), // Unexpected error
        }
    }
});
