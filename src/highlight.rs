use std::iter;

use crate::language::Language;
use crate::row::Row;
use crate::term_color::Color;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Highlight {
    Normal,
    Number,
    String,
    Comment,
    Keyword,
    Type,
    Definition,
    Char,
    Statement,
    Boolean,
    SpecialVar,
    Search,
    Match,
}

impl Highlight {
    pub fn color(self) -> Color {
        use Color::*;
        use Highlight::*;
        match self {
            Normal => Reset,
            Number => Purple,
            String => Green,
            Comment => Gray,
            Keyword => Blue,
            Type => Orange,
            Definition => Yellow,
            Char => Green,
            Statement => Red,
            Boolean => Purple,
            SpecialVar => Cyan,
            Search => OrangeBg,
            Match => YellowBg,
        }
    }
}

struct SyntaxHighlight {
    lang: Language,
    string_quotes: &'static [char],
    number: bool,
    hex_number: bool,
    bin_number: bool,
    number_delim: Option<char>,
    character: bool,
    line_comment: Option<&'static str>,
    block_comment: Option<(&'static str, &'static str)>,
    keywords: &'static [&'static str],
    control_statements: &'static [&'static str],
    builtin_types: &'static [&'static str],
    boolean_constants: &'static [&'static str],
    special_vars: &'static [&'static str],
    definition_keywords: &'static [&'static str],
}

const PLAIN_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Plain,
    number: false,
    hex_number: false,
    bin_number: false,
    number_delim: None,
    string_quotes: &[],
    character: false,
    line_comment: None,
    block_comment: None,
    keywords: &[],
    control_statements: &[],
    builtin_types: &[],
    boolean_constants: &[],
    special_vars: &[],
    definition_keywords: &[],
};

const C_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::C,
    number: true,
    hex_number: true,
    bin_number: false,
    number_delim: None,
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
    boolean_constants: &[],
    special_vars: &[],
    definition_keywords: &["enum", "struct", "union"],
};

const RUST_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Rust,
    number: true,
    hex_number: true,
    bin_number: true,
    number_delim: Some('_'),
    string_quotes: &['"'],
    character: true,
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "as", "async", "await", "const", "crate", "dyn", "enum", "extern", "fn", "impl", "let",
        "mod", "move", "mut", "pub", "ref", "Self", "static", "struct", "super", "trait", "type",
        "union", "unsafe", "use", "where",
    ],
    control_statements: &[
        "break", "continue", "else", "for", "if", "in", "loop", "match", "return", "while",
    ],
    builtin_types: &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char", "Box", "Option", "Some", "None", "Result", "Ok", "Err",
        "String", "Vec",
    ],
    boolean_constants: &["true", "false"],
    special_vars: &["self"],
    definition_keywords: &[
        "fn", "let", "const", "mod", "struct", "enum", "trait", "union",
    ],
};

const JAVASCRIPT_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::JavaScript,
    number: true,
    hex_number: true,
    bin_number: false,
    number_delim: None,
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
        "let",
        "new",
        "super",
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
    boolean_constants: &["true", "false"],
    special_vars: &["null", "this", "undefined"],
    definition_keywords: &["class", "const", "function", "var", "let"],
};

const GO_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Go,
    number: true,
    hex_number: true,
    bin_number: true,
    number_delim: Some('_'),
    string_quotes: &['"', '`'],
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
    boolean_constants: &["true", "false"],
    special_vars: &["nil"],
    definition_keywords: &[
        "const",
        "func",
        "interface",
        "package",
        "struct",
        "type",
        "var",
    ],
};

