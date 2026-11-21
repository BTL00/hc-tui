//! Keys as hc-tui reads them in its own modes, and the herdr-style keymap.
//!
//! Default bindings follow herdr's defaults (prefix `ctrl+b`), see
//! <https://github.com/herdrdev/herdr/blob/master/docs/next/website/src/content/docs/keyboard.mdx>.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Unknown,
}

/// The first key in `input` and how many bytes it used.
pub fn parse_key(input: &[u8]) -> (Key, usize) {
    let Some(&b0) = input.first() else {
        return (Key::Unknown, 0);
    };
    match b0 {
        0x1b => parse_escape(input),
        b'\r' | b'\n' => (Key::Enter, 1),
        b'\t' => (Key::Tab, 1),
        0x7f | 0x08 => (Key::Backspace, 1),
        0x00 => (Key::Ctrl(' '), 1),
        0x01..=0x1a => (Key::Ctrl((b'a' + b0 - 1) as char), 1),
        0x1c => (Key::Ctrl('\\'), 1),
        0x1d => (Key::Ctrl(']'), 1),
        0x1f => (Key::Ctrl('/'), 1),
        _ => {
            let len = utf8_len(b0).min(input.len());
            match std::str::from_utf8(&input[..len])
                .ok()
                .and_then(|s| s.chars().next())
            {
                Some(c) => (Key::Char(c), len),
                None => (Key::Unknown, 1),
            }
        }
    }
}

