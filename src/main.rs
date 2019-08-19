// Refs:
//   Build Your Own Text Editor: https://viewsourcecode.org/snaptoken/kilo/index.html
//   VT100 User Guide: https://vt100.net/docs/vt100-ug/chapter3.html

use std::cmp;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::iter;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::str;
use std::time::SystemTime;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const TAB_STOP: usize = 8;
const HELP_TEXT: &'static str = "HELP: ^S = save | ^Q = quit | ^G = find | ^? = help";

struct StdinRawMode {
    stdin: io::Stdin,
    orig: termios::Termios,
}

// TODO: Separate editor into frontend and backend. In frontend, it handles actual screen and user input.
// It interacts with backend by responding to request from frontend. Frontend focues on core editor
// logic. This is useful when adding a new frontend (e.g. wasm).

impl StdinRawMode {
    fn new() -> io::Result<StdinRawMode> {
        use termios::*;

        let stdin = io::stdin();
        let fd = stdin.as_raw_fd();
        let mut termios = Termios::from_fd(fd)?;
        let orig = termios.clone();

        // Set terminal raw mode. Disable echo back, canonical mode, signals (SIGINT, SIGTSTP) and Ctrl+V.
        termios.c_lflag &= !(ECHO | ICANON | ISIG | IEXTEN);
        // Disable control flow mode (Ctrl+Q/Ctrl+S) and CR-to-NL translation
        termios.c_iflag &= !(IXON | ICRNL | BRKINT | INPCK | ISTRIP);
        // Disable output processing such as \n to \r\n translation
        termios.c_oflag &= !OPOST;
        // Ensure character size is 8bits
        termios.c_cflag |= CS8;
        // Do not wait for next byte with blocking since reading 0 byte is permitted
        termios.c_cc[VMIN] = 0;
        // Set read timeout to 1/10 second it enables 100ms timeout on read()
        termios.c_cc[VTIME] = 1;
        // Apply terminal configurations
        tcsetattr(fd, TCSAFLUSH, &mut termios)?;

        Ok(StdinRawMode { stdin, orig })
    }

    fn input_keys(self) -> InputSequences {
        InputSequences { stdin: self }
    }
}

impl Drop for StdinRawMode {
    fn drop(&mut self) {
        // Restore original terminal mode
        termios::tcsetattr(self.stdin.as_raw_fd(), termios::TCSAFLUSH, &mut self.orig).unwrap();
    }
}

impl Deref for StdinRawMode {
    type Target = io::Stdin;

    fn deref(&self) -> &Self::Target {
        &self.stdin
    }
}

impl DerefMut for StdinRawMode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stdin
    }
}

#[derive(PartialEq, Debug)]
enum KeySeq {
    Unidentified,
    // TODO: Add Utf8Key(char),
    Key(u8), // Char code and ctrl mod
    LeftKey,
    RightKey,
    UpKey,
    DownKey,
    PageUpKey,
    PageDownKey,
    HomeKey,
    EndKey,
    DeleteKey,
    Cursor(usize, usize), // Pseudo key
}

impl fmt::Display for KeySeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use KeySeq::*;
        match self {
            Unidentified => write!(f, "UNKNOWN"),
            Key(b' ') => write!(f, "SPACE"),
            Key(b) if b.is_ascii_control() => write!(f, "\\x{:x}", b),
            Key(b) => write!(f, "{}", *b as char),
            LeftKey => write!(f, "LEFT"),
            RightKey => write!(f, "RIGHT"),
            UpKey => write!(f, "UP"),
            DownKey => write!(f, "DOWN"),
            PageUpKey => write!(f, "PAGEUP"),
            PageDownKey => write!(f, "PAGEDOWN"),
            HomeKey => write!(f, "HOME"),
            EndKey => write!(f, "END"),
            DeleteKey => write!(f, "DELETE"),
            Cursor(r, c) => write!(f, "CURSOR({},{})", r, c),
        }
    }
}

#[derive(PartialEq, Debug)]
struct InputSeq {
    key: KeySeq,
    ctrl: bool,
    alt: bool,
}

impl InputSeq {
    fn new(key: KeySeq) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
        }
    }

    fn ctrl(key: KeySeq) -> Self {
        Self {
            key,
            ctrl: true,
            alt: false,
        }
    }
}

struct InputSequences {
    stdin: StdinRawMode,
}

impl InputSequences {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        let mut one_byte: [u8; 1] = [0];
        Ok(if self.stdin.read(&mut one_byte)? == 0 {
            None
        } else {
            Some(one_byte[0])
        })
    }

    fn decode_escape_sequence(&mut self) -> io::Result<InputSeq> {
        use KeySeq::*;

        // Try to read expecting '[' as escape sequence header. Note that, if next input does
        // not arrive within next tick, it means that it is not an escape sequence.
        // TODO?: Should we consider sequences not starting with '['?
        match self.read_byte()? {
            Some(b'[') => { /* fall throught */ }
            Some(b) if b.is_ascii_control() => return Ok(InputSeq::new(Key(0x1b))), // Ignore control characters after ESC
            Some(b) => {
                // Alt key is sent as ESC prefix (e.g. Alt-A => \x1b\x61
                // https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Alt-and-Meta-Keys
                let mut seq = self.decode(b)?;
                seq.alt = true;
                return Ok(seq);
            }
            None => return Ok(InputSeq::new(Key(0x1b))),
        };

        // Now confirmed \1xb[ which is a header of escape sequence. Eat it until the end
        // of sequence with blocking
        let mut buf = vec![];
        let cmd = loop {
            if let Some(b) = self.read_byte()? {
                match b {
                    // Control command chars from http://ascii-table.com/ansi-escape-sequences-vt-100.php
                    b'A' | b'B' | b'C' | b'D' | b'F' | b'H' | b'K' | b'J' | b'R' | b'c' | b'f'
                    | b'g' | b'h' | b'l' | b'm' | b'n' | b'q' | b'y' | b'~' => break b,
                    _ => buf.push(b),
                }
            } else {
                // Unknown escape sequence ignored
                return Ok(InputSeq::new(Unidentified));
            }
        };

        let mut args = buf.split(|b| *b == b';');
        match cmd {
            b'R' => {
                // https://vt100.net/docs/vt100-ug/chapter3.html#CPR e.g. \x1b[24;80R
                let mut i =
                    args.map(|b| str::from_utf8(b).ok().and_then(|s| s.parse::<usize>().ok()));
                match (i.next(), i.next()) {
                    (Some(Some(r)), Some(Some(c))) => Ok(InputSeq::new(Cursor(r, c))),
                    _ => Ok(InputSeq::new(Unidentified)),
                }
            }
            // e.g. <LEFT> => \x1b[C
            // e.g. C-<LEFT> => \x1b[1;5C
            b'A' | b'B' | b'C' | b'D' => {
                let key = match cmd {
                    b'A' => UpKey,
                    b'B' => DownKey,
                    b'C' => RightKey,
                    b'D' => LeftKey,
                    _ => unreachable!(),
                };
                let ctrl = args.next() == Some(b"1") && args.next() == Some(b"5");
                let alt = false;
                Ok(InputSeq { key, ctrl, alt })
            }
            b'~' => {
                // e.g. \x1b[5~
                match args.next() {
                    Some(b"5") => Ok(InputSeq::new(PageUpKey)),
                    Some(b"6") => Ok(InputSeq::new(PageDownKey)),
                    Some(b"1") | Some(b"7") => Ok(InputSeq::new(HomeKey)),
                    Some(b"4") | Some(b"8") => Ok(InputSeq::new(EndKey)),
                    Some(b"3") => Ok(InputSeq::new(DeleteKey)),
                    _ => Ok(InputSeq::new(Unidentified)),
                }
            }
            b'H' | b'F' => {
                // C-HOME => \x1b[1;5H
                let key = match cmd {
                    b'H' => HomeKey,
                    b'F' => EndKey,
                    _ => unreachable!(),
                };
                let ctrl = args.next() == Some(b"1") && args.next() == Some(b"5");
                let alt = false;
                Ok(InputSeq { key, ctrl, alt })
            }
            _ => unreachable!(),
        }
    }

    fn decode(&mut self, b: u8) -> io::Result<InputSeq> {
        use KeySeq::*;
        match b {
            // (Maybe) Escape sequence
            0x1b => self.decode_escape_sequence(),
            // Ctrl-?
            0x1f => Ok(InputSeq::ctrl(Key(b | 0b0100000))),
            // 0x00~0x1f keys are ascii keys with ctrl. Ctrl mod masks key with 0b11111.
            // Here unmask it with 0b1100000. It only works with 0x61~0x7f.
            0x00..=0x1f => Ok(InputSeq::ctrl(Key(b | 0b1100000))),
            // Ascii key inputs
            0x20..=0x7f => Ok(InputSeq::new(Key(b))),
            _ => Ok(InputSeq::new(Unidentified)),
            // TODO: 0x80..=0xff => { ... } Handle UTF-8
        }
    }

    fn read_seq(&mut self) -> io::Result<InputSeq> {
        if let Some(b) = self.read_byte()? {
            self.decode(b)
        } else {
            Ok(InputSeq::new(KeySeq::Unidentified))
        }
    }
}