const CPP_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::C,
    number: true,
    hex_number: true,
    bin_number: true,
    number_delim: Some('\''),
    string_quotes: &['"'],
    character: true,
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "alignas",
        "alignof",
        "and",
        "and_eq",
        "asm",
        "atomic_cancel",
        "atomic_commit",
        "atomic_noexcept",
        "auto",
        "bitand",
        "bitor",
        "bool",
        "class",
        "compl",
        "concept",
        "const",
        "consteval",
        "constexpr",
        "const_cast",
        "co_await",
        "co_return",
        "co_yield",
        "decltype",
        "delete",
        "dynamic_cast",
        "enum",
        "explicit",
        "export",
        "extern",
        "friend",
        "inline",
        "mutable",
        "namespace",
        "new",
        "noexcept",
        "not",
        "not_eq",
        "nullptr",
        "operator",
        "or",
        "or_eq",
        "private",
        "protected",
        "public",
        "reflexpr",
        "register",
        "reinterpret_cast",
        "requires",
        "sizeof",
        "static",
        "static_assert",
        "static_cast",
        "struct",
        "synchronized",
        "template",
        "thread_local",
        "typedef",
        "typeid",
        "typename",
        "union",
        "using",
        "virtual",
        "volatile",
        "xor",
        "xor_eq",
        // XXX: Contextual keywords
        "override",
        "final",
        "import",
        "module",
        "transaction_safe",
        "transaction_safe_dynamic",
    ],
    control_statements: &[
        "break", "case", "catch", "continue", "default", "do", "else", "for", "goto", "if",
        "return", "switch", "throw", "try", "while",
    ],
    builtin_types: &[
        "char", "char8_t", "char16_t", "char32_t", "double", "float", "int", "long", "short",
        "signed", "unsigned", "void", "wchar_t",
    ],
    boolean_constants: &["true", "false"],
    special_vars: &["this"],
    definition_keywords: &[
        "class",
        "concept",
        "enum",
        "namespace",
        "typename",
        "union",
        "module",
    ],
};

const PYTHON_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Python,
    number: true,
    hex_number: true,
    bin_number: true,
    number_delim: Some('_'),
    string_quotes: &['"', '\''], // TODO: Multi-line strings '''...'''
    character: false,
    line_comment: Some("#"),
    block_comment: None,
    keywords: &[
        "and", "as", "assert", "async", "await", "class", "def", "del", "from", "global", "import",
        "in", "is", "lambda", "nonlocal", "not", "or", "with",
    ],
    control_statements: &[
        "break", "continue", "elif", "else", "except", "finally", "for", "if", "pass", "raise",
        "return", "try", "while", "yield",
    ],
    builtin_types: &[
        "int",
        "float",
        "complex",
        "list",
        "tuple",
        "range",
        "str",
        "bytes",
        "bytearray",
        "memoryview",
        "set",
        "frozenset",
        "dict",
    ],
    boolean_constants: &["True", "False"],
    special_vars: &["self", "None"],
    definition_keywords: &["def", "class", "global", "nonlocal"],
};

const JULIA_SYNTAX: SyntaxHighlight = SyntaxHighlight {
    lang: Language::Julia,
    number: true,
    hex_number: true,
    bin_number: true,
    number_delim: Some('_'),
    string_quotes: &['"'], // TODO: Multi-line strings """..."""
    character: false,
    line_comment: Some("#"),
    block_comment: Some(("#=", "=#")),
    keywords: &[
        "baremodule",
        "begin",
        "break",
        "catch",
        "const",
        "continue",
        "do",
        "else",
        "elseif",
        "end",
        "export",
        "finally",
        "for",
        "function",
        "global",
        "if",
        "import",
        "let",
        "local",
        "macro",
        "module",
        "quote",
        "return",
        "struct",
        "try",
        "using",
        "while",
        "abstract type",
        "primitive type",
        "mutable struct",
    ],
    control_statements: &[
        "break", "continue", "elif", "else", "except", "finally", "for", "if", "pass", "raise",
        "return", "try", "while", "yield",
    ],
    builtin_types: &[
        "AbstractArray",
        "AbstractChannel",
        "AbstractChar",
        "AbstractDict",
        "AbstractDisplay",
        "AbstractSet",
        "AbstractString",
        "Array",
        "Any",
        "Dict",
        "Bool",
        "Complex",
        "Real",
        "String",
        "AbstractFloat",
        "AbstractIrrational",
        "BigFloat",
        "Float16",
        "Float32",
        "Float64",
        "Integer",
        "Int",
        "Signed",
        "Unsigned",
        "Rational",
    ],
    boolean_constants: &["true", "false"],
    special_vars: &["in", "isa", "where", "nothing", "missing"],
    definition_keywords: &["def", "class", "global", "nonlocal"],
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
            Cpp => &CPP_SYNTAX,
            Python => &PYTHON_SYNTAX,
            Julia => &JULIA_SYNTAX,
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum NumLit {
    Digit,
    Hex,
    Bin,
}

enum ParseStep {
    Ahead(usize),
    Break,
}

fn is_sep(c: char) -> bool {
    c.is_ascii_whitespace() || (c.is_ascii_punctuation() && c != '_') || c == '\0'
}

struct Highlighter<'a> {
    syntax: &'a SyntaxHighlight,
    prev_quote: Option<char>,
    in_block_comment: bool,
    prev_hl: Highlight,
    prev_char: char,
    num: NumLit,
    after_def_keyword: bool,
}

