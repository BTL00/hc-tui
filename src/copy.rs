//! Exact copies of what Claude wrote. Claude Code keeps every reply verbatim
//! in `~/.claude/projects/<folder slug>/<session>.jsonl`; code blocks taken
//! from there have none of the line breaks the screen adds when it wraps.
//! The clipboard is set with OSC 52 (works in Windows Terminal and over SSH)
//! and, on Windows, natively as well.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBlock {
    pub lang: String,
    pub text: String,
}

/// Claude Code's folder name for a project: every character that is not a
/// letter or digit becomes `-` (`C:\Users\me\app` → `C--Users-me-app`).
pub fn slug(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Whether a folder path is a Windows one (`C:\...`, `\\server\...`).
pub fn is_windows_path(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() >= 2 && b[1] == b':') || p.starts_with("\\\\")
}

/// What Claude wrote in each turn (everything between two of your prompts),
/// oldest first. Tool calls and sub-agent chatter are left out.
pub fn replies(jsonl: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = false;
    for line in jsonl.lines() {
        let user = line.contains("\"type\":\"user\"");
        if !user && !line.contains("\"assistant\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["isSidechain"] == true {
            continue;
        }
        let content = &v["message"]["content"];
        if v["type"] == "user" {
            // A prompt you typed (not a tool result) starts a new turn.
            let prompt = content.is_string()
                || content
                    .as_array()
                    .is_some_and(|a| a.iter().all(|c| c["type"] != "tool_result"));
            if prompt {
                open = false;
            }
            continue;
        }
        if v["type"] != "assistant" {
            continue;
        }
        let text: Vec<&str> = content
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["type"] == "text")
            .filter_map(|c| c["text"].as_str())
            .collect();
        if text.is_empty() {
            continue;
        }
        let text = text.join("\n\n");
        match out.last_mut() {
            Some(body) if open => {
                body.push_str("\n\n");
                body.push_str(&text);
            }
            _ => {
                out.push(text);
                open = true;
            }
        }
    }
    out
}

/// Fenced code blocks (``` or ~~~, any indent) in Markdown, in order. A block
/// closes at a fence of the same character at least as long; an unclosed
/// one runs to the end. The opening fence's indent is taken off each line.
pub fn code_blocks(markdown: &str) -> Vec<CodeBlock> {
    let mut out = Vec::new();
    let mut open: Option<(char, usize, usize, String, Vec<&str>)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let fence = |c: char| trimmed.chars().take_while(|&x| x == c).count();
        match &mut open {
            None => {
                for c in ['`', '~'] {
                    let n = fence(c);
                    if n >= 3 {
                        let info = trimmed[n..].trim();
                        if c == '`' && info.contains('`') {
                            continue;
                        }
                        let lang = info.split_whitespace().next().unwrap_or("").to_string();
                        open = Some((c, n, indent, lang, Vec::new()));
                        break;
                    }
                }
            }
            Some((c, n, ind, lang, body)) => {
                let m = fence(*c);
                if m >= *n && trimmed[m..].trim().is_empty() {
                    out.push(CodeBlock {
                        lang: std::mem::take(lang),
                        text: body.join("\n"),
                    });
                    open = None;
                } else {
                    let cut = line
                        .char_indices()
                        .take_while(|(i, ch)| *i < *ind && ch.is_whitespace())
                        .count();
                    body.push(&line[cut..]);
                }
            }
        }
    }
    if let Some((_, _, _, lang, body)) = open {
        out.push(CodeBlock {
            lang,
            text: body.join("\n"),
        });
    }
    out
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn b64decode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// The OSC 52 sequence that asks the terminal to put `text` on the clipboard.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", b64encode(text.as_bytes()))
}

/// Puts `text` on the clipboard of the terminal you are looking at (OSC 52)
/// and, on Windows, on this machine's clipboard too. Err only when neither worked.
pub fn to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut out = std::io::stdout();
    let osc = out
        .write_all(osc52(text).as_bytes())
        .and_then(|_| out.flush())
        .map_err(|e| e.to_string());
    #[cfg(windows)]
    let native = win::set(text);
    #[cfg(not(windows))]
    let native: Result<(), String> = Err(String::new());
    match (osc, native) {
        (Err(a), Err(b)) => Err(format!("{a}; {b}")),
        _ => Ok(()),
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };

    const CF_UNICODETEXT: u32 = 13;

    pub fn set(text: &str) -> Result<(), String> {
        let wide: Vec<u16> = text
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            // Another program may hold it for a moment.
            let mut tries = 0;
            while OpenClipboard(std::ptr::null_mut()) == 0 {
                tries += 1;
                if tries == 20 {
                    return Err("the clipboard is busy".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            EmptyClipboard();
            let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
            let ok = !mem.is_null() && {
                let p = GlobalLock(mem) as *mut u16;
                if p.is_null() {
                    false
                } else {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                    GlobalUnlock(mem);
                    !SetClipboardData(CF_UNICODETEXT, mem).is_null()
                }
            };
            CloseClipboard();
            if ok {
                Ok(())
            } else {
                Err("could not set the clipboard".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_like_claude_code() {
        assert_eq!(slug("C:\\Users\\bomba\\hc-tui"), "C--Users-bomba-hc-tui");
        assert_eq!(slug("/home/me/my app"), "-home-me-my-app");
        assert!(is_windows_path("C:\\x"));
        assert!(!is_windows_path("/home/me"));
    }

    #[test]
    fn finds_fenced_blocks() {
        let md = "Text\n```rust\nfn a() {\n    1\n}\n```\nmore\n   ~~~\n   indented\n     deeper\n   ~~~\n````md\n```\ninner\n```\n````\n```sh\nopen";
        let b = code_blocks(md);
        assert_eq!(b.len(), 4);
        assert_eq!(b[0].lang, "rust");
        assert_eq!(b[0].text, "fn a() {\n    1\n}");
        assert_eq!(b[1].text, "indented\n  deeper");
        assert_eq!(b[2].lang, "md");
        assert_eq!(b[2].text, "```\ninner\n```");
        assert_eq!(b[3].text, "open");
        assert!(code_blocks("inline ```code``` only").is_empty());
    }

    #[test]
    fn reads_replies_from_a_transcript() {
        let jsonl = [
            r#"{"type":"user","message":{"content":"hi"}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"one"}]}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","name":"x"}]}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"two"}]}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"id":"s","content":[{"type":"text","text":"agent"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#,
            r#"{"type":"assistant","message":{"id":"m3","content":[{"type":"text","text":"same turn"}]}}"#,
            r#"{"type":"user","message":{"content":"next prompt"}}"#,
            r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"text","text":"three"}]}}"#,
            "not json",
        ]
        .join("\n");
        assert_eq!(replies(&jsonl), vec!["one\n\ntwo\n\nsame turn", "three"]);
    }

    #[test]
    fn base64_round_trip() {
        for s in ["", "f", "fo", "foo", "foob", "zażółć ```"] {
            assert_eq!(b64decode(&b64encode(s.as_bytes())), s.as_bytes());
        }
        assert_eq!(b64encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }
}