impl Iterator for InputSequences {
    type Item = io::Result<InputSeq>;

    // Read next byte from stdin with timeout 100ms. If nothing was read, it returns InputSeq::Unidentified.
    // This method never returns None so for loop never ends
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.read_seq())
    }
}

// Contain both actual path sequence and display string
struct FilePath {
    path: PathBuf,
    display: String,
}

impl FilePath {
    fn from<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        FilePath {
            path: PathBuf::from(path),
            display: path.to_string_lossy().to_string(),
        }
    }

    fn from_string<S: Into<String>>(s: S) -> Self {
        let display = s.into();
        FilePath {
            path: PathBuf::from(&display),
            display,
        }
    }
}

#[derive(PartialEq)]
enum StatusMessageKind {
    Info,
    Error,
}

struct StatusMessage {
    text: String,
    timestamp: SystemTime,
    kind: StatusMessageKind,
}

impl StatusMessage {
    fn info<S: Into<String>>(message: S) -> StatusMessage {
        StatusMessage::with_kind(message, StatusMessageKind::Info)
    }

    fn error<S: Into<String>>(message: S) -> StatusMessage {
        StatusMessage::with_kind(message, StatusMessageKind::Error)
    }

    fn with_kind<S: Into<String>>(message: S, kind: StatusMessageKind) -> StatusMessage {
        StatusMessage {
            text: message.into(),
            timestamp: SystemTime::now(),
            kind,
        }
    }
}

#[derive(PartialEq)]
enum AnsiColor {
    Reset,
    Red,
    Green,
    Gray,
    Yellow,
    Blue,
    Purple,
    CyanUnderline,
    RedBG,
    Invert,
}

impl AnsiColor {
    fn sequence(&self) -> &'static [u8] {
        // 'm' sets attributes to text printed after: https://vt100.net/docs/vt100-ug/chapter3.html#SGR
        // Color table: https://en.wikipedia.org/wiki/ANSI_escape_code#Colors
        use AnsiColor::*;
        match self {
            Reset => b"\x1b[39;0m",
            Red => b"\x1b[91m",
            Green => b"\x1b[32m",
            Gray => b"\x1b[90m",
            Yellow => b"\x1b[33m",
            Blue => b"\x1b[94m",
            Purple => b"\x1b[95m",
            CyanUnderline => b"\x1b[96;4m",
            RedBG => b"\x1b[41m",
            Invert => b"\x1b[7m",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Highlight {
    Normal,
    Number,
    String,
    Comment,
    Keyword,
    Type,
    Char,
    Statement,
    Match,
}

impl Highlight {
    fn color(&self) -> AnsiColor {
        use AnsiColor::*;
        use Highlight::*;
        match self {
            Normal => Reset,
            Number => Red,
            String => Green,
            Comment => Gray,
            Keyword => Yellow,
            Type => Purple,
            Char => Green,
            Statement => Blue,
            Match => CyanUnderline,
        }
    }
}

#[derive(Default)]
struct Row {
    buf: String,
    render: String,
    dirty: bool,
}

impl Row {
    fn new<S: Into<String>>(line: S) -> Row {
        let mut row = Row {
            buf: line.into(),
            render: "".to_string(),
            dirty: false,
        };
        row.update_render();
        row
    }

    fn buffer(&self) -> &[u8] {
        self.buf.as_str().as_bytes()
    }

    fn buffer_str(&self) -> &str {
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

    fn rx_from_cx(&self, cx: usize) -> usize {
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

    fn cx_from_rx(&self, rx: usize) -> usize {
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
    fn insert_char(&mut self, at: usize, c: char) {
        if self.buf.as_bytes().len() <= at {
            self.buf.push(c);
        } else {
            self.buf.insert(at, c);
        }
        self.update_render();
    }

    fn insert_str<S: AsRef<str>>(&mut self, at: usize, s: S) {
        if self.buf.as_bytes().len() <= at {
            self.buf.push_str(s.as_ref());
        } else {
            self.buf.insert_str(at, s.as_ref());
        }
        self.update_render();
    }

    fn delete_char(&mut self, at: usize) {
        if at < self.buf.as_bytes().len() {
            self.buf.remove(at);
            self.update_render();
        }
    }

    fn append<S: AsRef<str>>(&mut self, s: S) {
        let s = s.as_ref();
        if s.is_empty() {
            return;
        }
        self.buf.push_str(s);
        self.update_render();
    }

    fn truncate(&mut self, at: usize) {
        if at < self.buf.as_bytes().len() {
            self.buf.truncate(at);
            self.update_render();
        }
    }

    fn remove(&mut self, start: usize, end: usize) {
        if start < end {
            self.buf.drain(start..end);
            self.update_render();
        }
    }
}

enum Indent {
    AsIs,
    Fixed(&'static str),
}

#[derive(Clone, Copy, PartialEq)]
enum Language {
    Plain,
    C,
    Rust,
    JavaScript,
    Go,
}

impl Language {
    fn name(&self) -> &'static str {
        use Language::*;
        match self {
            Plain => "plain",
            C => "c",
            Rust => "rust",
            JavaScript => "javascript",
            Go => "go",
        }
    }

    fn file_exts(&self) -> &'static [&'static str] {
        use Language::*;
        match self {
            Plain => &[],
            C => &["c", "h"],
            Rust => &["rs"],
            JavaScript => &["js"],
            Go => &["go"],
        }
    }

    fn indent(&self) -> Indent {
        use Indent::*;
        use Language::*;
        match self {
            Plain => AsIs,
            C => Fixed("    "),
            Rust => Fixed("    "),
            JavaScript => Fixed("  "),
            Go => AsIs,
        }
    }

    fn detect<P: AsRef<Path>>(path: P) -> Language {
        use Language::*;
        if let Some(ext) = path.as_ref().extension().and_then(OsStr::to_str) {
            for lang in &[C, Rust, JavaScript, Go] {
                if lang.file_exts().contains(&ext) {
                    return *lang;
                }
            }
        }
        Plain
    }
}

struct SyntaxHighlight {
    lang: Language,
    string_quotes: &'static [char],
    number: bool,
    character: bool,
    line_comment: Option<&'static str>,
    block_comment: Option<(&'static str, &'static str)>,
    keywords: &'static [&'static str],
    control_statements: &'static [&'static str],
    builtin_types: &'static [&'static str],
}

const PLAIN_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Plain,
    number: false,
    string_quotes: &[],
    character: false,
    line_comment: None,
    block_comment: None,
    keywords: &[],
    control_statements: &[],
    builtin_types: &[],
};