impl<'a> Highlighter<'a> {
    fn new<'b: 'a>(syntax: &'b SyntaxHighlight) -> Self {
        Self {
            syntax,
            prev_quote: None,
            in_block_comment: false,
            prev_hl: Highlight::Normal,
            prev_char: '\0',
            num: NumLit::Digit,
            after_def_keyword: false,
        }
    }

    fn eat_n(
        &mut self,
        out: &mut [Highlight],
        input: &str,
        hl: Highlight,
        len: usize,
    ) -> ParseStep {
        debug_assert!(len > 0);
        debug_assert!(!input.is_empty());
        debug_assert!(!out.is_empty());

        for out in out.iter_mut().take(len) {
            *out = hl;
        }
        self.prev_hl = hl;
        self.prev_char = input.chars().nth(len - 1).unwrap();
        ParseStep::Ahead(len)
    }

    fn eat_one(&mut self, out: &mut [Highlight], c: char, hl: Highlight) -> ParseStep {
        out[0] = hl;
        self.prev_hl = hl;
        self.prev_char = c;
        ParseStep::Ahead(1)
    }

    fn highlight_block_comment(
        &mut self,
        start: &str,
        end: &str,
        c: char,
        out: &mut [Highlight],
        input: &str,
    ) -> Option<ParseStep> {
        if self.prev_quote.is_some() {
            return None;
        }

        let comment_delim = if self.in_block_comment && input.starts_with(end) {
            self.in_block_comment = false;
            end
        } else if !self.in_block_comment && input.starts_with(start) {
            self.in_block_comment = true;
            start
        } else {
            return if self.in_block_comment {
                Some(self.eat_one(out, c, Highlight::Comment))
            } else {
                None
            };
        };

        // Consume whole '/*' here. Otherwise such as '/*/' is wrongly accepted
        Some(self.eat_n(out, input, Highlight::Comment, comment_delim.len()))
    }

    fn highlight_line_comment(
        &mut self,
        leader: &str,
        out: &mut [Highlight],
        input: &str,
    ) -> Option<ParseStep> {
        if self.prev_quote.is_none() && input.starts_with(leader) {
            // Highlight as comment until end of line
            for hl in out.iter_mut() {
                *hl = Highlight::Comment;
            }
            Some(ParseStep::Break)
        } else {
            None
        }
    }

    fn highlight_string(&mut self, c: char, out: &mut [Highlight]) -> Option<ParseStep> {
        if let Some(q) = self.prev_quote {
            // In string literal. XXX: "\\" is not highlighted correctly
            if self.prev_char != '\\' && q == c {
                self.prev_quote = None;
            }
            Some(self.eat_one(out, c, Highlight::String))
        } else if self.syntax.string_quotes.contains(&c) {
            self.prev_quote = Some(c);
            Some(self.eat_one(out, c, Highlight::String))
        } else {
            None
        }
    }

