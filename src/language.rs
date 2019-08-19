use std::ffi::OsStr;
use std::path::Path;

pub enum Indent {
    AsIs,
    Fixed(&'static str),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Language {
    Plain,
    C,
    Rust,
    JavaScript,
    Go,
}

impl Language {
    pub fn name(self) -> &'static str {
        use Language::*;
        match self {
            Plain => "plain",
            C => "c",
            Rust => "rust",
            JavaScript => "javascript",
            Go => "go",
        }
    }

    fn file_exts(self) -> &'static [&'static str] {
        use Language::*;
        match self {
            Plain => &[],
            C => &["c", "h"],
            Rust => &["rs"],
            JavaScript => &["js"],
            Go => &["go"],
        }
    }

    pub fn indent(self) -> Indent {
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

    pub fn detect<P: AsRef<Path>>(path: P) -> Language {
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