const C_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::C,
    number: true,
    string_quotes: &['"'],
    character: true,
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "auto", "const", "enum", "extern", "inline", "register", "restrict", "sizeof", "static",
        "struct", "typedef", "union", "volatile",
    ],
    control_statements: &[
        "break", "case", "continue", "default", "do", "else", "for", "goto", "if", "return",
        "switch", "while",
    ],
    builtin_types: &[
        "char", "double", "float", "int", "long", "short", "signed", "unsigned", "void",
    ],
};

const RUST_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Rust,
    number: true,
    string_quotes: &['"'],
    character: true,
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "as", "const", "crate", "dyn", "enum", "extern", "false", "fn", "impl", "let", "mod",
        "move", "mut", "pub", "ref", "Self", "self", "static", "struct", "super", "trait", "true",
        "type", "unsafe", "use", "where",
    ],
    control_statements: &[
        "break", "continue", "else", "for", "if", "in", "loop", "match", "return", "while",
    ],
    builtin_types: &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usuze",
        "f32", "f64", "bool", "char",
    ],
};

const JAVASCRIPT_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::JavaScript,
    number: true,
    string_quotes: &['"', '\''],
    character: false,
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "class",
        "const",
        "debugger",
        "delete",
        "export",
        "extends",
        "function",
        "import",
        "in",
        "instanceof",
        "new",
        "super",
        "this",
        "typeof",
        "var",
        "void",
        "with",
        "yield",
    ],
    control_statements: &[
        "break", "case", "catch", "continue", "default", "do", "else", "finally", "for", "if",
        "return", "switch", "throw", "try", "while",
    ],
    builtin_types: &[
        "Object",
        "Function",
        "Boolean",
        "Symbol",
        "Error",
        "Number",
        "BigInt",
        "Math",
        "Date",
        "String",
        "RegExp",
        "Array",
        "Int8Array",
        "Int16Array",
        "Int32Array",
        "BigInt64Array",
        "Uint8Array",
        "Uint16Array",
        "Uint32Array",
        "BigUint64Array",
        "Float32Array",
        "Float64Array",
        "ArrayBuffer",
        "SharedArrayBuffer",
        "Atomics",
        "DataView",
        "JSON",
        "Promise",
        "Generator",
        "GeneratorFunction",
        "AsyncFunction",
        "Refrect",
        "Proxy",
        "Intl",
        "WebAssembly",
    ],
};

const GO_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Go,
    number: true,
    string_quotes: &['"'],
    character: true,
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "chan",
        "const",
        "defer",
        "func",
        "go",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "struct",
        "type",
        "var",
    ],
    control_statements: &[
        "break",
        "case",
        "continue",
        "default",
        "else",
        "fallthrough",
        "for",
        "goto",
        "if",
        "return",
        "select",
        "switch",
    ],
    builtin_types: &[
        "bool",
        "byte",
        "complex128",
        "complex64",
        "error",
        "float32",
        "float64",
        "int",
        "int16",
        "int32",
        "int64",
        "int8",
        "rune",
        "string",
        "uint",
        "uint16",
        "uint32",
        "uint64",
        "uint8",
        "uintptr",
    ],
};

impl SyntaxHighlight {
    fn for_lang(lang: Language) -> &'static SyntaxHighlight {
        use Language::*;
        match lang {
            Plain => &PLAIN_SYNTAX,
            C => &C_SYNTAX,
            Rust => &RUST_SYNTAX,
            JavaScript => &JAVASCRIPT_SYNTAX,
            Go => &GO_SYNTAX,
        }
    }
}

struct Highlighting {
    needs_update: bool,
    previous_bottom_of_screen: usize,
    lines: Vec<Vec<Highlight>>,
    matched: Option<(usize, usize, Vec<Highlight>)>, // (x, y, saved)
    syntax: &'static SyntaxHighlight,
}

impl Default for Highlighting {
    fn default() -> Self {
        Highlighting {
            needs_update: false,
            previous_bottom_of_screen: 0,
            lines: vec![],
            matched: None,
            syntax: &PLAIN_SYNTAX,
        }
    }
}

impl Highlighting {
    fn new<'a, R: Iterator<Item = &'a Row>>(lang: Language, iter: R) -> Highlighting {
        Highlighting {
            needs_update: true,
            previous_bottom_of_screen: 0,
            lines: iter
                .map(|r| {
                    iter::repeat(Highlight::Normal)
                        .take(r.render.as_bytes().len())
                        .collect()
                })
                .collect(),
            matched: None,
            syntax: SyntaxHighlight::for_lang(lang),
        }
    }

    fn lang_changed(&mut self, new_lang: Language) {
        if self.syntax.lang == new_lang {
            return;
        }
        self.syntax = SyntaxHighlight::for_lang(new_lang);
        self.needs_update = true;
    }