    fn highlight_ident(&mut self, out: &mut [Highlight], input: &str) -> Option<ParseStep> {
        fn lex_ident(mut input: &str) -> Option<&str> {
            for (i, c) in input.char_indices() {
                if is_sep(c) {
                    input = &input[..i];
                    break;
                }
            }
            if input.is_empty() {
                None
            } else {
                Some(input)
            }
        }

        // let iter_words = |words: &'static [&'static str], hl| words.iter().zip(iter::repeat(hl));
        fn iter_words<'a>(
            words: &'a [&'a str],
            hl: Highlight,
        ) -> impl Iterator<Item = (&&'a str, Highlight)> {
            words.iter().zip(iter::repeat(hl))
        }

        lex_ident(input).as_ref().and_then(|ident| {
            use Highlight::*;

            let keyword = iter_words(self.syntax.keywords, Keyword)
                .chain(iter_words(self.syntax.control_statements, Statement))
                .chain(iter_words(self.syntax.builtin_types, Type))
                .chain(iter_words(self.syntax.boolean_constants, Boolean))
                .chain(iter_words(self.syntax.special_vars, SpecialVar))
                .find(|(k, _)| *k == ident);

            let definition = keyword.or_else(|| {
                if self.after_def_keyword {
                    Some((ident, Highlight::Definition))
                } else {
                    None
                }
            });

            if keyword.is_some() && self.syntax.definition_keywords.contains(&ident) {
                self.after_def_keyword = true;
            }

            let highlighted = keyword.or(definition);
            highlighted.map(|(ident, hl)| self.eat_n(out, input, hl, ident.len()))
        })
    }

    fn highlight_prefix_number(
        &mut self,
        num: NumLit,
        is_bound: bool,
        c: char,
        out: &mut [Highlight],
        input: &str,
    ) -> Option<ParseStep> {
        let prefix: &[_] = match num {
            NumLit::Hex => b"0x",
            NumLit::Bin => b"0b",
            NumLit::Digit => unreachable!(),
        };

        fn is_num_char(b: u8, num: NumLit, delim: Option<char>) -> bool {
            match num {
                NumLit::Hex if b.is_ascii_hexdigit() => true,
                NumLit::Bin if b"01".contains(&b) => true,
                _ => delim == Some(b as char),
            }
        }

        let bytes = input.as_bytes();
        if is_bound {
            if bytes.starts_with(prefix)
                && bytes.len() > prefix.len()
                && is_num_char(bytes[prefix.len()], num, self.syntax.number_delim)
            {
                self.num = num;
                return Some(self.eat_n(out, input, Highlight::Number, prefix.len()));
            }
        } else if self.num == num
            && self.prev_hl == Highlight::Number
            && c.is_ascii()
            && is_num_char(c as u8, num, self.syntax.number_delim)
        {
            return Some(self.eat_one(out, c, Highlight::Number));
        }

        None
    }

    fn highlight_digit_number(
        &mut self,
        is_bound: bool,
        c: char,
        out: &mut [Highlight],
    ) -> Option<ParseStep> {
        let prev_is_number = self.num == NumLit::Digit && self.prev_hl == Highlight::Number;
        if is_bound {
            if c.is_ascii_digit() || prev_is_number && c == '.' {
                self.num = NumLit::Digit;
                return Some(self.eat_one(out, c, Highlight::Number));
            }
        } else if prev_is_number && (self.syntax.number_delim == Some(c) || c.is_ascii_digit()) {
            return Some(self.eat_one(out, c, Highlight::Number));
        }

        None
    }

    fn highlight_char(&mut self, out: &mut [Highlight], input: &str) -> Option<ParseStep> {
        if self.syntax.number_delim == Some('\'') && self.prev_hl == Highlight::Number {
            return None; // Consider number literal delimiter in C++ (e.g. `123'456'789`)
        }

        let mut i = input.chars();
        let len = match (i.next(), i.next(), i.next(), i.next()) {
            (Some('\''), Some('\\'), _, Some('\'')) => Some(4),
            (Some('\''), _, Some('\''), _) => Some(3),
            _ => None,
        };

        len.map(|len| self.eat_n(out, input, Highlight::Char, len))
    }

    fn highlight_one(&mut self, c: char, out: &mut [Highlight], input: &str) -> ParseStep {
        if self.after_def_keyword && !c.is_ascii_whitespace() && is_sep(c) {
            self.after_def_keyword = false;
        }

        macro_rules! try_highlight {
            ($call:expr) => {
                if let Some(step) = $call {
                    return step;
                }
            };
        }

        if let Some((comment_start, comment_end)) = self.syntax.block_comment {
            try_highlight!(self.highlight_block_comment(comment_start, comment_end, c, out, input));
        }

        if let Some(comment_leader) = self.syntax.line_comment {
            try_highlight!(self.highlight_line_comment(comment_leader, out, input));
        }

        if self.syntax.character {
            try_highlight!(self.highlight_char(out, input));
        }

        if !self.syntax.string_quotes.is_empty() {
            try_highlight!(self.highlight_string(c, out));
        }

        let is_bound = is_sep(self.prev_char) ^ is_sep(c);

        // Highlight identifiers
        if is_bound {
            try_highlight!(self.highlight_ident(out, input));
        }

        if self.syntax.hex_number {
            try_highlight!(self.highlight_prefix_number(NumLit::Hex, is_bound, c, out, input));
        }

        if self.syntax.bin_number {
            try_highlight!(self.highlight_prefix_number(NumLit::Bin, is_bound, c, out, input));
        }

        if self.syntax.number {
            try_highlight!(self.highlight_digit_number(is_bound, c, out));
        }

        self.eat_one(out, c, Highlight::Normal)
    }

    fn highlight_line(&mut self, out: &mut [Highlight], row: &str) {
        if self.syntax.lang == Language::Plain {
            // On 'plain' syntax, skip highlighting since nothing is highlighted.
            return;
        }

        // Initialize states for line highlighting
        self.prev_hl = Highlight::Normal;
        self.prev_char = '\0';
        self.num = NumLit::Digit;
        self.after_def_keyword = false;

        let mut iter = row.char_indices().enumerate();
        while let Some((x, (idx, c))) = iter.next() {
            let input = &row[idx..];
            let out = &mut out[x..];
            match self.highlight_one(c, out, input) {
                ParseStep::Ahead(len) if len >= 2 => {
                    // while statement always consume one character at top. Eat input chars considering that.
                    iter.nth(len.saturating_sub(2));
                }
                ParseStep::Ahead(len) if len == 1 => { /* Go next */ }
                ParseStep::Ahead(_) => unreachable!(),
                ParseStep::Break => break,
            }
        }
    }
}

