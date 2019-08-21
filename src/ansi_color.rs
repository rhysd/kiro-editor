use term::terminfo::TermInfo;

#[derive(Clone, Copy)]
pub enum ColorSupport {
    Extended256,
    Only16,
}

impl ColorSupport {
    pub fn from_env() -> ColorSupport {
        TermInfo::from_env()
            .ok()
            .and_then(|info| {
                info.numbers.get("colors").map(|colors| {
                    if *colors == 256 {
                        ColorSupport::Extended256
                    } else {
                        ColorSupport::Only16
                    }
                })
            })
            .unwrap_or(ColorSupport::Only16)
    }
}

#[derive(PartialEq)]
pub enum AnsiColor {
    Reset,
    Red,
    Green,
    Gray,
    Yellow,
    Blue,
    Purple,
    Cyan,
    CyanUnderline,
    RedBG,
    Invert,
}

impl AnsiColor {
    pub fn sequence(&self, support: ColorSupport) -> &'static [u8] {
        // 'm' sets attributes to text printed after: https://vt100.net/docs/vt100-ug/chapter3.html#SGR
        // Color table: https://en.wikipedia.org/wiki/ANSI_escape_code#Colors
        use AnsiColor::*;
        match support {
            ColorSupport::Extended256 => match self {
                // From color palette of gruvbox: https://github.com/morhetz/gruvbox#palette
                Reset => b"\x1b[39;0m\x1b[38;5;223m\x1b[48;5;235m",
                Red => b"\x1b[38;5;167m",
                Green => b"\x1b[38;5;142m",
                Gray => b"\x1b[38;5;246m",
                Yellow => b"\x1b[38;5;214m",
                Blue => b"\x1b[38;5;109m",
                Purple => b"\x1b[38;5;175m",
                Cyan => b"\x1b[38;5;108m",
                CyanUnderline => b"\x1b[38;4;208m",
                RedBG => b"\x1b[48;5;124m",
                Invert => b"\x1b[7m",
            },
            ColorSupport::Only16 => match self {
                Reset => b"\x1b[39;0m",
                Red => b"\x1b[91m",
                Green => b"\x1b[32m",
                Gray => b"\x1b[90m",
                Yellow => b"\x1b[33m",
                Blue => b"\x1b[94m",
                Purple => b"\x1b[95m",
                Cyan => b"\x1b[96m",
                CyanUnderline => b"\x1b[96;4m",
                RedBG => b"\x1b[41m",
                Invert => b"\x1b[7m",
            },
        }
    }

    pub fn is_underlined(self) -> bool {
        return self == AnsiColor::CyanUnderline;
    }
}