    fn update(&mut self, rows: &Vec<Row>, bottom_of_screen: usize) {
        if !self.needs_update && bottom_of_screen <= self.previous_bottom_of_screen {
            return;
        }

        self.lines.resize_with(rows.len(), Default::default);

        fn is_sep(b: u8) -> bool {
            b.is_ascii_whitespace() || (b.is_ascii_punctuation() && b != b'_') || b == b'\0'
        }

        fn starts_with_word(input: &[u8], word: &[u8]) -> bool {
            if !input.starts_with(word) {
                false
            } else if input.len() == word.len() {
                true
            } else if input.len() > word.len() && is_sep(input[word.len()]) {
                true
            } else {
                false
            }
        }

        let mut prev_quote = None;
        let mut in_block_comment = false;
        for (y, ref row) in rows.iter().enumerate().take(bottom_of_screen) {
            self.lines[y].resize(row.render.as_bytes().len(), Highlight::Normal);

            let mut prev_hl = Highlight::Normal;
            let mut prev_char = b'\0';
            let mut iter = row.render.as_bytes().iter().cloned().enumerate();

            while let Some((x, b)) = iter.next() {
                let mut hl = Highlight::Normal;

                if self.lines[y][x] == Highlight::Match {
                    hl = Highlight::Match;
                }

                if let Some((comment_start, comment_end)) = self.syntax.block_comment {
                    if hl == Highlight::Normal && prev_quote.is_none() {
                        let comment_delim = if in_block_comment
                            && row.render[x..].starts_with(comment_end)
                        {
                            in_block_comment = false;
                            Some(comment_end)
                        } else if !in_block_comment && row.render[x..].starts_with(comment_start) {
                            in_block_comment = true;
                            Some(comment_start)
                        } else {
                            None
                        };

                        // Eat delimiter of block comment at once
                        if let Some(comment_delim) = comment_delim {
                            // Consume whole '/*' here. Otherwise such as '/*/' is wrongly accepted
                            let len = comment_delim.as_bytes().len();
                            self.lines[y]
                                .splice(x..x + len, iter::repeat(Highlight::Comment).take(len));

                            prev_hl = Highlight::Comment;
                            prev_char = comment_delim.as_bytes()[len - 1];
                            iter.nth(len - 2);
                            continue;
                        }

                        if in_block_comment {
                            hl = Highlight::Comment;
                        }
                    }
                }

                if let Some(comment_leader) = self.syntax.line_comment {
                    if prev_quote.is_none() && row.render[x..].starts_with(comment_leader) {
                        let len = self.lines[y].len();
                        self.lines[y].splice(x.., iter::repeat(Highlight::Comment).take(len - x));
                        break;
                    }
                }

                if hl == Highlight::Normal && self.syntax.character {
                    let mut i = row.render.as_bytes()[x..].iter();
                    let len = match (i.next(), i.next(), i.next(), i.next()) {
                        (Some(b'\''), Some(b'\\'), _, Some(b'\'')) => Some(4),
                        (Some(b'\''), _, Some(b'\''), _) => Some(3),
                        _ => None,
                    };

                    if let Some(len) = len {
                        self.lines[y].splice(x..x + len, iter::repeat(Highlight::Char).take(len));
                        prev_hl = Highlight::Char;
                        prev_char = b'\'';
                        iter.nth(len - 2);
                        continue;
                    }
                }

                if hl == Highlight::Normal && self.syntax.string_quotes.len() > 0 {
                    if let Some(q) = prev_quote {
                        // In string literal. XXX: "\\" is not highlighted correctly
                        if prev_char != b'\\' && q == b {
                            prev_quote = None;
                        }
                        hl = Highlight::String;
                    } else if self.syntax.string_quotes.contains(&(b as char)) {
                        prev_quote = Some(b);
                        hl = Highlight::String;
                    }
                }

                let is_bound = is_sep(prev_char) ^ is_sep(b);

                // Highlight identifiers
                if hl == Highlight::Normal && is_bound {
                    let line = row.render[x..].as_bytes();
                    if let Some((keyword, highlight)) = self
                        .syntax
                        .keywords
                        .iter()
                        .zip(iter::repeat(Highlight::Keyword))
                        .chain(
                            self.syntax
                                .control_statements
                                .iter()
                                .zip(iter::repeat(Highlight::Statement)),
                        )
                        .chain(
                            self.syntax
                                .builtin_types
                                .iter()
                                .zip(iter::repeat(Highlight::Type)),
                        )
                        .find(|(k, _)| starts_with_word(line, k.as_bytes()))
                    {
                        let len = keyword.as_bytes().len();
                        self.lines[y].splice(x..x + len, iter::repeat(highlight).take(len));

                        prev_hl = highlight;
                        prev_char = line[len - 1];
                        // Consume keyword from input. `- 2` because first character was already
                        // consumed by the while statement
                        iter.nth(len - 2);

                        continue;
                    }
                }

                if hl == Highlight::Normal && self.syntax.number {
                    if b.is_ascii_digit() && (prev_hl == Highlight::Number || is_bound) {
                        hl = Highlight::Number;
                    } else if b == b'.' && prev_hl == Highlight::Number {
                        hl = Highlight::Number;
                    }
                }

                self.lines[y][x] = hl;
                prev_hl = hl;
                prev_char = b;
            }
        }

        self.needs_update = false;
        self.previous_bottom_of_screen = bottom_of_screen;
    }

    fn set_match(&mut self, y: usize, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.clear_previous_match();
        self.matched = Some((
            start,
            y,
            self.lines[y][start..end].iter().cloned().collect(),
        ));
        self.lines[y].splice(start..end, iter::repeat(Highlight::Match).take(end - start));
    }

    fn clear_previous_match(&mut self) -> Option<usize> {
        if let Some((x, y, saved)) = mem::replace(&mut self.matched, None) {
            // Restore previously replaced highlights
            self.lines[y].splice(x..(x + saved.len()), saved.into_iter());
            Some(y)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy)]
enum FindDir {
    Back,
    Forward,
}
struct FindState {
    last_match: Option<usize>,
    dir: FindDir,
}

impl FindState {
    fn new() -> FindState {
        FindState {
            last_match: None,
            dir: FindDir::Forward,
        }
    }
}

#[derive(Clone, Copy)]
enum CursorDir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(PartialEq)]
enum AfterKeyPress {
    Quit,
    Nothing,
}

struct Editor<I: Iterator<Item = io::Result<InputSeq>>> {
    // VT100 sequence stream represented as Iterator
    input: I,
    // File editor is opening
    file: Option<FilePath>,
    // (x, y) coordinate in internal text buffer of rows
    cx: usize,
    cy: usize,
    // (x, y) coordinate in `render` text of rows
    rx: usize,
    // Screen size
    screen_rows: usize,
    screen_cols: usize,
    // Lines of text buffer
    row: Vec<Row>,
    // Scroll position (row/col offset)
    rowoff: usize,
    coloff: usize,
    // Message in status line
    message: StatusMessage,
    // Flag set to true when buffer is modified after loading a file
    modified: bool,
    // After first Ctrl-Q
    quitting: bool,
    // Text search state
    finding: FindState,
    // Language which current buffer belongs to
    lang: Language,
    // Syntax highlighting
    hl: Highlighting,
}

impl<I: Iterator<Item = io::Result<InputSeq>>> Editor<I> {
    fn new(window_size: Option<(usize, usize)>, input: I) -> Editor<I> {
        let (w, h) = window_size.unwrap_or((0, 0));
        Editor {
            input,
            file: None,
            cx: 0,
            cy: 0,
            rx: 0,
            screen_cols: w,
            // Screen height is 1 line less than window height due to status bar
            screen_rows: h.saturating_sub(2),
            row: Vec::with_capacity(h),
            rowoff: 0,
            coloff: 0,
            message: StatusMessage::info(HELP_TEXT),
            modified: false,
            quitting: false,
            finding: FindState::new(),
            lang: Language::Plain,
            hl: Highlighting::default(),
        }
    }

