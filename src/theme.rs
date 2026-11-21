//! Colour themes: hc-tui's own chrome plus the default colours and 16-colour
//! palette used inside panes (truecolor output stays as programs draw it).

use ratatui::style::Color;

pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub bg2: Color,
    pub fg: Color,
    pub dim: Color,
    /// Text drawn on accent-coloured badges.
    pub ink: Color,
    pub yellow: Color,
    pub red: Color,
    pub green: Color,
    pub blue: Color,
    pub aqua: Color,
    pub orange: Color,
    pub wait_bg: Color,
    /// The 16 basic terminal colours inside panes; None keeps the terminal's own.
    pub ansi: Option<[Color; 16]>,
    /// Default text and background inside panes; None keeps the terminal's own.
    pub pane: Option<(Color, Color)>,
}

const fn rgb(v: u32) -> Color {
    Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

const fn palette(v: [u32; 16]) -> Option<[Color; 16]> {
    let mut out = [Color::Reset; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = rgb(v[i]);
        i += 1;
    }
    Some(out)
}

#[allow(clippy::too_many_arguments)]
const fn theme(
    name: &'static str,
    [bg, bg2, fg, dim]: [u32; 4],
    [yellow, red, green, blue, aqua, orange, wait_bg]: [u32; 7],
    ansi: [u32; 16],
) -> Theme {
    Theme {
        name,
        bg: rgb(bg),
        bg2: rgb(bg2),
        fg: rgb(fg),
        dim: rgb(dim),
        ink: rgb(bg),
        yellow: rgb(yellow),
        red: rgb(red),
        green: rgb(green),
        blue: rgb(blue),
        aqua: rgb(aqua),
        orange: rgb(orange),
        wait_bg: rgb(wait_bg),
        ansi: palette(ansi),
        pane: Some((rgb(fg), rgb(bg))),
    }
}

pub static THEMES: &[Theme] = &[
    theme(
        "Gruvbox Dark",
        [0x282828, 0x3c3836, 0xebdbb2, 0x928374],
        [
            0xfabd2f, 0xfb4934, 0xb8bb26, 0x83a598, 0x8ec07c, 0xfe8019, 0x4a3b12,
        ],
        [
            0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984,
            0x928374, 0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xebdbb2,
        ],
    ),
    theme(
        "Monokai",
        [0x272822, 0x3e3d32, 0xf8f8f2, 0x75715e],
        [
            0xe6db74, 0xf92672, 0xa6e22e, 0x66d9ef, 0xa1efe4, 0xfd971f, 0x4d4a26,
        ],
        [
            0x272822, 0xf92672, 0xa6e22e, 0xf4bf75, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xf8f8f2,
            0x75715e, 0xf92672, 0xa6e22e, 0xf4bf75, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xf9f8f5,
        ],
    ),
    theme(
        "Dracula",
        [0x282a36, 0x44475a, 0xf8f8f2, 0x6272a4],
        [
            0xf1fa8c, 0xff5555, 0x50fa7b, 0xbd93f9, 0x8be9fd, 0xffb86c, 0x4a4a2a,
        ],
        [
            0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
            0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
        ],
    ),
    theme(
        "Tokyo Night",
        [0x1a1b26, 0x292e42, 0xc0caf5, 0x565f89],
        [
            0xe0af68, 0xf7768e, 0x9ece6a, 0x7aa2f7, 0x7dcfff, 0xff9e64, 0x3b3324,
        ],
        [
            0x15161e, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xa9b1d6,
            0x414868, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xc0caf5,
        ],
    ),
    theme(
        "Catppuccin Mocha",
        [0x1e1e2e, 0x313244, 0xcdd6f4, 0x7f849c],
        [
            0xf9e2af, 0xf38ba8, 0xa6e3a1, 0x89b4fa, 0x94e2d5, 0xfab387, 0x403a2e,
        ],
        [
            0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de,
            0x585b70, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xa6adc8,
        ],
    ),
    theme(
        "One Dark",
        [0x282c34, 0x3e4451, 0xabb2bf, 0x5c6370],
        [
            0xe5c07b, 0xe06c75, 0x98c379, 0x61afef, 0x56b6c2, 0xd19a66, 0x433d2c,
        ],
        [
            0x282c34, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
            0x5c6370, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xffffff,
        ],
    ),
    theme(
        "Nord",
        [0x2e3440, 0x3b4252, 0xd8dee9, 0x616e88],
        [
            0xebcb8b, 0xbf616a, 0xa3be8c, 0x81a1c1, 0x88c0d0, 0xd08770, 0x4a4536,
        ],
        [
            0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0,
            0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
        ],
    ),
    theme(
        "Solarized Dark",
        [0x002b36, 0x073642, 0x93a1a1, 0x586e75],
        [
            0xb58900, 0xdc322f, 0x859900, 0x268bd2, 0x2aa198, 0xcb4b16, 0x2f3a12,
        ],
        [
            0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ],
    ),
    // The outer terminal's own colours everywhere.
    Theme {
        name: "Terminal",
        bg: Color::Reset,
        bg2: Color::Indexed(236),
        fg: Color::Reset,
        dim: Color::Indexed(244),
        ink: Color::Black,
        yellow: Color::Yellow,
        red: Color::LightRed,
        green: Color::Green,
        blue: Color::Blue,
        aqua: Color::Cyan,
        orange: Color::Indexed(208),
        wait_bg: Color::Indexed(58),
        ansi: None,
        pane: None,
    },
];

/// The theme called `name` (any case), else the first one.
pub fn by_name(name: &str) -> &'static Theme {
    THEMES
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
        .unwrap_or(&THEMES[0])
}

/// The theme `step` places after (or before) `name`, wrapping around.
pub fn step(name: &str, step: isize) -> &'static Theme {
    let i = THEMES
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name))
        .unwrap_or(0) as isize;
    let n = THEMES.len() as isize;
    &THEMES[(i + step).rem_euclid(n) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_and_cycle() {
        assert_eq!(by_name("monokai").name, "Monokai");
        assert_eq!(by_name("nope").name, "Gruvbox Dark");
        assert_eq!(step("Gruvbox Dark", -1).name, "Terminal");
        assert_eq!(step("Terminal", 1).name, "Gruvbox Dark");
        assert_eq!(
            by_name("Monokai").ansi.unwrap()[1],
            Color::Rgb(0xf9, 0x26, 0x72)
        );
    }
}
