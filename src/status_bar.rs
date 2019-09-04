use crate::language::Language;
use crate::text_buffer::TextBuffer;

#[derive(Default)]
pub struct StatusBar {
    pub modified: bool,
    pub filename: String,
    pub lang: Language,
    pub buf_pos: (usize, usize),
    pub line_pos: (usize, usize),
    pub redraw: bool,
}

macro_rules! setter {
    ($method:ident, $field:ident, $t:ty) => {
        pub fn $method(&mut self, $field: $t) {
            if self.$field != $field {
                self.redraw = true;
                self.$field = $field;
            }
        }
    };
    ($method:ident, $field:ident, $t:ty, $conv:expr) => {
        pub fn $method(&mut self, $field: $t) {
            if self.$field != $field {
                self.redraw = true;
                self.$field = $conv;
            }
        }
    };
}

impl StatusBar {
    setter!(set_buf_pos, buf_pos, (usize, usize));
    setter!(set_modified, modified, bool);
    setter!(set_filename, filename, &str, filename.to_string());
    setter!(set_lang, lang, Language);
    setter!(set_line_pos, line_pos, (usize, usize));

    pub fn left(&self) -> String {
        format!(
            "{:<20?} - {}/{} {}",
            self.filename,
            self.buf_pos.0,
            self.buf_pos.1,
            if self.modified { "(modified) " } else { "" }
        )
    }

    pub fn right(&self) -> String {
        let (lang, (y, len)) = (self.lang, self.line_pos);
        format!("{} {}/{}", lang.name(), y, len)
    }

    pub fn update_from_buf(&mut self, buf: &TextBuffer) {
        self.set_modified(buf.modified());
        self.set_lang(buf.lang());
        self.set_filename(buf.filename());
        self.set_line_pos((buf.cy() + 1, buf.rows().len()));
    }
}