pub struct RegionHighlight {
    pub hl: Highlight,
    pub start: (usize, usize),
    pub end: (usize, usize),
}

impl RegionHighlight {
    fn contains(&self, (x, y): (usize, usize)) -> bool {
        let ((sx, sy), (ex, ey)) = (self.start, self.end);
        if y < sy || ey < y {
            false
        } else if sy < y && y < ey {
            true
        } else {
            sx <= x && x < ex // Exclusive
        }
    }
}

pub struct Highlighting {
    pub needs_update: bool,
    // One item per render text byte
    pub lines: Vec<Vec<Highlight>>, // TODO: One item per one character
    previous_bottom_of_screen: usize,
    matched: Vec<RegionHighlight>,
    syntax: &'static SyntaxHighlight,
}

impl Default for Highlighting {
    fn default() -> Self {
        Highlighting {
            needs_update: false,
            lines: vec![],
            previous_bottom_of_screen: 0,
            matched: vec![],
            syntax: &PLAIN_SYNTAX,
        }
    }
}

impl Highlighting {
    pub fn new(lang: Language, rows: &[Row]) -> Highlighting {
        Highlighting {
            needs_update: true,
            lines: rows
                .iter()
                .map(|r| {
                    iter::repeat(Highlight::Normal)
                        .take(r.render_text().chars().count()) // TODO: One item per one character
                        .collect()
                })
                .collect(),
            previous_bottom_of_screen: 0,
            matched: vec![],
            syntax: SyntaxHighlight::for_lang(lang),
        }
    }

    pub fn lang_changed(&mut self, new_lang: Language) {
        if self.syntax.lang == new_lang {
            return;
        }
        self.syntax = SyntaxHighlight::for_lang(new_lang);
        self.needs_update = true;
    }

    fn highlight_match(&mut self, overwrite: Option<Highlight>) {
        for region in self.matched.iter() {
            let highlight = overwrite.unwrap_or(region.hl);
            for y in region.start.1..=region.end.1 {
                for (x, hl) in self.lines[y].iter_mut().enumerate() {
                    if region.contains((x, y)) {
                        *hl = highlight;
                    }
                }
            }
        }
    }

    pub fn update(&mut self, rows: &[Row], bottom_of_screen: usize) {
        if !self.needs_update && bottom_of_screen <= self.previous_bottom_of_screen {
            return;
        }

        let mut highlighter = Highlighter::new(&self.syntax);

        self.lines.resize_with(rows.len(), Default::default);
        for (y, ref row) in rows.iter().enumerate().take(bottom_of_screen) {
            let row = row.render_text();
            self.lines[y].resize(row.chars().count(), Highlight::Normal); // TODO: One item per one character

            highlighter.highlight_line(&mut self.lines[y], row);
        }

        // Overwrite matched region
        //
        // TODO: Move logic to highlighter rather than overwriting highlights after.
        // Give self.matched to Highlighter::new() and it checks each cell should be highlighted as match
        self.highlight_match(None);

        self.needs_update = false;
        self.previous_bottom_of_screen = bottom_of_screen;
    }

    pub fn set_matches(&mut self, matches: Vec<RegionHighlight>) {
        self.clear_previous_match();
        self.matched = matches;
    }

    pub fn clear_previous_match(&mut self) -> Option<usize> {
        let dirty_start = self.matched.iter().map(|r| r.start.1).min();
        if dirty_start.is_some() {
            if self.syntax.lang == Language::Plain {
                // Back to normal color. It is necessary on plain file type since it skips highlighting.
                // Otherwise, this process is unnecessary because next highlighting will overwrite match
                // highlights.
                self.highlight_match(Some(Highlight::Normal));
            }
            self.matched.clear();
        }
        dirty_start
    }
}
