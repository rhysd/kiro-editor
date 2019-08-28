use std::ffi::OsStr;
use std::path::Path;

pub enum Indent {
    AsIs,
    Fixed(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    Plain,
    C,
    Rust,
    JavaScript,
    Go,
    Cpp,
}

impl Default for Language {
    fn default() -> Language {
        Language::Plain
    }
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
            Cpp => "c++",
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
            Cpp => &["cpp", "hpp", "cxx", "hxx", "cc", "hh"],
        }
    }

    pub fn indent(self) -> Indent {
        use Language::*;
        match self {
            Plain | Go => Indent::AsIs,
            C | Rust | Cpp => Indent::Fixed("    "),
            JavaScript => Indent::Fixed("  "),
        }
    }

    pub fn detect<P: AsRef<Path>>(path: P) -> Language {
        use Language::*;
        if let Some(ext) = path.as_ref().extension().and_then(OsStr::to_str) {
            for lang in &[C, Rust, JavaScript, Go, Cpp] {
                if lang.file_exts().contains(&ext) {
                    return *lang;
                }
            }
        }
        Plain
    }
}
