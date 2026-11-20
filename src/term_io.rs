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
                    | ENABLE_MOUSE_INPUT))
                | ENABLE_VIRTUAL_TERMINAL_INPUT
                | ENABLE_WINDOW_INPUT;
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
                let key = events[..n as usize].iter().any(|e| {
                    e.EventType == KEY_EVENT as u16
                        && e.Event.KeyEvent.bKeyDown != 0
                        && !modifier(e.Event.KeyEvent.wVirtualKeyCode)
                });
                if !key {
                    ReadConsoleInputW(h, events.as_mut_ptr(), n, &mut n);
                    return Some(Vec::new());
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
