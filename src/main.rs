// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

use getopts::Options;
use std::env;
use std::io;
use std::process::exit;

use kiro_editor::{Editor, StdinRawMode, HELP, VERSION};

fn print_help(program: &str, opts: Options) {
    let description = format!(
        "{prog}: A tiny UTF-8 terminal text editor

Kiro is a tiny UTF-8 text editor on terminals for Unix-like systems.
Specify a file path to edit as a command argument or run without argument to
start to write a new text.
Help can show up with key mapping Ctrl-?.

Usage:
    {prog} [options] [FILE]

Mappings:
    {maps}",
        prog = program,
        maps = HELP,
    );
    println!("{}", opts.usage(&description));
}

fn edit(file: Option<String>) -> io::Result<()> {
    // TODO: Read input from stdin before start
    let input = StdinRawMode::new()?.input_keys();
    let mut editor = Editor::new(input)?;

    if let Some(f) = file {
        editor.open_file(f)?;
    }

    editor.edit()
}

fn main() {
    let mut argv = env::args();
    let program = argv.next().unwrap();

    let mut opts = Options::new();
    opts.optflag("v", "version", "Print version");
    opts.optflag("h", "help", "Print this help");

    let matches = match opts.parse(argv) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: {}. Please see --help for more details", e);
            exit(1);
        }
    };

    if matches.free.len() > 2 {
        eprintln!("Error: Cannot open multiple files. Please see --help for more details");
        exit(1);
    }

    if matches.opt_present("v") {
        println!("{}", VERSION);
        return;
    }

    if matches.opt_present("h") {
        print_help(&program, opts);
        return;
    }

    if let Err(err) = edit(matches.free.first().cloned()) {
        eprintln!("Error: {}", err);
        exit(1);
    }
}
