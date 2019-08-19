use unicode_width::UnicodeWidthChar;

const TAB_STOP: usize = 8;

fn byte_index_at<S: AsRef<str>>(at: usize, s: S) -> Option<usize> {
    s.as_ref().char_indices().nth(at).map(|c| c.0)
}

#[derive(Default)]
pub struct Row {
    buf: String,
    // TODO: Remove this field since how to rendering should be calculated at rendering screen.
    // We don't need to cache this because we have 'dirty' field to ensure that this line is
    // rendered once per update
    pub render: String,
    pub dirty: bool,
    pub len: usize,
}

impl Row {
    pub fn new<S: Into<String>>(line: S) -> Row {
        let mut row = Row {
            buf: line.into(),
            render: "".to_string(),
            dirty: false,
            len: 0,
        };
        row.update_render();
        row
    }

    pub fn buffer(&self) -> &str {
        self.buf.as_str()
    }

    pub fn char_at(&self, at: usize) -> char {
        self.char_at_checked(at).unwrap()
    }

    pub fn char_at_checked(&self, at: usize) -> Option<char> {
        // XXX: To avoid O(n) access to specific character in string by index,
        // should we use Vec<char> instead of String for buffer?
        self.buf.chars().nth(at)
    }

    fn update_render(&mut self) {
        self.render = String::with_capacity(self.len);
        self.len = 0;
        let mut index = 0;
        for c in self.buf.chars() {
            if c == '\t' {
                loop {
                    self.render.push(' ');
                    index += 1;
                    if index % TAB_STOP == 0 {
                        break;
                    }
                }
            } else {
                self.render.push(c);
                index += c.width_cjk().unwrap();
            }
            self.len += 1;
        }
        self.dirty = true;
    }

    pub fn rx_from_cx(&self, cx: usize) -> usize {
        self.buf.chars().take(cx).fold(0, |rx, ch| {
            if ch == '\t' {
                // Proceed TAB_STOP spaces then subtract spaces by mod TAB_STOP
                rx + TAB_STOP - (rx % TAB_STOP)
            } else {
                rx + ch.width_cjk().unwrap()
            }
        })
    }

    pub fn cx_from_rx(&self, rx: usize) -> usize {
        let mut current_rx = 0;
        for (cx, ch) in self.buf.chars().enumerate() {
            if ch == '\t' {
                current_rx += TAB_STOP - (current_rx % TAB_STOP);
            } else {
                current_rx += ch.width_cjk().unwrap();
            }
            if current_rx > rx {
                return cx; // Found
            }
        }
        self.len // Fall back to end of line
    }

    // Note: 'at' is an index of buffer, not render text
    pub fn insert_char(&mut self, at: usize, c: char) {
        if self.len <= at {
            self.buf.push(c);
        } else {
            let idx = byte_index_at(at, &self.buf).unwrap_or(0);
            self.buf.insert(idx, c);
        }
        // TODO: More efficient update for self.render
        self.update_render();
    }

    pub fn insert_str<S: AsRef<str>>(&mut self, at: usize, s: S) {
        if self.len <= at {
            self.buf.push_str(s.as_ref());
        } else {
            let idx = byte_index_at(at, &self.buf).unwrap_or(0);
            self.buf.insert_str(idx, s.as_ref());
        }
        self.update_render();
    }

    pub fn delete_char(&mut self, at: usize) {
        if at < self.len {
            let idx = byte_index_at(at, &self.buf).unwrap_or(0);
            self.buf.remove(idx);
            self.update_render();
        }
    }

    pub fn append<S: AsRef<str>>(&mut self, s: S) {
        let s = s.as_ref();
        if s.is_empty() {
            return;
        }
        self.buf.push_str(s);
        self.update_render();
    }

    pub fn truncate(&mut self, at: usize) {
        if at < self.len {
            let idx = byte_index_at(at, &self.buf).unwrap_or(0);
            self.buf.truncate(idx);
            self.update_render();
        }
    }

    pub fn remove(&mut self, start: usize, end: usize) {
        if start < end {
            let start_idx = byte_index_at(start, &self.buf).unwrap_or(0);
            let end_idx = byte_index_at(end, &self.buf).unwrap_or(0);
            self.buf.drain(start_idx..end_idx);
            self.update_render();
        }
    }
}
