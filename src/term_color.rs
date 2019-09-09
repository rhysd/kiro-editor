use std::env;
use term::terminfo::TermInfo;

#[derive(PartialEq, Clone, Copy)]
pub enum Color {
    Reset,
    Red,
    Green,
    Gray,
    Yellow,
    Orange,
    Blue,
    Purple,
    Cyan,
    RedBG,
    YellowBG,
    Invert,
}

#[derive(Clone, Copy)]
pub enum TermColor {
    TrueColor,
    Extended256,
    Only16,
}

impl TermColor {
    pub fn from_env() -> TermColor {
        env::var("COLORTERM")
            .ok()
            .and_then(|v| {
                if v == "truecolor" {
                    Some(TermColor::TrueColor)
                } else {
                    None
                }
            })
            .or_else(|| {
                TermInfo::from_env().ok().and_then(|info| {
                    info.numbers.get("colors").map(|colors| {
                        if *colors == 256 {
                            TermColor::Extended256
                        } else {
                            TermColor::Only16
                        }
                    })
                })
            })
            .unwrap_or(TermColor::Only16)
    }

    pub fn sequence(self, color: Color) -> &'static [u8] {
        // From color palette of gruvbox: https://github.com/morhetz/gruvbox#palette
        //
        // 'm' sets attributes to text printed after: https://vt100.net/docs/vt100-ug/chapter3.html#SGR
        // Color table: https://en.wikipedia.org/wiki/ANSI_escape_code#Colors
        //
        // 256 colors sequences are '\x1b[38;5;<n>m' (for fg) or '\x1b[48;5;<n>m (for bg)
        // https://www.xfree86.org/current/ctlseqs.html
        //
        // 24bit colors sequences are '\x1b[38;2;<r>;<g>;<b>m' (for fg) or '\x1b[48;2;<r>;<g>;<b>m' (for fg)
        // https://en.wikipedia.org/wiki/ANSI_escape_code#Colors

        macro_rules! rgb_color {
            (fg, $r:expr, $g:expr, $b:expr) => {
                concat!("\x1b[38;2;", $r, ';', $g, ';', $b, "m")
            };
            (bg, $r:expr, $g:expr, $b:expr) => {
                concat!("\x1b[48;2;", $r, ';', $g, ';', $b, "m")
            };
        }

        use Color::*;
        match self {
            TermColor::TrueColor => match color {
                Reset => concat!(
                    "\x1b[39;0m",
                    rgb_color!(fg, 0xfb, 0xf1, 0xc7),
                    rgb_color!(bg, 0x28, 0x28, 0x28),
                )
                .as_bytes(),
                Red => rgb_color!(fg, 0xfb, 0x49, 0x34).as_bytes(),
                Green => rgb_color!(fg, 0xb8, 0xbb, 0x26).as_bytes(),
                Gray => rgb_color!(fg, 0xa8, 0x99, 0x84).as_bytes(),
                Yellow => rgb_color!(fg, 0xfa, 0xbd, 0x2f).as_bytes(),
                Orange => rgb_color!(fg, 0xfe, 0x80, 0x19).as_bytes(),
                Blue => rgb_color!(fg, 0x83, 0xa5, 0x98).as_bytes(),
                Purple => rgb_color!(fg, 0xd3, 0x86, 0x9b).as_bytes(),
                Cyan => rgb_color!(fg, 0x8e, 0xc0, 0x7c).as_bytes(),
                RedBG => rgb_color!(bg, 0xcc, 0x24, 0x1d).as_bytes(),
                YellowBG => concat!(
                    rgb_color!(fg, 0x28, 0x28, 0x28),
                    rgb_color!(bg, 0xd7, 0x99, 0x21),
                )
                .as_bytes(),
                Invert => b"\x1b[7m",
            },
            TermColor::Extended256 => match color {
                Reset => b"\x1b[39;0m\x1b[38;5;230m\x1b[48;5;235m",
                Red => b"\x1b[38;5;167m",
                Green => b"\x1b[38;5;142m",
                Gray => b"\x1b[38;5;246m",
                Yellow => b"\x1b[38;5;214m",
                Orange => b"\x1b[38;5;208m",
                Blue => b"\x1b[38;5;109m",
                Purple => b"\x1b[38;5;175m",
                Cyan => b"\x1b[38;5;108m",
                RedBG => b"\x1b[48;5;124m",
                YellowBG => b"\x1b[38;5;235m\x1b[48;5;214m",
                Invert => b"\x1b[7m",
            },
            TermColor::Only16 => match color {
                Reset => b"\x1b[39;0m",
                Red => b"\x1b[91m",
                Green => b"\x1b[32m",
                Gray => b"\x1b[90m",
                Yellow => b"\x1b[93m",
                Orange => b"\x1b[33m", // No orange color in 16 colors. Use darker yellow instead
                Blue => b"\x1b[94m",
                Purple => b"\x1b[95m",
                Cyan => b"\x1b[96m",
                RedBG => b"\x1b[41m",
                YellowBG => b"\x1b[103m\x1b[30m",
                Invert => b"\x1b[7m",
            },
        }
    }
}
