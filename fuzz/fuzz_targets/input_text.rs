#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate kiro_editor;

use kiro_editor::{Editor, InputSeq, KeySeq, Result};
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
        let mut editor =
            Editor::with_lines(s.lines(), ImmediatelyQuit, Discard, Some((80, 24))).unwrap();
        editor.edit().unwrap();
    }
});
