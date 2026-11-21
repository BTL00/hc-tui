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
#[cfg(test)]
mod tests {
    use super::*;

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
