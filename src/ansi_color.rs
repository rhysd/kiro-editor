#[derive(PartialEq)]
pub enum AnsiColor {
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
    pub fn sequence(&self) -> &'static [u8] {
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
