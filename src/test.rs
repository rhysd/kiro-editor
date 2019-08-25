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

#[test]
fn test_empty_buffer() {
    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![InputSeq::ctrl(KeySeq::Key(b'q'))]);
    let mut editor = Editor::new(input).unwrap();
    editor.edit().unwrap();

    assert!(editor.screen().rows() > 0);
    assert!(editor.screen().cols() > 0);
    assert_eq!(editor.text_lines().count(), 0);

    let msg = editor.screen().message_text();
    assert_eq!(msg, "Ctrl-? for help");
}

#[test]
fn test_write_to_empty_buffer() {
    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![
        InputSeq::new(KeySeq::Key(b'a')),
        InputSeq::new(KeySeq::Key(b'b')),
        InputSeq::new(KeySeq::Key(b'c')),
        InputSeq::ctrl(KeySeq::Key(b'q')),
        InputSeq::ctrl(KeySeq::Key(b'q')),
    ]);
    let mut editor = Editor::new(input).unwrap();
    editor.edit().unwrap();

    let lines = editor.text_lines().collect::<Vec<_>>();
    assert_eq!(lines, vec!["abc"]);

    let msg = editor.screen().message_text();
    assert!(msg.contains("File has unsaved changes!"), "{}", msg);
}

#[test]
fn test_move_cursor_down() {
    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![
        InputSeq::new(KeySeq::Key(b'a')),
        InputSeq::new(KeySeq::DownKey),
        InputSeq::new(KeySeq::Key(b'b')),
        InputSeq::new(KeySeq::DownKey),
        InputSeq::new(KeySeq::Key(b'c')),
        InputSeq::ctrl(KeySeq::Key(b'q')),
        InputSeq::ctrl(KeySeq::Key(b'q')),
    ]);
    let mut editor = Editor::new(input).unwrap();
    editor.edit().unwrap();

    assert!(editor.screen().rows() > 0);
    assert!(editor.screen().cols() > 0);

    let lines = editor.text_lines().collect::<Vec<_>>();
    assert_eq!(lines, vec!["a", "b", "c"]);
}

#[test]
fn test_open_file() {
    let _stdin = StdinRawMode::new().unwrap();

    let input = DummyInputs(vec![InputSeq::ctrl(KeySeq::Key(b'q'))]);

    let mut editor = Editor::new(input).unwrap();
    let this_file = file!();
    editor.open_file(this_file).unwrap();
    editor.edit().unwrap();

    let f = BufReader::new(File::open(this_file).unwrap());
    for (i, (expected, actual)) in f.lines().zip(editor.text_lines()).enumerate() {
        assert_eq!(expected.unwrap(), actual, "Line: {}", i + 1);
    }

    assert_eq!(editor.lang(), &Language::Rust);
}
