// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

#![allow(clippy::unused_io_amount)]
#![allow(clippy::match_overlapping_arm)]
#![allow(clippy::useless_let_if_seq)]

mod ansi_color;
mod editor;
mod highlight;
mod input;
mod language;
mod row;
mod screen;

pub use editor::Editor;
pub use input::StdinRawMode;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const HELP: &str = r#"
A simplistic terminal text editor for Unix-like systems.

All keymaps as follows.

    Ctrl-Q     : Quit
    Ctrl-S     : Save to file
    Ctrl-P     : Move cursor up
    Ctrl-N     : Move cursor down
    Ctrl-F     : Move cursor right
    Ctrl-B     : Move cursor left
    Ctrl-A     : Move cursor to head of line
    Ctrl-E     : Move cursor to end of line
    Ctrl-V     : Next page
    Alt-V      : Previous page
    Alt-F      : Move cursor to next word
    Alt-B      : Move cursor to previous word
    Alt-<      : Move cursor to top of file
    Alt->      : Move cursor to bottom of file
    Ctrl-H     : Delete character
    Ctrl-D     : Delete next character
    Ctrl-U     : Delete until head of line
    Ctrl-K     : Delete until end of line
    Ctrl-M     : New line
    Ctrl-G     : Search text
    Ctrl-L     : Refresh screen
    Ctrl-?     : Show this help
    UP         : Move cursor up
    DOWN       : Move cursor down
    RIGHT      : Move cursor right
    LEFT       : Move cursor left
    PAGE DOWN  : Next page
    PAGE UP    : Previous page
    HOME       : Move cursor to head of line
    END        : Move cursor to end of line
    DELETE     : Delete next character
    BACKSPACE  : Delete character
    ESC        : Refresh screen
    Ctrl-RIGHT : Move cursor to next word
    Ctrl-LEFT  : Move cursor to previous word
    Alt-RIGHT  : Move cursor to end of line
    Alt-LEFT   : Move cursor to head of line
"#;
