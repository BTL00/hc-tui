//! Settings kept between runs (ctrl+b , to change them), as JSON in
//! `%APPDATA%\hc-tui\settings.json` or `~/.config/hc-tui/settings.json`.
//! Keys this version doesn't know are kept as they were.

use serde_json::{Map, Value, json};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: String,
    pub pane_gap: u16,
    pub mouse: bool,
    pub predict: bool,
    pub sidebar_width: u16,
    /// Programs in panes (Claude Code, nvim) may set the clipboard with OSC 52;
    /// pass that on to the clipboard of the machine you are looking at.
    pub forward_clipboard: bool,
    /// Hold a pane's output between a program's begin- and end-of-redraw marks
    /// (synchronized output, DEC mode 2026) and show the redraw at once.
    pub sync_redraws: bool,
    /// How the cursor of the focused pane is shown: one of [`CURSORS`].
    pub cursor: String,
    /// alt+v / ctrl+v in a coding agent on another machine pastes the image on
    /// this computer's clipboard (sent over as a file).
    pub paste_images: bool,
    /// Give your terminal the theme's background too, so the strip around the
    /// cells (padding, scrollbar, part of a row) doesn't stand out.
    pub paint_edges: bool,
    other: Map<String, Value>,
}

/// `steady`: shown once it has stayed put for a moment, so programs redrawing
/// around it (Claude Code while thinking) don't make it jump and blink.
/// `instant`: wherever the program has it right now. `off in agents`: hidden
/// in coding-agent panes, which draw their own caret; steady elsewhere.
pub const CURSORS: [&str; 3] = ["steady", "instant", "off in agents"];

pub const GAPS: [u16; 3] = [0, 1, 2];
pub const SIDEBAR_MIN: u16 = 24;
pub const SIDEBAR_MAX: u16 = 48;

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: "Gruvbox Dark".into(),
            pane_gap: 1,
            mouse: true,
            predict: true,
            sidebar_width: 32,
            forward_clipboard: true,
            sync_redraws: true,
            cursor: CURSORS[0].into(),
            paste_images: true,
            paint_edges: true,
            other: Map::new(),
        }
    }
}

impl Settings {
    pub fn from_json(v: &Value) -> Settings {
        let d = Settings::default();
        let mut other = v.as_object().cloned().unwrap_or_default();
        let mut take = |k: &str| other.remove(k);
        Settings {
            theme: take("theme")
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or(d.theme),
            pane_gap: take("pane_gap")
                .and_then(|v| v.as_u64())
                .map(|g| (g as u16).min(2))
                .unwrap_or(d.pane_gap),
            mouse: take("mouse").and_then(|v| v.as_bool()).unwrap_or(d.mouse),
            predict: take("predict")
                .and_then(|v| v.as_bool())
                .unwrap_or(d.predict),
            sidebar_width: take("sidebar_width")
                .and_then(|v| v.as_u64())
                .map(|w| (w as u16).clamp(SIDEBAR_MIN, SIDEBAR_MAX))
                .unwrap_or(d.sidebar_width),
            forward_clipboard: take("forward_clipboard")
                .and_then(|v| v.as_bool())
                .unwrap_or(d.forward_clipboard),
            sync_redraws: take("sync_redraws")
                .and_then(|v| v.as_bool())
                .unwrap_or(d.sync_redraws),
            cursor: take("cursor")
                .and_then(|v| v.as_str().map(str::to_string))
                .filter(|c| CURSORS.contains(&c.as_str()))
                .unwrap_or(d.cursor),
            paste_images: take("paste_images")
                .and_then(|v| v.as_bool())
                .unwrap_or(d.paste_images),
            paint_edges: take("paint_edges")
                .and_then(|v| v.as_bool())
                .unwrap_or(d.paint_edges),
            other,
        }
    }

    pub fn to_json(&self) -> Value {
        let mut m = self.other.clone();
        m.insert("theme".into(), json!(self.theme));
        m.insert("pane_gap".into(), json!(self.pane_gap));
        m.insert("mouse".into(), json!(self.mouse));
        m.insert("predict".into(), json!(self.predict));
        m.insert("sidebar_width".into(), json!(self.sidebar_width));
        m.insert("forward_clipboard".into(), json!(self.forward_clipboard));
        m.insert("sync_redraws".into(), json!(self.sync_redraws));
        m.insert("cursor".into(), json!(self.cursor));
        m.insert("paste_images".into(), json!(self.paste_images));
        m.insert("paint_edges".into(), json!(self.paint_edges));
        Value::Object(m)
    }

    /// The saved settings; defaults (and why) when there are none or they don't parse.
    pub fn load() -> (Settings, Option<String>) {
        let Some(path) = path() else {
            return (Settings::default(), None);
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(v) => (Settings::from_json(&v), None),
                Err(e) => (
                    Settings::default(),
                    Some(format!("{}: {e}; using defaults", path.display())),
                ),
            },
            Err(_) => (Settings::default(), None),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = path().ok_or("no settings folder")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(&self.to_json()).map_err(|e| e.to_string())?;
        std::fs::write(&path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("hc-tui").join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_unknown_keys() {
        let v = json!({"theme": "Monokai", "pane_gap": 9, "future": [1, 2], "sidebar_width": 5});
        let s = Settings::from_json(&v);
        assert_eq!(s.theme, "Monokai");
        assert_eq!(s.pane_gap, 2);
        assert_eq!(s.sidebar_width, SIDEBAR_MIN);
        assert!(s.mouse);
        let back = s.to_json();
        assert_eq!(back["future"], json!([1, 2]));
        assert_eq!(Settings::from_json(&back), s);
    }
}
