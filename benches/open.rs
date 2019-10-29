#![feature(test)]

extern crate test;

use kiro_editor::{Editor, InputSeq, KeySeq, Result, StdinRawMode};
use std::io;
use std::path::Path;
use test::Bencher;

struct NeverInput;

impl Iterator for NeverInput {
    type Item = Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Ok(InputSeq::new(KeySeq::Unidentified)))
    }
}

#[bench]
fn with_term_open_empty_buffer(b: &mut Bencher) {
    b.iter(|| {
        let _stdin = StdinRawMode::new().unwrap();
        let mut editor = Editor::new(NeverInput, io::stdout(), Some((80, 24))).unwrap();
        editor.first_paint().unwrap();
    });
}

#[bench]
fn with_term_open_plain_text(b: &mut Bencher) {
    b.iter(|| {
        let _stdin = StdinRawMode::new().unwrap();
        let files = &[Path::new("README.md")];
        let mut editor = Editor::open(NeverInput, io::stdout(), Some((80, 24)), files).unwrap();
        editor.first_paint().unwrap();
    });
}

#[bench]
fn with_term_open_highlighted_code(b: &mut Bencher) {
    b.iter(|| {
        let _stdin = StdinRawMode::new().unwrap();
        let files = &[Path::new("src/editor.rs")];
        let mut editor = Editor::open(NeverInput, io::stdout(), Some((80, 24)), files).unwrap();
        editor.first_paint().unwrap();
    });
}