fn utf8_len(b0: u8) -> usize {
    match b0 {
        0xf0..=0xf7 => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

fn parse_escape(input: &[u8]) -> (Key, usize) {
    match input.get(1) {
        None => (Key::Esc, 1),
        Some(b'[') => {
            let rest = &input[2..];
            let Some(end) = rest.iter().position(|b| (0x40..=0x7e).contains(b)) else {
                return (Key::Unknown, input.len());
            };
            let params = std::str::from_utf8(&rest[..end]).unwrap_or("");
            let used = 2 + end + 1;
            let key = match (rest[end], params) {
                (b'A', _) => Key::Up,
                (b'B', _) => Key::Down,
                (b'C', _) => Key::Right,
                (b'D', _) => Key::Left,
                (b'H', _) => Key::Home,
                (b'F', _) => Key::End,
                (b'Z', _) => Key::BackTab,
                (b'~', "1" | "7") => Key::Home,
                (b'~', "4" | "8") => Key::End,
                (b'~', "3") => Key::Delete,
                (b'~', "5") => Key::PageUp,
                (b'~', "6") => Key::PageDown,
                // kitty / modifyOtherKeys forms of plain keys we care about
                (b'u', p) => kitty_key(p),
                _ => Key::Unknown,
            };
            (key, used)
        }
        Some(b'O') => match input.get(2) {
            Some(b'A') => (Key::Up, 3),
            Some(b'B') => (Key::Down, 3),
            Some(b'C') => (Key::Right, 3),
            Some(b'D') => (Key::Left, 3),
            Some(b'H') => (Key::Home, 3),
            Some(b'F') => (Key::End, 3),
            _ => (Key::Unknown, input.len().min(3)),
        },
        // alt+key: treat as the key itself is wrong, report Unknown but consume.
        Some(_) => (Key::Unknown, 2),
    }
}

fn kitty_key(params: &str) -> Key {
    let mut fields = params.split(';');
    let code: u32 = fields
        .next()
        .and_then(|c| c.split(':').next())
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let mods: u32 = fields
        .next()
        .and_then(|m| m.split(':').next())
        .and_then(|m| m.parse().ok())
        .unwrap_or(1);
    let ctrl = mods >= 1 && (mods - 1) & 4 != 0;
    match (code, ctrl) {
        (27, _) => Key::Esc,
        (13, _) => Key::Enter,
        (c, true) if (97..=122).contains(&c) => Key::Ctrl(c as u8 as char),
        (c, false) => char::from_u32(c).map(Key::Char).unwrap_or(Key::Unknown),
        _ => Key::Unknown,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Down,
    Up,
    Right,
}

/// Actions reachable after the prefix, named after herdr's config keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewTab,
    SplitVertical,
    SplitHorizontal,
    Focus(Dir),
    NavigateWorkspace,
    Detach,
    Zoom,
    ClosePane,
    Swap(Dir),
    ResizeMode,
    CopyMode,
    NextTab,
    PreviousTab,
    SwitchTab(usize),
    RenameTab,
    CloseTab,
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    Goto,
    ToggleSidebar,
    Help,
    TogglePredict,
    NextWaiting,
    ToggleMouse,
    SendPrefix,
    Cancel,
}

/// What a key does right after the prefix (herdr defaults).
pub fn prefix_action(key: Key) -> Option<Action> {
    use Action::*;
    Some(match key {
        Key::Char('c') => NewTab,
        Key::Char('v') => SplitVertical,
        Key::Char('-') => SplitHorizontal,
        Key::Char('h') | Key::Left => Focus(Dir::Left),
        Key::Char('j') | Key::Down => Focus(Dir::Down),
        Key::Char('k') | Key::Up => Focus(Dir::Up),
        Key::Char('l') | Key::Right => Focus(Dir::Right),
        Key::Char('w') => NavigateWorkspace,
        Key::Char('q') => Detach,
        Key::Char('z') => Zoom,
        Key::Char('x') => ClosePane,
        Key::Char('H') => Swap(Dir::Left),
        Key::Char('J') => Swap(Dir::Down),
        Key::Char('K') => Swap(Dir::Up),
        Key::Char('L') => Swap(Dir::Right),
        Key::Char('r') => ResizeMode,
        Key::Char('[') => CopyMode,
        Key::Char('n') => NextTab,
        Key::Char('p') => PreviousTab,
        Key::Char(d @ '1'..='9') => SwitchTab(d as usize - '1' as usize),
        Key::Char('T') => RenameTab,
        Key::Char('X') => CloseTab,
        Key::Char('N') => NewWorkspace,
        Key::Char('W') => RenameWorkspace,
        Key::Char('D') => CloseWorkspace,
        Key::Char('g') => Goto,
        Key::Char('b') => ToggleSidebar,
        Key::Char('?') => Help,
        Key::Char('e') => TogglePredict,
        Key::Char('a') => NextWaiting,
        Key::Char('m') => ToggleMouse,
        Key::Ctrl('b') => SendPrefix,
        Key::Esc | Key::Ctrl('[') | Key::Ctrl('c') => Cancel,
        _ => return None,
    })
}

/// Help rows: (keys, what it does).
pub const HELP: &[(&str, &str)] = &[
    ("prefix", "ctrl+b, then the key (herdr defaults)"),
    ("prefix c", "new tab (shell) in this workspace"),
    ("prefix v / -", "split right / down"),
    ("prefix h/j/k/l", "move between panes"),
    ("prefix H/J/K/L", "swap panes"),
    ("prefix z", "zoom the focused pane"),
    ("prefix x", "close pane (ends its program)"),
    ("prefix r", "resize mode: h/j/k/l, esc"),
    ("prefix n / p", "next / previous tab"),
    ("prefix 1..9", "jump to tab 1-9"),
    ("prefix T / X", "rename / close tab"),
    ("prefix N / W / D", "new / rename / close workspace"),
    ("prefix w", "workspace navigation (sidebar)"),
    ("prefix g", "goto picker  (/ search, b w i d a filter)"),
    ("prefix a", "jump to the next agent waiting for you"),
    ("prefix b", "toggle sidebar"),
    ("prefix e", "toggle predictive echo"),
    ("prefix m", "toggle the mouse (off: select text natively)"),
    (
        "mouse",
        "click tab/pane, wheel scrolls history, drag dividers",
    ),
    ("prefix q", "detach (everything keeps running)"),
    ("prefix ctrl+b", "send ctrl+b to the program"),
    ("prefix ?", "this help"),
    ("", ""),
    (
        "shared",
        "workspaces/tabs/splits are the Herdr app's layout",
    ),
    ("not yet", "copy mode ([)"),
];

/// A mouse report from the outer terminal (SGR form), 0-based cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    /// xterm button code: low bits button (0 left, 1 middle, 2 right),
    /// +4 shift, +8 alt, +16 ctrl, +32 motion, +64 wheel (64 up, 65 down).
    pub code: u16,
    pub x: u16,
    pub y: u16,
    pub release: bool,
}

impl Mouse {
    pub fn wheel(&self) -> Option<bool> {
        (self.code & 64 != 0).then_some(self.code & 1 == 0)
    }
    pub fn motion(&self) -> bool {
        self.code & 32 != 0
    }
    pub fn left_press(&self) -> bool {
        !self.release && self.code & (64 | 32 | 3) == 0
    }
}

pub enum MouseParse {
    Done(Mouse, usize),
    /// The start of a report; the rest is still on its way.
    Partial,
    NotMouse,
}

/// `ESC [ < code ; x ; y (M|m)` at the start of `input`.
pub fn parse_mouse(input: &[u8]) -> MouseParse {
    const START: &[u8] = b"\x1b[<";
    if input.len() < START.len() {
        return if START.starts_with(input) && !input.is_empty() && input.len() > 1 {
            MouseParse::Partial
        } else {
            MouseParse::NotMouse
        };
    }
    if !input.starts_with(START) {
        return MouseParse::NotMouse;
    }
    let rest = &input[3..];
    let Some(end) = rest
        .iter()
        .position(|b| !(b.is_ascii_digit() || *b == b';'))
    else {
        return if rest.len() < 16 {
            MouseParse::Partial
        } else {
            MouseParse::NotMouse
        };
    };
    if !matches!(rest[end], b'M' | b'm') {
        return MouseParse::NotMouse;
    }
    let nums: Vec<u16> = std::str::from_utf8(&rest[..end])
        .unwrap_or("")
        .split(';')
        .filter_map(|n| n.parse().ok())
        .collect();
    let [code, x, y] = nums[..] else {
        return MouseParse::NotMouse;
    };
    MouseParse::Done(
        Mouse {
            code,
            x: x.saturating_sub(1),
            y: y.saturating_sub(1),
            release: rest[end] == b'm',
        },
        3 + end + 1,
    )
}

/// The report a program inside a pane asked for (its mode and encoding),
/// at cell (`col`, `row`) of that pane. None: it doesn't want this event.
pub fn encode_mouse(
    m: &Mouse,
    col: u16,
    row: u16,
    mode: vt100::MouseProtocolMode,
    encoding: vt100::MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    use vt100::{MouseProtocolEncoding as E, MouseProtocolMode as P};
    let wanted = match mode {
        P::None => false,
        P::Press => !m.release && !m.motion(),
        P::PressRelease => !m.motion(),
        _ => true,
    };
    if !wanted {
        return None;
    }
    if encoding == E::Sgr {
        let end = if m.release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{};{};{}{end}", m.code, col + 1, row + 1).into_bytes());
    }
    // Older forms: release is button 3, everything offset by 32.
    let code = if m.release { (m.code & !3) | 3 } else { m.code };
    let mut out = b"\x1b[M".to_vec();
    for v in [code, col + 1, row + 1] {
        let v = u32::from(v) + 32;
        match encoding {
            E::Utf8 => {
                let c = char::from_u32(v.min(2047)).unwrap_or(' ');
                let mut b = [0u8; 4];
                out.extend(c.encode_utf8(&mut b).as_bytes());
            }
            _ => out.push(v.min(255) as u8),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mouse_reports() {
        let MouseParse::Done(m, used) = parse_mouse(b"\x1b[<0;12;5Mrest") else {
            panic!("not parsed");
        };
        assert_eq!(used, 10);
        assert_eq!((m.x, m.y, m.release), (11, 4, false));
        assert!(m.left_press());
        let MouseParse::Done(w, _) = parse_mouse(b"\x1b[<65;1;1M") else {
            panic!("wheel");
        };
        assert_eq!(w.wheel(), Some(false));
        assert!(matches!(parse_mouse(b"\x1b[<0;1"), MouseParse::Partial));
        assert!(matches!(parse_mouse(b"\x1b["), MouseParse::Partial));
        assert!(matches!(parse_mouse(b"\x1b[A"), MouseParse::NotMouse));
        assert!(matches!(parse_mouse(b"x"), MouseParse::NotMouse));
    }

    #[test]
    fn encodes_for_programs() {
        use vt100::{MouseProtocolEncoding as E, MouseProtocolMode as P};
        let m = Mouse {
            code: 0,
            x: 50,
            y: 9,
            release: true,
        };
        assert_eq!(
            encode_mouse(&m, 3, 4, P::PressRelease, E::Sgr).unwrap(),
            b"\x1b[<0;4;5m"
        );
        assert_eq!(
            encode_mouse(&m, 3, 4, P::PressRelease, E::Default).unwrap(),
            b"\x1b[M\x23\x24\x25"
        );
        assert!(encode_mouse(&m, 3, 4, P::Press, E::Sgr).is_none());
        assert!(encode_mouse(&m, 3, 4, P::None, E::Sgr).is_none());
    }

    #[test]
    fn parses_common_keys() {
        assert_eq!(parse_key(b"\x02"), (Key::Ctrl('b'), 1));
        assert_eq!(parse_key(b"\x1b[A"), (Key::Up, 3));
        assert_eq!(parse_key(b"\x1b"), (Key::Esc, 1));
        assert_eq!(parse_key(b"N"), (Key::Char('N'), 1));
        assert_eq!(parse_key("ż".as_bytes()), (Key::Char('ż'), 2));
        assert_eq!(parse_key(b"\x1b[98;5u"), (Key::Ctrl('b'), 7));
        assert_eq!(parse_key(b"\r"), (Key::Enter, 1));
    }

    #[test]
    fn herdr_defaults() {
        assert_eq!(prefix_action(Key::Char('c')), Some(Action::NewTab));
        assert_eq!(prefix_action(Key::Char('3')), Some(Action::SwitchTab(2)));
        assert_eq!(prefix_action(Key::Char('N')), Some(Action::NewWorkspace));
        assert_eq!(prefix_action(Key::Ctrl('b')), Some(Action::SendPrefix));
    }
}
