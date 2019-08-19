const TAB_STOP: usize = 8;

#[derive(Default)]
pub struct Row {
    buf: String,
    pub render: String,
    pub dirty: bool,
}

impl Row {
    pub fn new<S: Into<String>>(line: S) -> Row {
        let mut row = Row {
            buf: line.into(),
            render: "".to_string(),
            dirty: false,
        };
        row.update_render();
        row
    }

    pub fn buffer(&self) -> &[u8] {
        self.buf.as_str().as_bytes()
    }

    pub fn buffer_str(&self) -> &str {
        self.buf.as_str()
    }

    fn update_render(&mut self) {
        // TODO: Check dirtiness more strict
        self.render = String::with_capacity(self.buf.as_bytes().len());
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
                index += 1;
            }
        }
        self.dirty = true;
    }

    pub fn rx_from_cx(&self, cx: usize) -> usize {
        // TODO: Consider UTF-8 character width
        self.buf.chars().take(cx).fold(0, |rx, ch| {
            if ch == '\t' {
                // Proceed TAB_STOP spaces then subtract spaces by mod TAB_STOP
                rx + TAB_STOP - (rx % TAB_STOP)
            } else {
                rx + 1
            }
        })
    }

    pub fn cx_from_rx(&self, rx: usize) -> usize {
        // TODO: Consider UTF-8 character width
        let mut current_rx = 0;
        for (cx, ch) in self.buf.chars().enumerate() {
            if ch == '\t' {
                current_rx += TAB_STOP - (current_rx % TAB_STOP);
            } else {
                current_rx += 1;
            }
            if current_rx > rx {
                return cx; // Found
            }
        }
        self.buf.as_bytes().len() // Fall back to end of line
    }

    // Note: 'at' is an index of buffer, not render text
    pub fn insert_char(&mut self, at: usize, c: char) {
        if self.buf.as_bytes().len() <= at {
            self.buf.push(c);
        } else {
            self.buf.insert(at, c);
        }
        self.update_render();
    }

    pub fn insert_str<S: AsRef<str>>(&mut self, at: usize, s: S) {
        if self.buf.as_bytes().len() <= at {
            self.buf.push_str(s.as_ref());
        } else {
            self.buf.insert_str(at, s.as_ref());
        }
        self.update_render();
    }

    pub fn delete_char(&mut self, at: usize) {
        if at < self.buf.as_bytes().len() {
            self.buf.remove(at);
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
        if at < self.buf.as_bytes().len() {
            self.buf.truncate(at);
            self.update_render();
        }
    }

    pub fn remove(&mut self, start: usize, end: usize) {
        if start < end {
            self.buf.drain(start..end);
            self.update_render();
        }
    }
}
