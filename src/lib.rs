// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide:           https://vt100.net/docs/vt100-ug/chapter3.html
//   Xterm Control Sequences:    https://www.xfree86.org/current/ctlseqs.html

#![allow(clippy::unused_io_amount)]
#![allow(clippy::match_overlapping_arm)]
#![allow(clippy::useless_let_if_seq)]
#![allow(clippy::cognitive_complexity)]

mod ansi_color;
mod editor;
mod highlight;
mod input;
mod language;
mod row;
mod screen;
mod signal;
mod text_buffer;

#[cfg(test)]
mod ui_test;

pub use editor::Editor;
pub use input::StdinRawMode;
pub use language::Language;
pub use screen::{Screen, HELP, VERSION};
pub use text_buffer::Lines;
