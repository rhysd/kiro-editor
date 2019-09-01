use crate::editor::Editor;
use crate::input::{InputSeq, KeySeq};
use crate::language::Language;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use KeySeq::*;

struct DummyInputs(Vec<InputSeq>);

impl Iterator for DummyInputs {
    type Item = io::Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            None
        } else {
            Some(Ok(self.0.remove(0)))
        }
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

fn key(c: char) -> InputSeq {
    InputSeq::new(Key(c as u8))
}

fn ctrl(c: char) -> InputSeq {
    InputSeq::ctrl(Key(c as u8))
}

fn sp(k: KeySeq) -> InputSeq {
    if let Key(_) = k {
        assert!(false, "{:?}", k);
    }
    InputSeq::new(k)
}

#[test]
fn test_empty_buffer() {
    let input = DummyInputs(vec![InputSeq::ctrl(Key(b'q'))]);
    let mut editor = Editor::new(input, Discard, None).unwrap();
    editor.edit().unwrap();

    assert!(editor.screen().rows() > 0);
    assert!(editor.screen().cols() > 0);
    assert_eq!(editor.lines().count(), 0);

    let msg = editor.screen().message_text();
    assert_eq!(msg, "Ctrl-? for help");
}

#[test]
fn test_write_to_empty_buffer() {
    let input = DummyInputs(vec![key('a'), key('b'), key('c'), ctrl('q'), ctrl('q')]);
    let mut editor = Editor::new(input, Discard, None).unwrap();
    editor.edit().unwrap();

    let lines = editor.lines().collect::<Vec<_>>();
    assert_eq!(lines, vec!["abc"]);

    let msg = editor.screen().message_text();
    assert!(
        msg.contains("At least one file has unsaved changes!"),
        "{}",
        msg
    );
}

#[test]
fn test_move_cursor_down() {
    let input = DummyInputs(vec![
        key('a'),
        sp(DownKey),
        key('b'),
        sp(DownKey),
        key('c'),
        ctrl('q'),
        ctrl('q'),
    ]);
    let mut editor = Editor::new(input, Discard, None).unwrap();
    editor.edit().unwrap();

    assert!(editor.screen().rows() > 0);
    assert!(editor.screen().cols() > 0);

    let lines = editor.lines().collect::<Vec<_>>();
    assert_eq!(lines, vec!["a", "b", "c"]);
}

#[test]
fn test_open_file() {
    let input = DummyInputs(vec![ctrl('q')]);

    let this_file = file!();
    let mut editor = Editor::open(input, Discard, None, &[this_file]).unwrap();
    editor.edit().unwrap();

    let f = BufReader::new(File::open(this_file).unwrap());
    for (i, (expected, actual)) in f.lines().zip(editor.lines()).enumerate() {
        assert_eq!(expected.unwrap(), actual, "Line: {}", i + 1);
    }

    assert_eq!(editor.lang(), Language::Rust);
}

#[test]
fn test_message_bar_squashed() {
    let input = DummyInputs(vec![ctrl('l'), sp(Unidentified), ctrl('q')]);
    let mut buf = Vec::new();
    let mut editor = Editor::new(input, &mut buf, None).unwrap();
    editor.edit().unwrap();

    let msg = editor.screen().message_text();
    assert_eq!(msg, "");
}