    fn trim_line<'a, S: AsRef<str>>(&self, line: &'a S) -> &'a str {
        let mut line = line.as_ref();
        if line.len() <= self.coloff {
            return "";
        }
        if self.coloff > 0 {
            line = &line[self.coloff..];
        }
        if line.len() > self.screen_cols {
            line = &line[..self.screen_cols]
        }
        line
    }

    fn draw_status_bar<W: Write>(&self, mut buf: W) -> io::Result<()> {
        write!(buf, "\x1b[{}H", self.screen_rows + 1)?;

        buf.write(AnsiColor::Invert.sequence())?;

        let file = if let Some(ref f) = self.file {
            f.display.as_str()
        } else {
            "[No Name]"
        };

        let modified = if self.modified { "(modified) " } else { "" };
        let left = format!("{:<20?} - {} lines {}", file, self.row.len(), modified);
        let left = &left[..cmp::min(left.len(), self.screen_cols)];
        buf.write(left.as_bytes())?; // Left of status bar

        let rest_len = self.screen_cols - left.len();
        if rest_len == 0 {
            return Ok(());
        }

        let right = format!(
            "{} {}/{}",
            self.hl.syntax.lang.name(),
            self.cy,
            self.row.len(),
        );
        if right.len() > rest_len {
            for _ in 0..rest_len {
                buf.write(b" ")?;
            }
            return Ok(());
        }

        for _ in 0..rest_len - right.len() {
            buf.write(b" ")?; // Add spaces at center of status bar
        }
        buf.write(right.as_bytes())?;

        // Defualt argument of 'm' command is 0 so it resets attributes
        buf.write(AnsiColor::Reset.sequence())?;
        Ok(())
    }

    fn draw_message_bar<W: Write>(&self, mut buf: W) -> io::Result<()> {
        write!(buf, "\x1b[{}H", self.screen_rows + 2)?;
        if let Ok(d) = SystemTime::now().duration_since(self.message.timestamp) {
            if d.as_secs() < 5 {
                let msg = &self.message.text[..cmp::min(self.message.text.len(), self.screen_cols)];
                if self.message.kind == StatusMessageKind::Error {
                    buf.write(AnsiColor::RedBG.sequence())?;
                    buf.write(msg.as_bytes())?;
                    buf.write(AnsiColor::Reset.sequence())?;
                } else {
                    buf.write(msg.as_bytes())?;
                }
            }
        }
        buf.write(b"\x1b[K")?;
        Ok(())
    }

    fn draw_welcome_message<W: Write>(&self, mut buf: W) -> io::Result<()> {
        let msg_buf = format!("Kilo editor -- version {}", VERSION);
        let welcome = self.trim_line(&msg_buf);
        let padding = (self.screen_cols - welcome.len()) / 2;
        if padding > 0 {
            buf.write(b"~")?;
            for _ in 0..padding - 1 {
                buf.write(b" ")?;
            }
        }
        buf.write(welcome.as_bytes())?;
        Ok(())
    }

    fn draw_rows<W: Write>(&self, mut buf: W) -> io::Result<()> {
        let mut prev_color = AnsiColor::Reset;
        let row_len = self.row.len();

        for y in 0..self.screen_rows {
            let file_row = y + self.rowoff;

            if file_row < row_len && !self.row[file_row].dirty {
                continue;
            }

            // Move cursor to target line
            write!(buf, "\x1b[{}H", y + 1)?;

            if file_row >= row_len {
                if self.row.is_empty() && y == self.screen_rows / 3 {
                    self.draw_welcome_message(&mut buf)?;
                } else {
                    if prev_color != AnsiColor::Reset {
                        buf.write(AnsiColor::Reset.sequence())?;
                        prev_color = AnsiColor::Reset;
                    }
                    buf.write(b"~")?;
                }
            } else {
                // TODO: Support UTF-8
                let row = &self.row[file_row];

                for (b, hl) in row
                    .render
                    .as_bytes()
                    .iter()
                    .cloned()
                    .zip(self.hl.lines[file_row].iter())
                    .skip(self.coloff)
                    .take(self.screen_cols)
                {
                    let color = hl.color();
                    if color != prev_color {
                        buf.write(color.sequence())?;
                        prev_color = color;
                    }
                    buf.write(&[b])?;
                }
            }

            // Erases the part of the line to the right of the cursor. http://vt100.net/docs/vt100-ug/chapter3.html#EL
            buf.write(b"\x1b[K")?;
        }

        if prev_color != AnsiColor::Reset {
            buf.write(AnsiColor::Reset.sequence())?; // Ensure to reset color at end of screen
        }

        Ok(())
    }

    fn redraw_screen(&self) -> io::Result<()> {
        let mut buf = Vec::with_capacity((self.screen_rows + 1) * self.screen_cols);

        // \x1b[: Escape sequence header
        // Hide cursor while updating screen. 'l' is command to set mode http://vt100.net/docs/vt100-ug/chapter3.html#SM
        buf.write(b"\x1b[?25l")?;
        // H: Command to move cursor. Here \x1b[H is the same as \x1b[1;1H
        buf.write(b"\x1b[H")?;

        self.draw_rows(&mut buf)?;
        self.draw_status_bar(&mut buf)?;
        self.draw_message_bar(&mut buf)?;

        // Move cursor
        let cursor_row = self.cy - self.rowoff + 1;
        let cursor_col = self.rx - self.coloff + 1;
        write!(buf, "\x1b[{};{}H", cursor_row, cursor_col)?;

        // Reveal cursor again. 'h' is command to reset mode https://vt100.net/docs/vt100-ug/chapter3.html#RM
        buf.write(b"\x1b[?25h")?;

        let mut stdout = io::stdout();
        stdout.write(&buf)?;
        stdout.flush()
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        self.do_scroll();
        self.hl.update(&self.row, self.rowoff + self.screen_rows);
        self.redraw_screen()?;

        for row in self.row.iter_mut().skip(self.rowoff).take(self.screen_rows) {
            row.dirty = false; // Rendered the row. It's no longer dirty
        }

        Ok(())
    }

    fn clear_screen(&self) -> io::Result<()> {
        let mut stdout = io::stdout();
        // 2: Argument of 'J' command to reset entire screen
        // J: Command to erase screen http://vt100.net/docs/vt100-ug/chapter3.html#ED
        stdout.write(b"\x1b[2J")?;
        // Set cursor position to left-top corner
        stdout.write(b"\x1b[H")?;
        stdout.flush()
    }

    fn open_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        if path.exists() {
            let file = fs::File::open(path)?;
            self.row = io::BufReader::new(file)
                .lines()
                .map(|r| Ok(Row::new(r?)))
                .collect::<io::Result<_>>()?;
            self.modified = false;
        } else {
            // When the path does not exist, consider it as a new file
            self.row = vec![];
            self.modified = true;
        }
        self.lang = Language::detect(path);
        self.hl = Highlighting::new(self.lang, self.row.iter());
        self.file = Some(FilePath::from(path));
        Ok(())
    }

    fn save(&mut self) -> io::Result<()> {
        let mut create = false;
        if self.file.is_none() {
            if let Some(input) =
                self.prompt("Save as: {} (^G or ESC to cancel)", |_, _, _, _| Ok(()))?
            {
                let file = FilePath::from_string(input);
                self.lang = Language::detect(&file.path);
                self.hl.lang_changed(self.lang);
                self.file = Some(file);
                create = true;
            }
        }

        let ref file = if let Some(ref file) = self.file {
            file
        } else {
            return Ok(()); // Canceled
        };

        let f = match fs::File::create(&file.path) {
            Ok(f) => f,
            Err(e) => {
                self.message = StatusMessage::error(format!("Could not save: {}", e));
                if create {
                    self.file = None; // Could not make file. Back to unnamed buffer
                }
                return Ok(()); // This is not a fatal error
            }
        };
        let mut f = io::BufWriter::new(f);
        let mut bytes = 0;
        for line in self.row.iter() {
            let b = line.buffer();
            f.write(b)?;
            f.write(b"\n")?;
            bytes += b.len() + 1;
        }
        f.flush()?;

        self.message = StatusMessage::info(format!("{} bytes written to {}", bytes, &file.display));
        self.modified = false;
        Ok(())
    }

    fn on_incremental_find(&mut self, query: &str, seq: InputSeq, end: bool) -> io::Result<()> {
        use KeySeq::*;

        if self.finding.last_match.is_some() {
            if let Some(y) = self.hl.clear_previous_match() {
                self.row[y].dirty = true;
            }
        }

        if end {
            return Ok(());
        }

        match (seq.key, seq.ctrl, seq.alt) {
            (RightKey, ..) | (DownKey, ..) | (Key(b'f'), true, ..) | (Key(b'n'), true, ..) => {
                self.finding.dir = FindDir::Forward
            }
            (LeftKey, ..) | (UpKey, ..) | (Key(b'b'), true, ..) | (Key(b'p'), true, ..) => {
                self.finding.dir = FindDir::Back
            }
            _ => self.finding = FindState::new(),
        }

        fn next_line(y: usize, dir: FindDir, len: usize) -> usize {
            // Wrapping text search at top/bottom of text buffer
            match dir {
                FindDir::Forward if y == len - 1 => 0,
                FindDir::Forward => y + 1,
                FindDir::Back if y == 0 => len - 1,
                FindDir::Back => y - 1,
            }
        }

        let row_len = self.row.len();
        let dir = self.finding.dir;
        let mut y = self
            .finding
            .last_match
            .map(|y| next_line(y, dir, row_len)) // Start from next line on moving to next match
            .unwrap_or(self.cy);

        for _ in 0..row_len {
            if let Some(rx) = self.row[y].render.find(query) {
                // XXX: This searches render text, not actual buffer. So it may not work properly on
                // character which is rendered differently (e.g. tab character)
                self.cy = y;
                self.cx = self.row[y].cx_from_rx(rx);
                // Cause do_scroll() to scroll upwards to the matching line at next screen redraw
                self.rowoff = row_len;
                self.finding.last_match = Some(y);
                // This refresh is necessary because highlight must be updated before saving highlights
                // of matched region
                self.refresh_screen()?;
                // Set match highlight on the found line
                self.hl.set_match(y, rx, rx + query.as_bytes().len());
                self.row[y].dirty = true;
                break;
            }
            y = next_line(y, dir, row_len);
        }

        Ok(())
    }

    fn find(&mut self) -> io::Result<()> {
        let (cx, cy, coloff, rowoff) = (self.cx, self.cy, self.coloff, self.rowoff);
        let s = "Search: {} (^F or RIGHT to forward, ^B or LEFT to back, ^G or ESC to cancel)";
        if self.prompt(s, Self::on_incremental_find)?.is_none() {
            // Canceled. Restore cursor position
            self.cx = cx;
            self.cy = cy;
            self.coloff = coloff;
            self.rowoff = rowoff;
            self.set_dirty_rows(self.rowoff); // Redraw all lines
        } else {
            self.message = if self.finding.last_match.is_some() {
                StatusMessage::info("Found")
            } else {
                StatusMessage::error("Not Found")
            };
        }

        self.finding = FindState::new(); // Clear text search state for next time
        Ok(())
    }

    fn set_dirty_rows(&mut self, start: usize) {
        for row in self.row.iter_mut().skip(start).take(self.screen_rows) {
            row.dirty = true;
        }
    }

    fn do_scroll(&mut self) {
        let prev_rowoff = self.rowoff;
        let prev_coloff = self.coloff;

        // Calculate X coordinate to render considering tab stop
        if self.cy < self.row.len() {
            self.rx = self.row[self.cy].rx_from_cx(self.cx);
        } else {
            self.rx = 0;
        }

        // Adjust scroll position when cursor is outside screen
        if self.cy < self.rowoff {
            // Scroll up when cursor is above the top of window
            self.rowoff = self.cy;
        }
        if self.cy >= self.rowoff + self.screen_rows {
            // Scroll down when cursor is below the bottom of screen
            self.rowoff = self.cy - self.screen_rows + 1;
        }
        if self.rx < self.coloff {
            self.coloff = self.rx;
        }
        if self.rx >= self.coloff + self.screen_cols {
            self.coloff = self.rx - self.screen_cols + 1;
        }

        if prev_rowoff != self.rowoff || prev_coloff != self.coloff {
            // If scroll happens, all rows on screen must be updated
            self.set_dirty_rows(self.rowoff);
        }
    }

    fn insert_char(&mut self, ch: char) {
        if self.cy == self.row.len() {
            self.row.push(Row::default());
        }
        self.row[self.cy].insert_char(self.cx, ch);
        self.cx += 1;
        self.modified = true;
        self.hl.needs_update = true;
    }

    fn insert_tab(&mut self) {
        match self.lang.indent() {
            Indent::AsIs => self.insert_char('\t'),
            Indent::Fixed(indent) => self.insert_str(indent),
        }
    }

    fn insert_str<S: AsRef<str>>(&mut self, s: S) {
        if self.cy == self.row.len() {
            self.row.push(Row::default());
        }
        let s = s.as_ref();
        self.row[self.cy].insert_str(self.cx, s);
        self.cx += s.as_bytes().len();
        self.modified = true;
        self.hl.needs_update = true;
    }

    fn squash_to_previous_line(&mut self) {
        // At top of line, backspace concats current line to previous line
        self.cx = self.row[self.cy - 1].buffer().len(); // Move cursor column to end of previous line
        let row = self.row.remove(self.cy);
        self.cy -= 1; // Move cursor to previous line
        self.row[self.cy].append(row.buffer_str());
        self.modified = true;
        self.hl.needs_update = true;

        self.set_dirty_rows(self.cy + 1);
    }

    fn delete_char(&mut self) {
        if self.cy == self.row.len() || self.cx == 0 && self.cy == 0 {
            return;
        }
        if self.cx > 0 {
            self.row[self.cy].delete_char(self.cx - 1);
            self.cx -= 1;
            self.modified = true;
            self.hl.needs_update = true;
        } else {
            self.squash_to_previous_line();
        }
    }

    fn delete_until_end_of_line(&mut self) {
        if self.cy == self.row.len() {
            return;
        }
        if self.cx == self.row[self.cy].buffer().len() {
            // Do nothing when cursor is at end of line of end of text buffer
            if self.cy == self.row.len() - 1 {
                return;
            }
            // At end of line, concat with next line
            let deleted = self.row.remove(self.cy + 1);
            self.row[self.cy].append(deleted.buffer_str());
            self.set_dirty_rows(self.cy + 1);
        } else {
            self.row[self.cy].truncate(self.cx);
        }
        self.modified = true;
        self.hl.needs_update = true;
    }

    fn delete_until_head_of_line(&mut self) {
        if self.cx == 0 && self.cy == 0 || self.cy == self.row.len() {
            return;
        }
        if self.cx == 0 {
            self.squash_to_previous_line();
        } else {
            self.row[self.cy].remove(0, self.cx);
            self.cx = 0;
            self.modified = true;
            self.hl.needs_update = true;
        }
    }

    fn delete_word(&mut self) {
        if self.cx == 0 || self.cy == self.row.len() {
            return;
        }

        let mut x = self.cx - 1;
        let buf = self.row[self.cy].buffer();
        while x > 0 && buf[x].is_ascii_whitespace() {
            x -= 1;
        }
        // `x - 1` since x should stop at the last non-whitespace character to remove
        while x > 0 && !buf[x - 1].is_ascii_whitespace() {
            x -= 1;
        }

        if x < self.cx {
            self.row[self.cy].remove(x, self.cx);
            self.cx = x;
            self.modified = true;
            self.hl.needs_update = true;
        }
    }

    fn delete_right_char(&mut self) {
        self.move_cursor_one(CursorDir::Right);
        self.delete_char();
    }

    fn insert_line(&mut self) {
        if self.cy >= self.row.len() {
            self.row.push(Row::new(""));
        } else if self.cx >= self.row[self.cy].buffer().len() {
            self.row.insert(self.cy + 1, Row::new(""));
        } else {
            let split = String::from(&self.row[self.cy].buffer_str()[self.cx..]);
            self.row[self.cy].truncate(self.cx);
            self.row.insert(self.cy + 1, Row::new(split));
        }

        self.cy += 1;
        self.cx = 0;
        self.hl.needs_update = true;

        self.set_dirty_rows(self.cy);
    }

    fn move_cursor_one(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Up => self.cy = self.cy.saturating_sub(1),
            CursorDir::Left => {
                if self.cx > 0 {
                    self.cx -= 1;
                } else if self.cy > 0 {
                    // When moving to left at top of line, move cursor to end of previous line
                    self.cy -= 1;
                    self.cx = self.row[self.cy].buffer().len();
                }
            }
            CursorDir::Down => {
                // Allow to move cursor until next line to the last line of file to enable to add a
                // new line at the end.
                if self.cy < self.row.len() {
                    self.cy += 1;
                }
            }
            CursorDir::Right => {
                if self.cy < self.row.len() {
                    let len = self.row[self.cy].buffer().len();
                    if self.cx < len {
                        // Allow to move cursor until next col to the last col of line to enable to
                        // add a new character at the end of line.
                        self.cx += 1;
                    } else if self.cx >= len {
                        // When moving to right at the end of line, move cursor to top of next line.
                        self.cy += 1;
                        self.cx = 0;
                    }
                }
            }
        };

        // Snap cursor to end of line when moving up/down from longer line
        let len = self.row.get(self.cy).map(|r| r.buffer().len()).unwrap_or(0);
        if self.cx > len {
            self.cx = len;
        }
    }

    fn move_cursor_per_page(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Up => {
                self.cy = self.rowoff; // Set cursor to top of screen
                for _ in 0..self.screen_rows {
                    self.move_cursor_one(CursorDir::Up);
                }
            }
            CursorDir::Down => {
                // Set cursor to bottom of screen considering end of buffer
                self.cy = cmp::min(self.rowoff + self.screen_rows - 1, self.row.len());
                for _ in 0..self.screen_rows {
                    self.move_cursor_one(CursorDir::Down)
                }
            }
            _ => unreachable!(),
        }
    }

    fn move_cursor_to_buffer_edge(&mut self, dir: CursorDir) {
        match dir {
            CursorDir::Left => self.cx = 0,
            CursorDir::Right => {
                if self.cy < self.row.len() {
                    self.cx = self.row[self.cy].buffer().len();
                }
            }
            CursorDir::Up => self.cy = 0,
            CursorDir::Down => self.cy = self.row.len(),
        }
    }

    fn move_cursor_by_word(&mut self, dir: CursorDir) {
        #[derive(PartialEq)]
        enum CharKind {
            Ident,
            Punc,
            Space,
        }

        impl CharKind {
            fn new_at(rows: &Vec<Row>, x: usize, y: usize) -> Self {
                rows.get(y)
                    .and_then(|r| r.buffer().get(x))
                    .map(|b| {
                        if b.is_ascii_whitespace() {
                            CharKind::Space
                        } else if *b == b'_' || b.is_ascii_alphanumeric() {
                            CharKind::Ident
                        } else {
                            CharKind::Punc
                        }
                    })
                    .unwrap_or(CharKind::Space)
            }
        }

        fn at_word_start(left: &CharKind, right: &CharKind) -> bool {
            match (left, right) {
                (&CharKind::Space, &CharKind::Ident)
                | (&CharKind::Space, &CharKind::Punc)
                | (&CharKind::Punc, &CharKind::Ident)
                | (&CharKind::Ident, &CharKind::Punc) => true,
                _ => false,
            }
        }

        self.move_cursor_one(dir);
        let mut prev = CharKind::new_at(&self.row, self.cx, self.cy);
        self.move_cursor_one(dir);
        let mut current = CharKind::new_at(&self.row, self.cx, self.cy);

        loop {
            if self.cy == 0 && self.cx == 0 || self.cy == self.row.len() {
                return;
            }

            match dir {
                CursorDir::Right if at_word_start(&prev, &current) => return,
                CursorDir::Left if at_word_start(&current, &prev) => {
                    self.move_cursor_one(CursorDir::Right); // Adjust cursor position to start of word
                    return;
                }
                _ => {}
            }

            prev = current;
            self.move_cursor_one(dir);
            current = CharKind::new_at(&self.row, self.cx, self.cy);
        }
    }

    fn prompt<S, F>(&mut self, prompt: S, mut incremental_callback: F) -> io::Result<Option<String>>
    where
        S: AsRef<str>,
        F: FnMut(&mut Self, &str, InputSeq, bool) -> io::Result<()>,
    {
        let mut buf = String::new();
        let mut canceled = false;
        let prompt = prompt.as_ref();
        self.message = StatusMessage::info(prompt.replacen("{}", "", 1));
        self.refresh_screen()?;

        while let Some(seq) = self.input.next() {
            use KeySeq::*;

            let seq = seq?;
            let mut finished = false;
            match (&seq.key, seq.ctrl, seq.alt) {
                (&Unidentified, ..) => continue,
                (&Key(b'h'), true, ..) | (&Key(0x7f), ..) | (&DeleteKey, ..) if !buf.is_empty() => {
                    buf.pop();
                }
                (&Key(b'g'), true, ..) | (&Key(b'q'), true, ..) | (&Key(0x1b), ..) => {
                    finished = true;
                    canceled = true;
                }
                (&Key(b'\r'), ..) | (&Key(b'm'), true, ..) => {
                    finished = true;
                }
                (&Key(b), ..) => buf.push(b as char),
                _ => {}
            }

            incremental_callback(self, buf.as_str(), seq, finished)?;
            if finished {
                break;
            }
            self.message = StatusMessage::info(prompt.replacen("{}", &buf, 1));
            self.refresh_screen()?;
        }

        self.message = StatusMessage::info(if canceled { "" } else { "Canceled" });
        self.refresh_screen()?;

        Ok(if canceled || buf.is_empty() {
            None
        } else {
            Some(buf)
        })
    }

    fn handle_quit(&mut self) -> io::Result<AfterKeyPress> {
        if !self.modified || self.quitting {
            Ok(AfterKeyPress::Quit)
        } else {
            self.quitting = true;
            self.message = StatusMessage::error(
                "File has unsaved changes! Press ^Q again to quit or ^S to save",
            );
            Ok(AfterKeyPress::Nothing)
        }
    }

    fn process_keypress(&mut self, s: InputSeq) -> io::Result<AfterKeyPress> {
        use KeySeq::*;

        match (s.key, s.ctrl, s.alt) {
            (Key(b'p'), true, false) => self.move_cursor_one(CursorDir::Up),
            (Key(b'b'), true, false) => self.move_cursor_one(CursorDir::Left),
            (Key(b'n'), true, false) => self.move_cursor_one(CursorDir::Down),
            (Key(b'f'), true, false) => self.move_cursor_one(CursorDir::Right),
            (Key(b'v'), true, false) => self.move_cursor_per_page(CursorDir::Down),
            (Key(b'a'), true, false) => self.move_cursor_to_buffer_edge(CursorDir::Left),
            (Key(b'e'), true, false) => self.move_cursor_to_buffer_edge(CursorDir::Right),
            (Key(b'd'), true, false) => self.delete_right_char(),
            (Key(b'g'), true, false) => self.find()?,
            (Key(b'h'), true, false) => self.delete_char(),
            (Key(b'k'), true, false) => self.delete_until_end_of_line(),
            (Key(b'u'), true, false) => self.delete_until_head_of_line(),
            (Key(b'w'), true, false) => self.delete_word(),
            (Key(b'l'), true, false) => self.set_dirty_rows(self.rowoff), // Clear
            (Key(b's'), true, false) => self.save()?,
            (Key(b'i'), true, false) => self.insert_tab(),
            (Key(b'?'), true, false) => self.message = StatusMessage::info(HELP_TEXT),
            (Key(b'v'), false, true) => self.move_cursor_per_page(CursorDir::Up),
            (Key(b'f'), false, true) => self.move_cursor_by_word(CursorDir::Right),
            (Key(b'b'), false, true) => self.move_cursor_by_word(CursorDir::Left),
            (Key(b'<'), false, true) => self.move_cursor_to_buffer_edge(CursorDir::Up),
            (Key(b'>'), false, true) => self.move_cursor_to_buffer_edge(CursorDir::Down),
            (Key(0x08), false, false) => self.delete_char(), // Backspace
            (Key(0x7f), false, false) => self.delete_char(), // Delete key is mapped to \x1b[3~
            (Key(0x1b), false, false) => self.set_dirty_rows(self.rowoff), // Clear on ESC
            (Key(b'\r'), false, false) => self.insert_line(),
            (Key(byte), false, false) if !byte.is_ascii_control() => self.insert_char(byte as char),
            (Key(b'q'), true, ..) => return self.handle_quit(),
            (UpKey, false, false) => self.move_cursor_one(CursorDir::Up),
            (LeftKey, false, false) => self.move_cursor_one(CursorDir::Left),
            (DownKey, false, false) => self.move_cursor_one(CursorDir::Down),
            (RightKey, false, false) => self.move_cursor_one(CursorDir::Right),
            (PageUpKey, false, false) => self.move_cursor_per_page(CursorDir::Up),
            (PageDownKey, false, false) => self.move_cursor_per_page(CursorDir::Down),
            (HomeKey, false, false) => self.move_cursor_to_buffer_edge(CursorDir::Left),
            (EndKey, false, false) => self.move_cursor_to_buffer_edge(CursorDir::Right),
            (DeleteKey, false, false) => self.delete_right_char(),
            (LeftKey, true, false) => self.move_cursor_by_word(CursorDir::Left),
            (RightKey, true, false) => self.move_cursor_by_word(CursorDir::Right),
            (LeftKey, false, true) => self.move_cursor_to_buffer_edge(CursorDir::Left),
            (RightKey, false, true) => self.move_cursor_to_buffer_edge(CursorDir::Right),
            (Unidentified, ..) => unreachable!(),
            (Cursor(_, _), ..) => unreachable!(),
            (key, ctrl, alt) => {
                let m = match (ctrl, alt) {
                    (true, true) => "C-M-",
                    (true, false) => "C-",
                    (false, true) => "M-",
                    (false, false) => "",
                };
                self.message = StatusMessage::error(format!("Key '{}{}' not mapped", m, key))
            }
        }

        self.quitting = false;
        Ok(AfterKeyPress::Nothing)
    }

    fn ensure_screen_size(&mut self) -> io::Result<()> {
        if self.screen_cols > 0 && self.screen_rows > 0 {
            return Ok(());
        }

        // By moving cursor at the bottom-right corner by 'B' and 'C' commands, get the size of
        // current screen. \x1b[9999;9999H is not available since it does not guarantee cursor
        // stops on the corner. Finaly command 'n' queries cursor position.
        let mut stdout = io::stdout();
        stdout.write(b"\x1b[9999C\x1b[9999B\x1b[6n")?;
        stdout.flush()?;

        // Wait for response from terminal discarding other sequences
        for seq in &mut self.input {
            if let KeySeq::Cursor(r, c) = seq?.key {
                self.screen_cols = c;
                self.screen_rows = r.saturating_sub(2);
                break;
            }
        }

        Ok(())
    }

    fn run(&mut self) -> io::Result<()> {
        self.ensure_screen_size()?;

        // Render first screen
        self.refresh_screen()?;

        while let Some(seq) = self.input.next() {
            let seq = seq?;
            if seq.key == KeySeq::Unidentified {
                continue; // Ignore
            }
            if self.process_keypress(seq)? == AfterKeyPress::Quit {
                break;
            }
            self.refresh_screen()?; // Update screen after keypress
        }

        self.clear_screen() // Finally clear screen on exit
    }
}

fn main() -> io::Result<()> {
    let input = StdinRawMode::new()?.input_keys();
    let mut editor = Editor::new(term_size::dimensions_stdout(), input);
    if let Some(arg) = std::env::args().skip(1).next() {
        editor.open_file(arg)?;
    }
    editor.run()
}
