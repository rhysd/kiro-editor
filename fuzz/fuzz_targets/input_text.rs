#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate kiro_editor;

use kiro_editor::{Editor, Error, InputSeq, KeySeq, Language, Result};
use std::io::{self, Write};
use std::str;

// TODO: Fuzz stdin input

struct AllOperations(Vec<InputSeq>);

impl AllOperations {
    fn new() -> Self {
        let mut ops = vec![
            // Insert and move cursor
            InputSeq::new(KeySeq::LeftKey),
            InputSeq::new(KeySeq::RightKey),
            InputSeq::new(KeySeq::UpKey),
            InputSeq::new(KeySeq::DownKey),
            InputSeq::new(KeySeq::Key(b'\r')),
            InputSeq::new(KeySeq::Key(b'a')),
            InputSeq::new(KeySeq::Key(b'b')),
            InputSeq::new(KeySeq::Utf8Key('あ')),
            InputSeq::new(KeySeq::Key(b'\r')),
            InputSeq::new(KeySeq::Key(b'c')),
            // Search
            InputSeq::ctrl(KeySeq::Key(b'g')),
            InputSeq::new(KeySeq::Key(b'a')),
            InputSeq::new(KeySeq::UpKey),
            InputSeq::new(KeySeq::DownKey),
            InputSeq::new(KeySeq::Key(b'b')),
            InputSeq::new(KeySeq::Key(b'h')),
            InputSeq::new(KeySeq::Key(b'\r')),
            InputSeq::ctrl(KeySeq::Key(b'a')),
            InputSeq::ctrl(KeySeq::Key(b'e')),
            InputSeq::ctrl(KeySeq::Key(b'l')),
            // Undo/Redo
            InputSeq::ctrl(KeySeq::Key(b'u')),
            InputSeq::ctrl(KeySeq::Key(b'r')),
            // Delete
            InputSeq::ctrl(KeySeq::Key(b'h')),
            InputSeq::ctrl(KeySeq::Key(b'd')),
            InputSeq::ctrl(KeySeq::Key(b'w')),
            InputSeq::ctrl(KeySeq::Key(b'j')),
            InputSeq::ctrl(KeySeq::Key(b'k')),
            // Scroll
            InputSeq::ctrl(KeySeq::Key(b'v')),
            InputSeq::alt(KeySeq::Key(b'v')),
        ];
        ops.reverse();
        Self(ops)
    }
}

impl Iterator for AllOperations {
    type Item = Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        let seq = self.0.pop().unwrap_or(InputSeq::ctrl(KeySeq::Key(b'q')));
        Some(Ok(seq))
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
        match Editor::with_lines(s.lines(), AllOperations::new(), Discard, Some((80, 24))) {
            Ok(mut editor) => {
                editor.set_lang(Language::Rust);
                editor.edit().unwrap(); // Editor must quit successfully
            }
            Err(Error::ControlCharInText(_)) => { /* Do nothing since it is a possible error */ }
            Err(err) => assert!(false, "{:?}", err), // Unexpected error
        }
    }
});
