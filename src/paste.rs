//! Pasting an image from the clipboard of the computer you are at into a
//! program on another machine (Claude Code reads images from its own
//! machine's clipboard, which doesn't have yours). The image is read here, the
//! core puts it in a file beside the pane's terminal (`terminal.put`), and its
//! path is pasted; Claude Code attaches a pasted image path as an image.

/// The core takes at most this much per file.
pub const IMAGE_MAX: usize = 8 << 20;

/// The image on this computer's clipboard (a copied picture, or a copied image
/// file), with its file extension. None when there is no image.
pub fn clipboard_image() -> Result<Option<(Vec<u8>, &'static str)>, String> {
    // For testing: a file stands in for the clipboard.
    let bytes = match std::env::var_os("HC_TUI_PASTE_FILE") {
        Some(path) => Some(std::fs::read(&path).map_err(|e| e.to_string())?),
        None => read_clipboard()?,
    };
    Ok(bytes.and_then(|b| kind(&b).map(|ext| (b, ext))))
}

/// The image format by its first bytes, as a file extension.
pub fn kind(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG") {
        Some("png")
    } else if b.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if b.starts_with(b"GIF8") {
        Some("gif")
    } else if b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

#[cfg(windows)]
fn read_clipboard() -> Result<Option<Vec<u8>>, String> {
    win::read()
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    const CF_DIB: u32 = 8;
    const CF_HDROP: u32 = 15;

    /// A picture as PNG (as copied, or converted), else a copied image file.
    pub fn read() -> Result<Option<Vec<u8>>, String> {
        let (png, dib, files) = {
            let _open = Open::new()?;
            let name: Vec<u16> = "PNG\0".encode_utf16().collect();
            let png = data(unsafe { RegisterClipboardFormatW(name.as_ptr()) });
            let dib = if png.is_none() { data(CF_DIB) } else { None };
            (png, dib, data(CF_HDROP))
        };
        if png.is_some() {
            return Ok(png);
        }
        if let Some(dib) = dib {
            return dib_to_png(&dib)
                .map(Some)
                .ok_or_else(|| "this picture's format isn't supported".into());
        }
        for path in files.map(|f| dropped_files(&f)).unwrap_or_default() {
            let path = std::path::PathBuf::from(path);
            let image = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                ["png", "jpg", "jpeg", "gif", "webp"].contains(&e.to_ascii_lowercase().as_str())
            });
            if !image {
                continue;
            }
            let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if size > super::IMAGE_MAX as u64 {
                return Err(format!("image too big ({} MB, at most 8)", size >> 20));
            }
            return std::fs::read(&path).map(Some).map_err(|e| e.to_string());
        }
        Ok(None)
    }

    /// The clipboard, open until dropped.
    struct Open;

    impl Open {
        fn new() -> Result<Open, String> {
            // Another program may hold it for a moment.
            for _ in 0..20 {
                if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                    return Ok(Open);
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            Err("the clipboard is busy".into())
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }

    /// A copy of what the (open) clipboard holds in `format`.
    fn data(format: u32) -> Option<Vec<u8>> {
        unsafe {
            let mem = GetClipboardData(format);
            if mem.is_null() {
                return None;
            }
            let p = GlobalLock(mem) as *const u8;
            if p.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(p, GlobalSize(mem)).to_vec();
            GlobalUnlock(mem);
            Some(bytes)
        }
    }

    fn u32_at(b: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
    }

    /// The paths in a CF_HDROP block (DROPFILES, then NUL-ended UTF-16 strings).
    pub fn dropped_files(b: &[u8]) -> Vec<String> {
        let (Some(at), Some(wide)) = (u32_at(b, 0), u32_at(b, 16)) else {
            return Vec::new();
        };
        if wide == 0 {
            return Vec::new();
        }
        let units: Vec<u16> = b
            .get(at as usize..)
            .unwrap_or_default()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        units
            .split(|&u| u == 0)
            .take_while(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }

    /// A device-independent bitmap (CF_DIB, 24 or 32 bits) as PNG. The fourth
    /// byte of 32-bit pixels is often unset rather than alpha, so it is dropped.
    pub fn dib_to_png(b: &[u8]) -> Option<Vec<u8>> {
        let header = u32_at(b, 0)? as usize;
        let width = u32_at(b, 4)? as i32;
        let height = u32_at(b, 8)? as i32;
        let bits = usize::from(u16::from_le_bytes([*b.get(14)?, *b.get(15)?]));
        let compression = u32_at(b, 16)?;
        let colours = u32_at(b, 32)? as usize;
        if header < 40 || width <= 0 || height == 0 {
            return None;
        }
        let (w, h) = (width as usize, height.unsigned_abs() as usize);
        let mut at = header;
        // Where red, green and blue are in a pixel.
        let order = match (bits, compression) {
            (24 | 32, 0) => [2, 1, 0],
            (32, 3) => {
                if header == 40 {
                    at += 12;
                }
                let mut order = [0; 3];
                for (i, o) in order.iter_mut().enumerate() {
                    *o = match u32_at(b, 40 + 4 * i)? {
                        0xff => 0,
                        0xff00 => 1,
                        0xff_0000 => 2,
                        0xff00_0000 => 3,
                        _ => return None,
                    };
                }
                order
            }
            _ => return None,
        };
        at = at.checked_add(colours.checked_mul(4)?)?;
        let stride = w.checked_mul(bits)?.checked_add(31)? / 32 * 4;
        let pixels = b.get(at..at.checked_add(stride.checked_mul(h)?)?)?;
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            // Rows go bottom up unless the height is negative.
            let row = if height > 0 { h - 1 - y } else { y };
            let row = &pixels[row * stride..][..w * bits / 8];
            for p in row.chunks_exact(bits / 8) {
                rgb.extend(order.map(|i| p[i]));
            }
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgb).ok()?;
        writer.finish().ok()?;
        Some(out)
    }
}

#[cfg(target_os = "macos")]
fn read_clipboard() -> Result<Option<Vec<u8>>, String> {
    // Prints «data PNGf89504E47…», or fails when there is no picture.
    let out = std::process::Command::new("osascript")
        .args(["-e", "the clipboard as «class PNGf»"])
        .output()
        .map_err(|e| format!("osascript: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(hex) = text
        .split_once("PNGf")
        .map(|(_, rest)| rest.trim_end().trim_end_matches('»'))
    else {
        return Ok(None);
    };
    let bytes: Option<Vec<u8>> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
        .collect();
    Ok(bytes.filter(|b| !b.is_empty()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_clipboard() -> Result<Option<Vec<u8>>, String> {
    let tries: [&[&str]; 2] = [
        &["wl-paste", "--no-newline", "--type", "image/png"],
        &["xclip", "-selection", "clipboard", "-t", "image/png", "-o"],
    ];
    for argv in tries {
        if let Ok(out) = std::process::Command::new(argv[0])
            .args(&argv[1..])
            .output()
            && out.status.success()
            && !out.stdout.is_empty()
        {
            return Ok(Some(out.stdout));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_image_formats() {
        assert_eq!(kind(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(kind(b"\xff\xd8\xff\xe0"), Some("jpg"));
        assert_eq!(kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(kind(b"hello"), None);
    }

    /// A BITMAPINFOHEADER.
    #[cfg(windows)]
    fn dib(w: i32, h: i32, bits: u16, compression: u32) -> Vec<u8> {
        let mut b = vec![0u8; 40];
        b[0] = 40;
        b[4..8].copy_from_slice(&w.to_le_bytes());
        b[8..12].copy_from_slice(&h.to_le_bytes());
        b[14..16].copy_from_slice(&bits.to_le_bytes());
        b[16..20].copy_from_slice(&compression.to_le_bytes());
        b
    }

    #[cfg(windows)]
    fn pixels(png_bytes: &[u8]) -> Vec<u8> {
        let mut r = png::Decoder::new(std::io::Cursor::new(png_bytes))
            .read_info()
            .unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let n = r.next_frame(&mut buf).unwrap().buffer_size();
        buf.truncate(n);
        buf
    }

    #[cfg(windows)]
    #[test]
    fn bitmaps_become_png() {
        // 24 bits, one pixel a row, bottom up, rows padded to 4 bytes.
        let mut b = dib(1, 2, 24, 0);
        b.extend([1, 2, 3, 0, 4, 5, 6, 0]);
        let out = win::dib_to_png(&b).unwrap();
        assert_eq!(kind(&out), Some("png"));
        assert_eq!(pixels(&out), [6, 5, 4, 3, 2, 1]);
        // 32 bits, top down, red/green/blue masks after the header.
        let mut b = dib(2, -1, 32, 3);
        for mask in [0xff_0000u32, 0xff00, 0xff] {
            b.extend(mask.to_le_bytes());
        }
        b.extend([1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(pixels(&win::dib_to_png(&b).unwrap()), [3, 2, 1, 6, 5, 4]);
        // Too short, or sizes that don't add up: no picture, no panic.
        assert!(win::dib_to_png(&b[..b.len() - 1]).is_none());
        assert!(win::dib_to_png(&dib(i32::MAX, i32::MIN, 32, 0)).is_none());
        assert!(win::dib_to_png(b"short").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn reads_dropped_file_names() {
        let mut b = vec![0u8; 20];
        b[0] = 20;
        b[16] = 1;
        for u in "C:\\a ż.png\0D:\\b.txt\0\0".encode_utf16() {
            b.extend(u.to_le_bytes());
        }
        assert_eq!(win::dropped_files(&b), ["C:\\a ż.png", "D:\\b.txt"]);
        assert!(win::dropped_files(&b[..10]).is_empty());
    }

    /// `cargo test -- --ignored --nocapture`: only says what the clipboard holds.
    #[test]
    #[ignore = "reads the real clipboard"]
    fn real_clipboard() {
        match clipboard_image() {
            Ok(Some((b, ext))) => println!("clipboard: {ext}, {} KB", b.len() / 1024),
            Ok(None) => println!("clipboard: no image"),
            Err(e) => println!("clipboard: error: {e}"),
        }
    }
}
