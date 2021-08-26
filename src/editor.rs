use crate::error::Result;
use crate::highlight::Highlighting;
use crate::input::{InputSeq, KeySeq};
use crate::language::Language;
use crate::prompt::{self, Prompt, PromptResult};
use crate::screen::Screen;
use crate::status_bar::StatusBar;
use crate::text_buffer::{CursorDir, Lines, TextBuffer};
use std::io::Write;
use std::path::Path;

enum EditStep {
    Continue(InputSeq),
    Quit,
}

impl EditStep {
    fn continues(&self) -> bool {
        match self {
            EditStep::Continue(_) => true,
            EditStep::Quit => false,
        }
    }
}

pub struct Editor<I: Iterator<Item = Result<InputSeq>>, W: Write> {
    input: I,       // Escape sequences stream represented as Iterator
    quitting: bool, // After first Ctrl-Q
    hl: Highlighting,
    screen: Screen<W>,
    bufs: Vec<TextBuffer>,
    buf_idx: usize,
    status_bar: StatusBar,
}

impl<I, W> Editor<I, W>
where
    I: Iterator<Item = Result<InputSeq>>,
    W: Write,
{
    fn with_buf(
        buf: TextBuffer,
        mut input: I,
        output: W,
        window_size: Option<(usize, usize)>,
    ) -> Result<Editor<I, W>> {
        let screen = Screen::new(window_size, &mut input, output)?;
        let status_bar = StatusBar::from_buffer(&buf, (1, 1));
        Ok(Editor {
            input,
            quitting: false,
            hl: Highlighting::default(),
            screen,
            bufs: vec![buf],
            buf_idx: 0,
            status_bar,
        })
    }

    pub fn new(input: I, output: W, window_size: Option<(usize, usize)>) -> Result<Editor<I, W>> {
        Self::with_buf(TextBuffer::empty(), input, output, window_size)
    }

    pub fn with_lines<S: AsRef<str>, L: Iterator<Item = S>>(
        lines: L,
        input: I,
        output: W,
        window_size: Option<(usize, usize)>,
    ) -> Result<Editor<I, W>> {
        Self::with_buf(TextBuffer::with_lines(lines)?, input, output, window_size)
    }

    pub fn open<P: AsRef<Path>>(
        mut input: I,
        output: W,
        window_size: Option<(usize, usize)>,
        paths: &[P],
    ) -> Result<Editor<I, W>> {
        if paths.is_empty() {
            return Self::new(input, output, window_size);
        }
        let screen = Screen::new(window_size, &mut input, output)?;
        let bufs: Vec<_> = paths.iter().map(TextBuffer::open).collect::<Result<_>>()?;
        let hl = Highlighting::new(bufs[0].lang(), bufs[0].rows());
        let status_bar = StatusBar::from_buffer(&bufs[0], (1, bufs.len()));
        Ok(Editor {
            input,
            quitting: false,
            hl,
            screen,
            bufs,
            buf_idx: 0,
            status_bar,
        })
    }

    pub fn buf(&self) -> &TextBuffer {
        &self.bufs[self.buf_idx]
    }

    fn buf_mut(&mut self) -> &mut TextBuffer {
        &mut self.bufs[self.buf_idx]
    }

    fn refresh_status_bar(&mut self) {
        self.status_bar
            .set_buf_pos((self.buf_idx + 1, self.bufs.len()));
        self.status_bar.update_from_buf(&self.bufs[self.buf_idx]);
    }

    fn render_screen(&mut self) -> Result<()> {
        self.refresh_status_bar();
        self.screen
            .render(&self.bufs[self.buf_idx], &mut self.hl, &self.status_bar)?;
        self.status_bar.redraw = false;
        Ok(())
    }

    fn will_reset_scroll(&mut self) {
        self.screen.set_dirty_start(0);
        self.screen.rowoff = 0;
        self.screen.coloff = 0;
    }

    fn will_reset_screen(&mut self) {
        self.screen.set_dirty_start(self.screen.rowoff);
        self.screen.unset_message();
        self.status_bar.redraw = true;
    }

    fn open_buffer(&mut self) -> Result<()> {
        if let PromptResult::Input(input) = self.prompt::<prompt::NoAction>(
            "Open: {} (Empty name for new text buffer, ^G or ESC to cancel)",
            false,
        )? {
            let buf = if input.is_empty() {
                TextBuffer::empty()
            } else {
                TextBuffer::open(input)?
            };
            self.hl = Highlighting::new(buf.lang(), buf.rows());
            self.bufs.push(buf);
            self.buf_idx = self.bufs.len() - 1;
            self.will_reset_scroll();
        }
        Ok(())
    }

    fn switch_buffer(&mut self, idx: usize) {
        let len = self.bufs.len();
        if len == 1 {
            self.screen.set_info_message("No other buffer is opened");
            return;
        }

        debug_assert!(idx < len);
        self.buf_idx = idx;
        let buf = self.buf();

        // XXX: Should we put Highlighting instance in TextBuffer rather than Editor?
        // Then we don't need to recreate Highlighting instance for each buffer switch.
        self.hl = Highlighting::new(buf.lang(), buf.rows());
        self.will_reset_scroll();
    }

    fn next_buffer(&mut self) {
        self.switch_buffer(if self.buf_idx == self.bufs.len() - 1 {
            0
        } else {
            self.buf_idx + 1
        });
    }

    fn previous_buffer(&mut self) {
        self.switch_buffer(if self.buf_idx == 0 {
            self.bufs.len() - 1
        } else {
            self.buf_idx - 1
        });
    }

    fn prompt<A: prompt::Action>(
        &mut self,
        prompt: &str,
        empty_is_cancel: bool,
    ) -> Result<PromptResult> {
        Prompt::new(
            &mut self.screen,
            &mut self.bufs[self.buf_idx],
            &mut self.hl,
            &mut self.status_bar,
            empty_is_cancel,
        )
        .run::<A, _, _>(prompt, &mut self.input)
    }

    fn save(&mut self) -> Result<()> {
        let mut create = false;
        if !self.buf().has_file() {
            let template = "Save as: {} (^G or ESC to cancel)";
            if let PromptResult::Input(input) = self.prompt::<prompt::NoAction>(template, true)? {
                let prev_lang = self.buf().lang();
                self.buf_mut().set_file(input);
                self.hl.lang_changed(self.buf().lang());
                if prev_lang != self.buf().lang() {
                    // Render entire screen since highglight updated
                    self.screen.set_dirty_start(self.screen.rowoff);
                }
                create = true;
            }
        }

        match self.buf_mut().save() {
            Ok(msg) => self.screen.set_info_message(msg),
            Err(msg) => {
                self.screen.set_error_message(msg);
                if create {
                    self.buf_mut().set_unnamed();
                }
            }
        }

        Ok(())
    }

    fn find(&mut self) -> Result<()> {
        let template = "Search: {} (^F or ^N or RIGHT to forward, ^B or ^P or LEFT to back, ^G or ESC to cancel)";
        self.prompt::<prompt::TextSearch>(template, true)?;
        Ok(())
    }

    fn show_help(&mut self) -> Result<()> {
        self.screen.render_help()?;

        // This `while` loop cannot be replaced with `for seq in &mut self.input` since loop body
        // borrows self.input.
        #[allow(clippy::while_let_on_iterator)]
        while let Some(seq) = self.input.next() {
            // Consume any key
            if self.screen.maybe_resize(&mut self.input)? {
                self.screen.render_help()?;
                self.status_bar.redraw = true;
            }
            if seq?.key != KeySeq::Unidentified {
                break;
            }
        }

        // Redraw screen after closing help
        self.screen.set_dirty_start(self.screen.rowoff);
        Ok(())
    }

    fn handle_quit(&mut self, s: InputSeq) -> EditStep {
        let modified = self.bufs.iter().any(|b| b.modified());
        if !modified || self.quitting {
            EditStep::Quit
        } else {
            self.quitting = true;
            self.screen.set_error_message(
                "At least one file has unsaved changes! Press ^Q again to quit or ^S to save",
            );
            EditStep::Continue(s)
        }
    }

    fn handle_not_mapped(&mut self, seq: &InputSeq) {
        self.screen
            .set_error_message(format!("Key '{}' not mapped", seq));
    }

    fn process_keypress(&mut self, s: InputSeq) -> Result<EditStep> {
        use KeySeq::*;

        let rowoff = self.screen.rowoff;
        let rows = self.screen.rows();
        let prev_cursor = self.buf().cursor();

        match &s {
            InputSeq {
                key: Unidentified, ..
            } => return Ok(EditStep::Continue(s)),
            InputSeq { key, alt: true, .. } => match key {
                Key(b'v') => self.buf_mut().move_cursor_page(CursorDir::Up, rowoff, rows),
                Key(b'f') => self.buf_mut().move_cursor_by_word(CursorDir::Right),
                Key(b'b') => self.buf_mut().move_cursor_by_word(CursorDir::Left),
                Key(b'n') => self.buf_mut().move_cursor_paragraph(CursorDir::Down),
                Key(b'p') => self.buf_mut().move_cursor_paragraph(CursorDir::Up),
                Key(b'x') => self.previous_buffer(),
                Key(b'<') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Up),
                Key(b'>') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Down),
                LeftKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Left),
                RightKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Right),
                _ => self.handle_not_mapped(&s),
            },
            InputSeq {
                key, ctrl: true, ..
            } => match key {
                Key(b'p') => self.buf_mut().move_cursor_one(CursorDir::Up),
                Key(b'b') => self.buf_mut().move_cursor_one(CursorDir::Left),
                Key(b'n') => self.buf_mut().move_cursor_one(CursorDir::Down),
                Key(b'f') => self.buf_mut().move_cursor_one(CursorDir::Right),
                Key(b'v') => self
                    .buf_mut()
                    .move_cursor_page(CursorDir::Down, rowoff, rows),
                Key(b'a') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Left),
                Key(b'e') => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Right),
                Key(b'd') => self.buf_mut().delete_right_char(),
                Key(b'g') => self.find()?,
                Key(b'h') => self.buf_mut().delete_char(),
                Key(b'k') => self.buf_mut().delete_until_end_of_line(),
                Key(b'j') => self.buf_mut().delete_until_head_of_line(),
                Key(b'w') => self.buf_mut().delete_word(),
                Key(b'l') => {
                    self.screen.set_dirty_start(self.screen.rowoff); // Clear
                    self.screen.unset_message();
                    self.status_bar.redraw = true;
                }
                Key(b's') => self.save()?,
                Key(b'i') => self.buf_mut().insert_tab(),
                Key(b'm') => self.buf_mut().insert_line(),
                Key(b'o') => self.open_buffer()?,
                Key(b'?') => self.show_help()?,
                Key(b'x') => self.next_buffer(),
                Key(b']') => self
                    .buf_mut()
                    .move_cursor_page(CursorDir::Down, rowoff, rows),
                Key(b'u') => {
                    if !self.buf_mut().undo() {
                        self.screen.set_info_message("No older change");
                    }
                }
                Key(b'r') => {
                    if !self.buf_mut().redo() {
                        self.screen.set_info_message("Buffer is already newest");
                    }
                }
                LeftKey => self.buf_mut().move_cursor_by_word(CursorDir::Left),
                RightKey => self.buf_mut().move_cursor_by_word(CursorDir::Right),
                DownKey => self.buf_mut().move_cursor_paragraph(CursorDir::Down),
                UpKey => self.buf_mut().move_cursor_paragraph(CursorDir::Up),
                Key(b'q') => return Ok(self.handle_quit(s)),
                _ => self.handle_not_mapped(&s),
            },
            InputSeq { key, .. } => match key {
                Key(0x1b) => self.buf_mut().move_cursor_page(CursorDir::Up, rowoff, rows), // Clash with Ctrl-[
                Key(0x08) => self.buf_mut().delete_char(), // Backspace
                Key(0x7f) => self.buf_mut().delete_char(), // Delete key is mapped to \x1b[3~
                Key(b'\r') => self.buf_mut().insert_line(),
                Key(b) if !b.is_ascii_control() => self.buf_mut().insert_char(*b as char),
                Utf8Key(c) => self.buf_mut().insert_char(*c),
                UpKey => self.buf_mut().move_cursor_one(CursorDir::Up),
                LeftKey => self.buf_mut().move_cursor_one(CursorDir::Left),
                DownKey => self.buf_mut().move_cursor_one(CursorDir::Down),
                RightKey => self.buf_mut().move_cursor_one(CursorDir::Right),
                PageUpKey => self.buf_mut().move_cursor_page(CursorDir::Up, rowoff, rows),
                PageDownKey => self
                    .buf_mut()
                    .move_cursor_page(CursorDir::Down, rowoff, rows),
                HomeKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Left),
                EndKey => self.buf_mut().move_cursor_to_buffer_edge(CursorDir::Right),
                DeleteKey => self.buf_mut().delete_right_char(),
                Cursor(_, _) => unreachable!(),
                _ => self.handle_not_mapped(&s),
            },
        }

        if let Some(line) = self.buf_mut().finish_edit() {
            self.hl.needs_update = true;
            self.screen.set_dirty_start(line);
        }
        if self.buf().cursor() != prev_cursor {
            self.screen.cursor_moved = true;
        }
        self.quitting = false;
        Ok(EditStep::Continue(s))
    }

    fn step(&mut self) -> Result<EditStep> {
        let seq = if let Some(seq) = self.input.next() {
            seq?
        } else {
            return Ok(EditStep::Quit);
        };

        if self.screen.maybe_resize(&mut self.input)? {
            self.will_reset_screen();
        }

        let step = self.process_keypress(seq)?;

        if step.continues() {
            self.render_screen()?;
        }

        Ok(step)
    }

    pub fn first_paint(&mut self) -> Result<Edit<'_, I, W>> {
        if self.buf().is_scratch() {
            self.screen.render_welcome(&self.status_bar)?;
            self.status_bar.redraw = false;
        } else {
            self.render_screen()?;
        }
        Ok(Edit { editor: self })
    }

    pub fn edit(&mut self) -> Result<()> {
        // Map Iterator<Result<T>> to Iterator<Result<()>> for .collect()
        self.first_paint()?.try_for_each(|r| r.map(|_| ()))
    }

    pub fn lines(&self) -> Lines<'_> {
        self.buf().lines()
    }

    pub fn screen(&self) -> &'_ Screen<W> {
        &self.screen
    }

    pub fn lang(&self) -> Language {
        self.buf().lang()
    }

    pub fn set_lang(&mut self, lang: Language) {
        let buf = self.buf_mut();
        if buf.lang() == lang {
            return;
        }
        buf.set_lang(lang);
        self.hl = Highlighting::new(lang, buf.rows());
    }
}

