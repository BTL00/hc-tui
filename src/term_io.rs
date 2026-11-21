//! Raw keyboard input as VT bytes, so keys reach the program exactly as the
//! outer terminal sent them. The only keys we look at are ours (the prefix).
//!
//! The Windows console code is adapted from herdr-core's
//! `platform/windows.rs` (Apache-2.0, Copyright the herdr authors).

use std::sync::mpsc;

/// Restores the console when dropped.
pub struct RawMode(#[allow(dead_code)] imp::Saved);

impl RawMode {
    pub fn enter() -> RawMode {
        RawMode(imp::enter())
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        imp::leave(&self.0);
    }
}

/// Asks the outer terminal for mouse reports (clicks, drags, wheel; SGR form).
pub fn set_mouse(on: bool) {
    use std::io::Write;
    let seq: &[u8] = if on {
        b"\x1b[?1000h\x1b[?1002h\x1b[?1006h"
    } else {
        b"\x1b[?1006l\x1b[?1002l\x1b[?1000l"
    };
    let mut out = std::io::stdout();
    let _ = out.write_all(seq);
    let _ = out.flush();
}

/// Reads keys on a thread and hands them to `tx` until input closes.
pub fn spawn_reader<T: Send + 'static>(tx: mpsc::Sender<T>, wrap: fn(Vec<u8>) -> T) {
    std::thread::spawn(move || {
        loop {
            match imp::read(100) {
                Some(bytes) if bytes.is_empty() => continue,
                Some(bytes) => {
                    if tx.send(wrap(bytes)).is_err() {
                        break;
                    }
                }
                None => break,
            }
        }
    });
}

