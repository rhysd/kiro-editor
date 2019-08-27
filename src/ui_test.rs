use crate::editor::Editor;
use crate::input::{InputSeq, KeySeq, StdinRawMode};
use crate::language::Language;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

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

fn key(c: char) -> InputSeq {
    InputSeq::new(KeySeq::Key(c as u8))
}

fn ctrl(c: char) -> InputSeq {
    InputSeq::ctrl(KeySeq::Key(c as u8))
}

fn sp(k: KeySeq) -> InputSeq {
    if let KeySeq::Key(_) = k {
        assert!(false, "{:?}", k);
    }
    InputSeq::new(k)
}

#[test]
fn test_empty_buffer() {
    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![InputSeq::ctrl(KeySeq::Key(b'q'))]);
    let mut editor = Editor::new(input).unwrap();
    editor.edit().unwrap();

    assert!(editor.screen().rows() > 0);
    assert!(editor.screen().cols() > 0);
    assert_eq!(editor.lines().count(), 0);

    let msg = editor.screen().message_text();
    assert_eq!(msg, "Ctrl-? for help");
}

#[test]
fn test_write_to_empty_buffer() {
    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![key('a'), key('b'), key('c'), ctrl('q'), ctrl('q')]);
    let mut editor = Editor::new(input).unwrap();
    editor.edit().unwrap();

    let lines = editor.lines().collect::<Vec<_>>();
    assert_eq!(lines, vec!["abc"]);

    let msg = editor.screen().message_text();
    assert!(msg.contains("File has unsaved changes!"), "{}", msg);
}

#[test]
fn test_move_cursor_down() {
    use KeySeq::*;

    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![
        key('a'),
        sp(DownKey),
        key('b'),
        sp(DownKey),
        key('c'),
        ctrl('q'),
        ctrl('q'),
    ]);
    let mut editor = Editor::new(input).unwrap();
    editor.edit().unwrap();

    assert!(editor.screen().rows() > 0);
    assert!(editor.screen().cols() > 0);

    let lines = editor.lines().collect::<Vec<_>>();
    assert_eq!(lines, vec!["a", "b", "c"]);
}

#[test]
fn test_open_file() {
    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![ctrl('q')]);

    let mut editor = Editor::new(input).unwrap();
    let this_file = file!();
    editor.open_file(this_file).unwrap();
    editor.edit().unwrap();

    let f = BufReader::new(File::open(this_file).unwrap());
    for (i, (expected, actual)) in f.lines().zip(editor.lines()).enumerate() {
        assert_eq!(expected.unwrap(), actual, "Line: {}", i + 1);
    }

    assert_eq!(editor.lang(), Language::Rust);
}