pub struct Edit<'a, I, W>
where
    I: Iterator<Item = Result<InputSeq>>,
    W: Write,
{
    editor: &'a mut Editor<I, W>,
}

impl<'a, I, W> Edit<'a, I, W>
where
    I: Iterator<Item = Result<InputSeq>>,
    W: Write,
{
    pub fn editor(&self) -> &'_ Editor<I, W> {
        self.editor
    }
}

impl<'a, I, W> Iterator for Edit<'a, I, W>
where
    I: Iterator<Item = Result<InputSeq>>,
    W: Write,
{
    type Item = Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.editor.step() {
            Ok(EditStep::Continue(seq)) => Some(Ok(seq)),
            Ok(EditStep::Quit) => None,
            Err(err) => Some(Err(err)),
        }
    }
}

#[cfg(test)]
mod tests {
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
            panic!("{:?}", k);
        }
        InputSeq::new(k)
    }

    fn utf8(c: char) -> InputSeq {
        InputSeq::new(Utf8Key(c))
    }

    #[test]
    fn empty_buffer() {
        let input = DummyInputs(vec![InputSeq::ctrl(Key(b'q'))]);
        let mut editor = Editor::new(input, Discard, Some((80, 24))).unwrap();
        editor.edit().unwrap();

        assert!(editor.screen().rows() > 0);
        assert!(editor.screen().cols() > 0);
        assert_eq!(editor.lines().collect::<Vec<_>>(), vec![""]);

        let msg = editor.screen().message_text();
        assert_eq!(msg, "Ctrl-? for help");
    }

    #[test]
    fn write_to_empty_buffer() {
        let input = DummyInputs(vec![key('a'), key('b'), key('c'), ctrl('q'), ctrl('q')]);
        let mut editor = Editor::new(input, Discard, Some((80, 24))).unwrap();
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
    fn edit_step_by_step() {
        let keys = vec![key('a'), key('b'), key('c'), ctrl('q'), ctrl('q')];
        let input = DummyInputs(keys.clone());
        let mut editor = Editor::new(input, Discard, Some((80, 24))).unwrap();
        let mut editing = editor.first_paint().unwrap();

        let mut keys = keys.iter();
        let mut xs = [1, 2, 3, 3].iter();
        while let Some(res) = editing.next() {
            let key = res.unwrap();
            let (x, y) = editing.editor().buf().cursor();
            assert_eq!(y, 0);
            assert_eq!(*xs.next().unwrap(), x);
            assert_eq!(keys.next().unwrap(), &key);
        }

        let mut lines = editor.lines();
        assert_eq!(lines.len(), 1);
        let line = lines.next().unwrap();
        assert_eq!(line, "abc");
    }

    #[test]
    fn move_cursor_down() {
        let input = DummyInputs(vec![
            key('a'),
            sp(DownKey),
            key('b'),
            sp(DownKey),
            key('c'),
            ctrl('q'),
            ctrl('q'),
        ]);
        let mut editor = Editor::new(input, Discard, Some((80, 24))).unwrap();
        editor.edit().unwrap();

        assert!(editor.screen().rows() > 0);
        assert!(editor.screen().cols() > 0);

        let lines = editor.lines().collect::<Vec<_>>();
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn open_file() {
        let input = DummyInputs(vec![ctrl('q')]);

        let this_file = file!();
        let mut editor = Editor::open(input, Discard, Some((80, 24)), &[this_file]).unwrap();
        editor.edit().unwrap();

        let f = BufReader::new(File::open(this_file).unwrap());
        for (i, (expected, actual)) in f.lines().zip(editor.lines()).enumerate() {
            assert_eq!(expected.unwrap(), actual, "Line: {}", i + 1);
        }

        assert_eq!(editor.lang(), Language::Rust);
    }

    #[test]
    fn message_bar_squashed() {
        let input = DummyInputs(vec![ctrl('l'), sp(Unidentified), ctrl('q')]);
        let mut buf = Vec::new();
        let mut editor = Editor::new(input, &mut buf, Some((80, 24))).unwrap();
        editor.edit().unwrap();

        let msg = editor.screen().message_text();
        assert_eq!(msg, "");
    }

    #[test]
    fn undo_modified() {
        let input = DummyInputs(vec![
            key('a'),
            key('b'),
            key('c'),
            ctrl('m'),
            ctrl('u'),
            ctrl('u'),
            ctrl('q'),
            ctrl('q'),
        ]);
        let mut editor = Editor::new(input, Discard, Some((80, 24))).unwrap();
        editor.edit().unwrap();

        let lines = editor.lines().collect::<Vec<_>>();
        assert_eq!(lines, vec![""]);

        assert!(!editor.bufs[0].modified());
    }

    macro_rules! test_text_edit {
    ($title:ident, $title_undo:ident, $title_redo:ident {
        before: $before:expr,
        input: [$($input:expr,)+],
        after: $after:expr,
        cursor: $cursor:expr,
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

            assert_eq!(editor.buf().cursor(), $cursor)
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
        insert_char,
        insert_char_undo,
        insert_char_redo {
            before: "",
            input: [
                key('a'),
                key('b'),
                sp(DownKey),
                key('c'), // Insert first char to new line
                key('\r'),
                key('d'),
                key('e'),
            ],
            after: "
ab
c
de",
            cursor: (2, 2),
        }
    );

    test_text_edit!(
        delete_char,
        delete_char_undo,
        delete_char_redo {
            before: "
abc
def

gh",
            input: [
                key('\x08'), // Do nothing (0x08 means backspace)
                sp(EndKey),
                key('\x08'), // Delete c
                key('\x08'), // Delete b
                sp(DownKey),
                sp(DownKey),
                key('\x08'), // Remove empty line
                key('\x08'), // Remove f
                ctrl('v'),   // Move to end of buffer
                key('\x08'), // Do nothing
                sp(UpKey),
                sp(RightKey),
                key('\x08'), // Delete g
                key('\x08'), // Delete a line
                key('\x08'), // Delete e
            ],
            after: "
a
dh",
            cursor: (1, 1),
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
            cursor: (3, 3),
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
            cursor: (0, 8),
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
                sp(UpKey),
                sp(EndKey),
                sp(DeleteKey), // Do nothing at end of last line
            ],
            after: "
b
g",
            cursor: (1, 1),
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
            cursor: (0, 4),
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
            cursor: (0, 4),
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
            cursor: (0, 4),
        }
    );

    test_text_edit!(
        insert_utf8_char,
        insert_utf8_char_undo,
        insert_utf8_char_redo {
            before: "",
            input: [
                utf8('あ'),
                utf8('い'),
                sp(DownKey),
                utf8('う'), // Insert first char to new line
                key('\r'),
                utf8('え'),
                utf8('お'),
            ],
            after: "
あい
う
えお",
            cursor: (2, 2),
        }
    );

    test_text_edit!(
        delete_utf8_char,
        delete_utf8_char_undo,
        delete_utf8_char_redo {
            before: "
あいう
えおか

きく",
            input: [
                key('\x08'), // Do nothing (0x08 means backspace)
                sp(EndKey),
                key('\x08'), // Delete c
                key('\x08'), // Delete b
                sp(DownKey),
                sp(DownKey),
                key('\x08'), // Remove empty line
                key('\x08'), // Remove f
                ctrl('v'),   // Move to end of buffer
                key('\x08'), // Do nothing
                sp(UpKey),
                sp(RightKey),
                key('\x08'), // Delete g
                key('\x08'), // Delete a line
                key('\x08'), // Delete e
            ],
            after: "
あ
えく",
            cursor: (1, 1),
        }
    );

    test_text_edit!(
        insert_tab_utf8,
        insert_tab_utf8_undo,
        insert_tab_utf8_redo {
            before: "

あい
うえ
おか",
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
	
	あい
う	え
おか	",
            cursor: (3, 3),
        }
    );

    test_text_edit!(
        insert_line_utf8,
        insert_line_utf8_undo,
        insert_line_utf8_redo {
            before: "

あい
うえ",
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



あ
い

うえ


",
            cursor: (0, 8),
        }
    );

    test_text_edit!(
        delete_right_utf8_char,
        delete_right_utf8_char_undo,
        delete_right_utf8_char_redo {
            before: "
あいう

え",
            input: [
                sp(DeleteKey), // Delete a
                sp(RightKey),
                sp(DeleteKey), // Delete c
                sp(DownKey),
                sp(DeleteKey), // Delete empty line
                ctrl('v'),     // Move to end of buffer
                sp(DeleteKey), // Do nothing
                sp(UpKey),
                sp(EndKey),
                sp(DeleteKey), // Do nothing at end of last line
            ],
            after: "
い
え",
            cursor: (1, 1),
        }
    );

    test_text_edit!(
        delete_until_end_of_line_utf8,
        delete_until_end_of_line_utf8_undo,
        delete_until_end_of_line_utf8_redo {
            before: "
あい
うえ
おか
き

く",
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

う
おかき
く",
            cursor: (0, 4),
        }
    );

    test_text_edit!(
        delete_until_head_of_line_utf8,
        delete_until_head_of_line_utf8_undo,
        delete_until_head_of_line_utf8_redo {
            before: "
あい
うえ
おか
きく

け",
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
い

おかきく
け",
            cursor: (0, 4),
        }
    );

    test_text_edit!(
        delete_utf8_word,
        delete_utf8_word_undo,
        delete_utf8_word_redo {
            before: "
あいう えおか きくけ
こさし すせそ たちつ

て",
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
う  くけ
たちつ

て",
            cursor: (0, 4),
        }
    );
}