#[cfg(windows)]
mod imp {
    use std::mem::zeroed;
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicU32, Ordering};
    use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::Storage::FileSystem::ReadFile;
    use windows_sys::Win32::System::Console::*;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    pub type Saved = Option<(u32, u32, u32, u32)>;

    fn std_handle(which: STD_HANDLE) -> HANDLE {
        unsafe { GetStdHandle(which) }
    }

    pub fn enter() -> Saved {
        unsafe {
            let (mut input, mut output) = (0u32, 0u32);
            if GetConsoleMode(std_handle(STD_INPUT_HANDLE), &mut input) == 0
                || GetConsoleMode(std_handle(STD_OUTPUT_HANDLE), &mut output) == 0
            {
                return None;
            }
            let (cp_out, cp_in) = (GetConsoleOutputCP(), GetConsoleCP());
            let raw_in = (input
                & !(ENABLE_LINE_INPUT
                    | ENABLE_ECHO_INPUT
                    | ENABLE_PROCESSED_INPUT
                    | ENABLE_MOUSE_INPUT
                    | ENABLE_QUICK_EDIT_MODE))
                | ENABLE_VIRTUAL_TERMINAL_INPUT
                | ENABLE_WINDOW_INPUT
                // Mouse records for the classic console; Windows Terminal sends
                // VT mouse reports as text instead.
                | ENABLE_MOUSE_INPUT
                | ENABLE_EXTENDED_FLAGS;
            SetConsoleMode(std_handle(STD_INPUT_HANDLE), raw_in);
            SetConsoleMode(
                std_handle(STD_OUTPUT_HANDLE),
                output | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN,
            );
            SetConsoleOutputCP(65001);
            SetConsoleCP(65001);
            Some((input, output, cp_out, cp_in))
        }
    }

    pub fn leave(saved: &Saved) {
        if let Some((input, output, cp_out, cp_in)) = *saved {
            unsafe {
                SetConsoleMode(std_handle(STD_INPUT_HANDLE), input);
                SetConsoleMode(std_handle(STD_OUTPUT_HANDLE), output);
                SetConsoleOutputCP(cp_out);
                SetConsoleCP(cp_in);
            }
        }
    }

    /// Waits up to `ms` for keys. Empty: nothing yet. None: input closed.
    pub fn read(ms: u32) -> Option<Vec<u8>> {
        unsafe {
            let h = std_handle(STD_INPUT_HANDLE);
            if WaitForSingleObject(h, ms) != WAIT_OBJECT_0 {
                return Some(Vec::new());
            }
            let mut mode = 0u32;
            if GetConsoleMode(h, &mut mode) != 0 {
                let mut events: [INPUT_RECORD; 32] = zeroed();
                let mut n = 0u32;
                if PeekConsoleInputW(h, events.as_mut_ptr(), events.len() as u32, &mut n) == 0 {
                    return None;
                }
                // Modifier keys alone type nothing; ReadFile would block on them.
                let modifier = |vk: u16| matches!(vk, 0x10..=0x12 | 0x14 | 0x5B | 0x5C | 0x90 | 0x91 | 0xA0..=0xA5);
                let is_key = |e: &INPUT_RECORD| {
                    e.EventType == KEY_EVENT as u16
                        && e.Event.KeyEvent.bKeyDown != 0
                        && !modifier(e.Event.KeyEvent.wVirtualKeyCode)
                };
                // Records before the first key: mouse ones become VT reports.
                let lead = events[..n as usize]
                    .iter()
                    .position(is_key)
                    .unwrap_or(n as usize);
                if lead > 0 {
                    let mut out = Vec::new();
                    for e in &events[..lead] {
                        if e.EventType == MOUSE_EVENT as u16 {
                            mouse_report(&e.Event.MouseEvent, &mut out);
                        }
                    }
                    let mut used = 0u32;
                    ReadConsoleInputW(h, events.as_mut_ptr(), lead as u32, &mut used);
                    return Some(out);
                }
            }
            let mut buf = [0u8; 8192];
            let mut got = 0u32;
            if ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut got, null_mut()) == 0
                || got == 0
            {
                return None;
            }
            Some(buf[..got as usize].to_vec())
        }
    }
    static BUTTONS: AtomicU32 = AtomicU32::new(0);

    /// A console mouse record as the SGR report a VT terminal would send.
    unsafe fn mouse_report(m: &MOUSE_EVENT_RECORD, out: &mut Vec<u8>) {
        let (mut left, mut top) = (0i32, 0i32);
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { zeroed() };
        if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } != 0 {
            left = info.srWindow.Left as i32;
            top = info.srWindow.Top as i32;
        }
        let x = (m.dwMousePosition.X as i32 - left).max(0) + 1;
        let y = (m.dwMousePosition.Y as i32 - top).max(0) + 1;
        let mods = {
            let k = m.dwControlKeyState;
            let mut v = 0;
            if k & SHIFT_PRESSED != 0 {
                v |= 4;
            }
            if k & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0 {
                v |= 8;
            }
            if k & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 {
                v |= 16;
            }
            v
        };
        let mut push = |code: u32, release: bool| {
            let end = if release { 'm' } else { 'M' };
            out.extend(format!("\x1b[<{};{x};{y}{end}", code | mods).as_bytes());
        };
        // Console order: left, right, middle. VT: 0 left, 1 middle, 2 right.
        let vt = |bit: u32| match bit {
            FROM_LEFT_1ST_BUTTON_PRESSED => 0,
            RIGHTMOST_BUTTON_PRESSED => 2,
            _ => 1,
        };
        let bits = [
            FROM_LEFT_1ST_BUTTON_PRESSED,
            RIGHTMOST_BUTTON_PRESSED,
            FROM_LEFT_2ND_BUTTON_PRESSED,
        ];
        let now = m.dwButtonState & 0x7;
        if m.dwEventFlags & MOUSE_WHEELED != 0 {
            let up = (m.dwButtonState as i32 >> 16) > 0;
            push(if up { 64 } else { 65 }, false);
            return;
        }
        if m.dwEventFlags & MOUSE_HWHEELED != 0 {
            return;
        }
        let before = BUTTONS.swap(now, Ordering::Relaxed);
        if m.dwEventFlags & MOUSE_MOVED != 0 && now == before {
            if let Some(&b) = bits.iter().find(|&&b| now & b != 0) {
                push(vt(b) + 32, false);
            }
            return;
        }
        for b in bits {
            if now & b != 0 && before & b == 0 {
                push(vt(b), false);
            } else if now & b == 0 && before & b != 0 {
                push(vt(b), true);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io::Read;

    /// Whether raw mode was turned on (and so must be turned off).
    pub type Saved = bool;

    pub fn enter() -> Saved {
        ratatui::crossterm::terminal::enable_raw_mode().is_ok()
    }

    pub fn leave(saved: &Saved) {
        if *saved {
            let _ = ratatui::crossterm::terminal::disable_raw_mode();
        }
    }

    pub fn read(_ms: u32) -> Option<Vec<u8>> {
        let mut buf = [0u8; 8192];
        match std::io::stdin().read(&mut buf) {
            Ok(0) | Err(_) => None,
            Ok(n) => Some(buf[..n].to_vec()),
        }
    }
}
