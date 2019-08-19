// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

use clap::{App, Arg};
use std::io;

use kiro_editor::{Editor, StdinRawMode, HELP, VERSION};

fn main() -> io::Result<()> {
    let matches = App::new("kiro")
        .version(VERSION)
        .author("rhysd <https://github.com/rhysd>")
        .about("A simplistic terminal text editor for Unix-like systems")
        .long_about(HELP)
        .arg(
            Arg::with_name("FILE")
                .help("File to open")
                .takes_value(true),
        )
        .get_matches();

    // TODO: Read input from stdin before start
    let input = StdinRawMode::new()?.input_keys();
    let mut editor = Editor::new(term_size::dimensions_stdout(), input);

    if let Some(arg) = matches.value_of("FILE") {
        editor.open_file(arg)?;
    }

    editor.run()
}
