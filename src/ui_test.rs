use crate::editor::Editor;
use crate::error::Result;
use crate::input::{InputSeq, KeySeq};
use crate::language::Language;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use KeySeq::*;

struct DummyInputs(Vec<InputSeq>);

impl Iterator for DummyInputs {
    type Item = Result<InputSeq>;

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
    assert_eq!(editor.lines().collect::<Vec<_>>(), vec![""]);

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

macro_rules! test_text_edit {
    ($title:ident, $title_undo:ident, $title_redo:ident {
        before: $before:expr,
        input: [$($input:expr,)+],
        after: $after:expr,
    }) => {
        #[test]
        fn $title() {
            let input = DummyInputs(vec![$($input,)+]);

            let mut editor = Editor::with_lines(
                $before.lines().skip(1), // .skip(1) for first empty line
                input,
                Discard,
                Some((80, 24)),
            ).unwrap();
            editor.edit().unwrap();

            let actual = editor.lines().collect::<Vec<_>>();
            let expected = $after.lines().skip(1).collect::<Vec<_>>(); // .skip(1) for first empty line

            assert_eq!(expected.len(), actual.len(), "expected='{:?}' actual='{:?}'", expected, actual);

            for (idx, (actual_line, expected_line)) in actual.iter().zip(expected.iter()).enumerate() {
                assert_eq!(
                    expected_line,
                    actual_line,
                    "Line {} mismatch! expected='{:?} actual='{:?}'", idx+1, expected, actual,
                );
            }
        }

        #[test]
        fn $title_undo() {
            let mut input = vec![$($input,)+];
            for _ in 0..input.len() {
                input.push(ctrl('u')); // Add undo input to rollback all changes
            }
            let input = DummyInputs(input);

            let mut editor = Editor::with_lines(
                $before.lines().skip(1), // .skip(1) for first empty line
                input,
                Discard,
                Some((80, 24)),
            ).unwrap();
            editor.edit().unwrap();

            // After enough undo operations, buffer must be the same buffer as init
            let actual = editor.lines().collect::<Vec<_>>();
            let expected = $before.lines().skip(1).collect::<Vec<_>>(); // .skip(1) for first empty line

            assert_eq!(expected.len(), actual.len(), "expected='{:?}' actual='{:?}'", expected, actual);

            for (idx, (actual_line, expected_line)) in actual.iter().zip(expected.iter()).enumerate() {
                assert_eq!(
                    expected_line,
                    actual_line,
                    "Line {} mismatch! expected='{:?} actual='{:?}'", idx+1, expected, actual,
                );
            }
        }

        #[test]
        fn $title_redo() {
            let mut input = vec![$($input,)+];
            let len = input.len();
            for _ in 0..len {
                input.push(ctrl('u')); // Add undo input to rollback all changes
            }
            for _ in 0..len {
                input.push(ctrl('r')); // Add redo input to rollback all changes
            }
            let input = DummyInputs(input);

            let mut editor = Editor::with_lines(
                $before.lines().skip(1), // .skip(1) for first empty line
                input,
                Discard,
                Some((80, 24)),
            ).unwrap();
            editor.edit().unwrap();

            // After enough undo and redo operations
            let actual = editor.lines().collect::<Vec<_>>();
            let expected = $after.lines().skip(1).collect::<Vec<_>>(); // .skip(1) for first empty line

            assert_eq!(expected.len(), actual.len(), "expected='{:?}' actual='{:?}'", expected, actual);

            for (idx, (actual_line, expected_line)) in actual.iter().zip(expected.iter()).enumerate() {
                assert_eq!(
                    expected_line,
                    actual_line,
                    "Line {} mismatch! expected='{:?} actual='{:?}'", idx+1, expected, actual,
                );
            }
        }
    }
}

test_text_edit!(
    inesrt_char,
    inesrt_char_undo,
    inesrt_char_redo {
        before: "",
        input: [key('a'), key('b'), key('c'), key('\r'), key('d'), key('e'),],
        after: "
abc
de",
    }
);

test_text_edit!(
    delete_char,
    delete_char_undo,
    delete_char_redo {
        before: "
abc
de

fg",
        input: [
            key('\x08'), // Do nothing
            sp(EndKey),
            key('\x08'), // Delete c
            key('\x08'), // Delete b
            sp(DownKey),
            sp(DownKey),
            key('\x08'), // Remove empty key
            ctrl('v'),   // Move to end of buffer
            key('\x08'), // Do nothing
            sp(UpKey),
            sp(RightKey),
            key('\x08'), // Delete f
            key('\x08'), // Line
            key('\x08'), // Delete e
        ],
        after: "
a
dg",
    }
);

test_text_edit!(
    insert_tab,
    insert_tab_undo,
    insert_tab_redo {
        before: "

ab
cd
ef",
        input: [
            ctrl('i'),
            sp(DownKey),
            sp(HomeKey),
            ctrl('i'),
            sp(DownKey),
            sp(HomeKey),
            sp(RightKey),
            ctrl('i'),
            sp(DownKey),
            sp(EndKey),
            ctrl('i'),
        ],
        after: "
	
	ab
c	d
ef	",
    }
);

test_text_edit!(
    insert_line,
    insert_line_undo,
    insert_line_redo {
        before: "

ab
cd",
        input: [
            key('\r'), // insert line at empty line
            sp(DownKey),
            key('\r'), // insert line at head of line
            sp(RightKey),
            key('\r'), // insert line at middle of line
            sp(EndKey),
            key('\r'), // insert line at end of line
            ctrl('v'), // move to end of buffer
            key('\r'), // insert new line
            key('\r'), // insert new line
        ],
        after: "



a
b

cd


",
    }
);

test_text_edit!(
    delete_right_char,
    delete_right_char_undo,
    delete_right_char_redo {
        before: "
abc

g",
        input: [
            sp(DeleteKey), // Delete a
            sp(RightKey),
            sp(DeleteKey), // Delete c
            sp(DownKey),
            sp(DeleteKey), // Delete empty line
            ctrl('v'),     // Move to end of buffer
            sp(DeleteKey), // Do nothing
        ],
        after: "
b
g",
    }
);

test_text_edit!(
    delete_until_end_of_line,
    delete_until_end_of_line_undo,
    delete_until_end_of_line_redo {
        before: "
ab
cd
ef
g

h",
        input: [
            ctrl('k'), // Delete at head of line
            sp(DownKey),
            sp(RightKey),
            ctrl('k'), // Delete at middle of line
            sp(DownKey),
            sp(RightKey),
            ctrl('k'), // Delete at end of line
            sp(DownKey),
            ctrl('k'), // Delete at empty line
            ctrl('v'), // Move to end of buffer
            ctrl('k'), // Do nothing at end of buffer
            ctrl('k'), // Do nothing at end of buffer
        ],
        after: "

c
efg
h",
    }
);

test_text_edit!(
    delete_until_head_of_line,
    delete_until_head_of_line_undo,
    delete_until_head_of_line_redo {
        before: "
ab
cd
ef
gh

i",
        input: [
            ctrl('j'), // Do nothing at head of buffer
            ctrl('j'), // Do nothing at head of buffer
            sp(RightKey),
            ctrl('j'), // Delete at middle of line
            sp(DownKey),
            sp(EndKey),
            ctrl('j'), // Delete at end of line
            sp(DownKey),
            sp(DownKey),
            ctrl('j'), // Delete at head of line
            sp(DownKey),
            ctrl('j'), // Delete at empty line
            ctrl('v'), // End of buffer
            ctrl('j'), // Do nothing at end of buffer
            ctrl('j'), // Do nothing at end of buffer
        ],
        after: "
b

efgh
i",
    }
);

test_text_edit!(
    delete_word,
    delete_word_undo,
    delete_word_redo {
        before: "
abc def ghi
jkl mno pqr

s",
        input: [
            ctrl('w'), // Do nothing at head of buffer
            ctrl('w'), // Do nothing at head of buffer
            sp(EndKey),
            sp(LeftKey),
            sp(LeftKey),
            sp(LeftKey),
            sp(LeftKey),
            ctrl('w'), // Delete 'def' (end of word)
            sp(LeftKey),
            sp(LeftKey),
            ctrl('w'), // Delete 'ab' (middle of word)
            sp(EndKey),
            sp(LeftKey),
            sp(LeftKey),
            ctrl('w'), // Delete 'g' (middle of word)
            sp(DownKey),
            sp(EndKey),
            sp(LeftKey),
            sp(LeftKey),
            sp(LeftKey),
            ctrl('w'), // Delete 'mno '
            ctrl('w'), // Delete 'jkl '
            sp(DownKey),
            ctrl('w'), // Do nothing at empty line
            ctrl('w'), // Do nothing at empty line
            ctrl('v'), // End of buffer
            ctrl('w'), // Do nothing at end of buffer
            ctrl('w'), // Do nothing at end of buffer
        ],
        after: "
c  hi
pqr

s",
    }
);
