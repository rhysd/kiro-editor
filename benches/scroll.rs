#![feature(test)]

extern crate test;

use kiro_editor::{Editor, InputSeq, KeySeq, Language, Result, StdinRawMode};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use test::Bencher;

#[derive(Clone)]
struct ScrollInput {
    times: i32,
    count: i32,
    down: bool,
}

impl ScrollInput {
    fn new(times: i32) -> Self {
        assert!(times > 0);
        Self {
            times,
            count: 0,
            down: true,
        }
    }
}

const UP: InputSeq = InputSeq {
    key: KeySeq::Key(b'v'),
    ctrl: false,
    alt: true,
};

const DOWN: InputSeq = InputSeq {
    key: KeySeq::Key(b'v'),
    ctrl: true,
    alt: false,
};

impl Iterator for ScrollInput {
    type Item = Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.down && self.count == 0 {
            return None;
        }

        if self.count == self.times {
            self.down = false;
        }

        self.count += if self.down { 1 } else { -1 };

        let input = if self.down { DOWN } else { UP };
        Some(Ok(input))
    }
}

// TODO: Move to helper
pub struct Discard;

impl Write for Discard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[bench]
fn with_term_scroll_up_down_plain_text(b: &mut Bencher) {
    let f = BufReader::new(File::open(&Path::new("README.md")).unwrap());
    let lines = f.lines().map(|r| r.unwrap()).collect::<Vec<_>>();
    let input = ScrollInput::new(20);
    let _stdin = StdinRawMode::new().unwrap();
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), io::stdout(), Some((80, 24))).unwrap();
        editor.edit().unwrap();
    });
}

#[bench]
fn with_term_scroll_up_down_rust_code(b: &mut Bencher) {
    let f = BufReader::new(File::open(&Path::new("src/editor.rs")).unwrap());
    let lines = f.lines().map(|r| r.unwrap()).collect::<Vec<_>>();
    let input = ScrollInput::new(20);
    let _stdin = StdinRawMode::new().unwrap();
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), io::stdout(), Some((80, 24))).unwrap();
        editor.set_lang(Language::Rust);
        editor.edit().unwrap();
    });
}

#[bench]
fn no_term_scroll_up_down_plain_text(b: &mut Bencher) {
    let f = BufReader::new(File::open(&Path::new("README.md")).unwrap());
    let lines = f.lines().map(|r| r.unwrap()).collect::<Vec<_>>();
    let input = ScrollInput::new(20);
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), Discard, Some((80, 24))).unwrap();
        editor.edit().unwrap();
    });
}

#[bench]
fn no_term_scroll_up_down_rust_code(b: &mut Bencher) {
    let f = BufReader::new(File::open(&Path::new("src/editor.rs")).unwrap());
    let lines = f.lines().map(|r| r.unwrap()).collect::<Vec<_>>();
    let input = ScrollInput::new(20);
    b.iter(|| {
        let mut editor =
            Editor::with_lines(lines.iter(), input.clone(), Discard, Some((80, 24))).unwrap();
        editor.set_lang(Language::Rust);
        editor.edit().unwrap();
    });
}
