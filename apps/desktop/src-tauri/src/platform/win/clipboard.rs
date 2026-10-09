//! The Windows clipboard: its change counter, and reading and writing text, pictures and files.

use std::thread;
use std::time::Duration;

use tauri::WebviewWindow;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::core::{PCWSTR, w};

use crate::clipboard::{Clip, MAX_PIXELS, Picture, decode_png};

/// Standard formats; `windows` keeps them in a large module of its own.
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
/// "Preferred DropEffect" value that makes a paste copy the files rather than move them.
const DROPEFFECT_COPY: u32 = 1;
const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const BI_ALPHABITFIELDS: u32 = 6;

/// A number that changes whenever anything puts new data on the clipboard.
pub fn clipboard_sequence() -> u32 {
    // SAFETY: no arguments, no preconditions.
    unsafe { GetClipboardSequenceNumber() }
}

/// What is on the clipboard: files, or else text, or else a picture. Files come first, since
/// some apps add their paths as text as well. `Ok(None)` when it holds neither, or the
/// app that copied it asked clipboard managers to leave it alone (password managers do).
/// `Err` while another app keeps the clipboard open, so the caller tries again later.
pub fn read_clipboard() -> Result<Option<Clip>, ()> {
    let _open = Opened::new(None).ok_or(())?;
    if is_private() {
        return Ok(None);
    }
    if let Some(paths) = data(CF_HDROP).and_then(|list| parse_drop_files(&list)) {
        return Ok(Some(Clip::Files(paths)));
    }
    if let Some(text) = data(CF_UNICODETEXT) {
        let units: Vec<u16> = text.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
        let end = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
        return Ok(Some(Clip::Text(String::from_utf16_lossy(&units[..end]))));
    }
    // Browsers and Office also offer PNG, which keeps transparency.
    let png = format(w!("PNG")).and_then(data).and_then(|bytes| decode_png(&bytes));
    Ok(png.or_else(|| data(CF_DIB).and_then(|dib| dib_to_picture(&dib))).map(Clip::Picture))
}

pub fn write_clipboard_text(owner: &WebviewWindow, text: &str) -> Result<(), String> {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    let bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    let _open = Opened::new(Some(owner.hwnd().map_err(|e| e.to_string())?)).ok_or(BUSY)?;
    // SAFETY: the clipboard is open for this window.
    unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
    put(CF_UNICODETEXT, &bytes)
}

/// Puts a picture on the clipboard as a bitmap, which every app reads, and as the PNG it is
/// kept in, which keeps transparency for the apps that read that.
pub fn write_clipboard_picture(owner: &WebviewWindow, picture: &Picture, png: &[u8]) -> Result<(), String> {
    let dib = picture_to_dib(picture);
    let _open = Opened::new(Some(owner.hwnd().map_err(|e| e.to_string())?)).ok_or(BUSY)?;
    // SAFETY: the clipboard is open for this window.
    unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
    put(CF_DIB, &dib)?;
    if let Some(format) = format(w!("PNG")) {
        put(format, png)?;
    }
    Ok(())
}

/// Puts files on the clipboard the way Explorer does, so pasting copies them.
pub fn write_clipboard_files(owner: &WebviewWindow, paths: &[String]) -> Result<(), String> {
    let list = drop_files(paths);
    let _open = Opened::new(Some(owner.hwnd().map_err(|e| e.to_string())?)).ok_or(BUSY)?;
    // SAFETY: the clipboard is open for this window.
    unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
    put(CF_HDROP, &list)?;
    if let Some(format) = format(w!("Preferred DropEffect")) {
        put(format, &DROPEFFECT_COPY.to_le_bytes())?;
    }
    Ok(())
}

const BUSY: &str = "another app is using the clipboard";

/// The clipboard, open until this is dropped. Another app may hold it for a moment, so opening
/// is retried a few times.
struct Opened;

impl Opened {
    /// Writing needs an owner window: with none, Windows refuses the new data.
    fn new(owner: Option<HWND>) -> Option<Self> {
        for _ in 0..5 {
            // SAFETY: `owner` is a live window or none.
            if unsafe { OpenClipboard(owner) }.is_ok() {
                return Some(Self);
            }
            thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by `Opened::new`.
        let _ = unsafe { CloseClipboard() };
    }
}

fn format(name: PCWSTR) -> Option<u32> {
    // SAFETY: `name` is a static, null-terminated string.
    Some(unsafe { RegisterClipboardFormatW(name) }).filter(|&format| format != 0)
}

/// Formats apps add to keep what they copy out of clipboard history: the ones Windows' own
/// history honours, and the older convention for clipboard viewers.
fn is_private() -> bool {
    let present = |name| format(name).is_some_and(|format| unsafe { IsClipboardFormatAvailable(format) }.is_ok());
    if present(w!("ExcludeClipboardContentFromMonitorProcessing")) || present(w!("Clipboard Viewer Ignore")) {
        return true;
    }
    format(w!("CanIncludeInClipboardHistory"))
        .and_then(data)
        .is_some_and(|value| value.get(..4) == Some(&[0, 0, 0, 0]))
}

/// A copy of the data in one format. The clipboard must be open.
fn data(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the handle belongs to the open clipboard and stays valid until it closes; the
    // memory is read only between lock and unlock, and no further than its size.
    unsafe {
        let memory = HGLOBAL(GetClipboardData(format).ok()?.0);
        let size = GlobalSize(memory);
        let start = GlobalLock(memory) as *const u8;
        if start.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(start, size).to_vec();
        let _ = GlobalUnlock(memory);
        Some(bytes)
    }
}

/// Hands a copy of `bytes` to the clipboard, which owns it from then on. The clipboard must be
/// open for a window.
fn put(format: u32, bytes: &[u8]) -> Result<(), String> {
    // SAFETY: the memory is written only between lock and unlock, within its size, and freed
    // here only when the clipboard did not take it.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|e| e.to_string())?;
        let start = GlobalLock(memory) as *mut u8;
        if start.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err("could not lock the clipboard memory".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), start, bytes.len());
        let _ = GlobalUnlock(memory);
        if let Err(err) = SetClipboardData(format, Some(HANDLE(memory.0))) {
            let _ = GlobalFree(Some(memory));
            return Err(err.to_string());
        }
    }
    Ok(())
}

/// Reads a `DROPFILES` list: a 20-byte header that says where the paths start and whether they
/// are UTF-16, then the paths, each ending in a null, with one more null after the last.
fn parse_drop_files(list: &[u8]) -> Option<Vec<String>> {
    let start = u32::from_le_bytes(list.get(0..4)?.try_into().ok()?) as usize;
    let wide = u32::from_le_bytes(list.get(16..20)?.try_into().ok()?) != 0;
    let names = list.get(start..)?;
    let paths: Vec<String> = if wide {
        let units: Vec<u16> = names.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
        units.split(|&unit| unit == 0).take_while(|path| !path.is_empty()).map(String::from_utf16_lossy).collect()
    } else {
        // Old ANSI lists; their non-ASCII names come out garbled.
        names.split(|&byte| byte == 0).take_while(|path| !path.is_empty()).map(|path| String::from_utf8_lossy(path).into_owned()).collect()
    };
    (!paths.is_empty()).then_some(paths)
}

fn drop_files(paths: &[String]) -> Vec<u8> {
    // Paths start after the header; no drop point; UTF-16 names.
    let mut list: Vec<u8> = [20u32, 0, 0, 0, 1].iter().flat_map(|value| value.to_le_bytes()).collect();
    for path in paths {
        list.extend(path.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
    }
    list.extend([0, 0]);
    list
}

/// Reads a device-independent bitmap of 24 or 32 bits per pixel, the kind screenshots and
/// image editors copy. Other depths are rare on today's clipboard and are skipped.
fn dib_to_picture(dib: &[u8]) -> Option<Picture> {
    let u32_at = |at: usize| dib.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0)? as usize;
    let width = u32_at(4)? as i32;
    let height = u32_at(8)? as i32;
    let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]);
    let compression = u32_at(16)?;
    let palette = u32_at(32)? as usize;
    if header < 40 || width <= 0 || height == 0 || !matches!(bits, 24 | 32) {
        return None;
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return None;
    }
    // Red, green, blue and alpha masks. A plain header is followed by the masks it uses;
    // newer headers hold them.
    let (masks, after_masks) = match compression {
        BI_RGB => ([0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000], 0),
        BI_BITFIELDS | BI_ALPHABITFIELDS if bits == 32 => {
            // Either way the masks start right after the fields of the plain header.
            let alpha = compression == BI_ALPHABITFIELDS || header >= 56;
            let masks = [u32_at(40)?, u32_at(44)?, u32_at(48)?, if alpha { u32_at(52)? } else { 0 }];
            let extra = match (header, alpha) {
                (40, true) => 16,
                (40, false) => 12,
                _ => 0,
            };
            (masks, extra)
        }
        _ => return None,
    };
    let start = header + after_masks + palette * 4;
    let stride = (w * usize::from(bits) / 8).div_ceil(4) * 4;
    let pixels = dib.get(start..start + stride * h)?;

    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        // Positive heights are stored bottom row first.
        let source = if height > 0 { h - 1 - row } else { row };
        let line = &pixels[source * stride..];
        for x in 0..w {
            if bits == 24 {
                let p = &line[x * 3..x * 3 + 3];
                rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
            } else {
                let p = &line[x * 4..x * 4 + 4];
                let value = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                rgba.extend(masks.map(|mask| channel(value, mask)));
            }
        }
    }
    // Most apps leave the fourth byte of 32-bit bitmaps at zero rather than mean transparency.
    if bits == 32 && rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0) {
        rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 255);
    }
    Some(Picture { width: w as u32, height: h as u32, rgba })
}

/// One colour channel scaled to 8 bits; 0 for a channel the bitmap does not have.
fn channel(value: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let width = (mask >> shift).count_ones();
    let raw = (value & mask) >> shift;
    if width >= 8 { (raw >> (width - 8)) as u8 } else { (raw * 255 / ((1 << width) - 1)) as u8 }
}

/// A 32-bit bottom-up bitmap with a plain header, which every app understands.
fn picture_to_dib(picture: &Picture) -> Vec<u8> {
    let (w, h) = (picture.width as usize, picture.height as usize);
    let mut dib = Vec::with_capacity(40 + w * h * 4);
    for value in [40, picture.width, picture.height] {
        dib.extend_from_slice(&value.to_le_bytes());
    }
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    for value in [BI_RGB, (w * h * 4) as u32, 0, 0, 0, 0] {
        dib.extend_from_slice(&value.to_le_bytes());
    }
    for row in picture.rgba.chunks_exact(w * 4).rev() {
        for pixel in row.as_chunks::<4>().0 {
            dib.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    dib
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmaps_survive_the_round_trip() {
        let picture = Picture { width: 2, height: 2, rgba: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 9, 9, 9, 255] };
        let back = dib_to_picture(&picture_to_dib(&picture)).unwrap();
        assert_eq!((back.width, back.height), (2, 2));
        assert_eq!(back.rgba, picture.rgba);
    }

    #[test]
    fn file_lists_survive_the_round_trip() {
        let paths = vec!["C:\\Users\\Анна\\фото.jpg".to_string(), "D:\\work".to_string()];
        assert_eq!(parse_drop_files(&drop_files(&paths)), Some(paths));
        assert_eq!(parse_drop_files(&drop_files(&[])), None);
    }

    #[test]
    fn reads_24_bit_and_opaque_32_bit_bitmaps() {
        // 1×2, 24 bits, bottom-up: rows padded to 4 bytes, blue then red in BGR order.
        let mut dib = Vec::new();
        for value in [40u32, 1, 2] {
            dib.extend_from_slice(&value.to_le_bytes());
        }
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&24u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[255, 0, 0, 0, 0, 0, 255, 0]);
        let picture = dib_to_picture(&dib).unwrap();
        assert_eq!(picture.rgba, vec![255, 0, 0, 255, 0, 0, 255, 255]);

        // A 32-bit screenshot with the fourth byte left at zero is opaque.
        let opaque = Picture { width: 1, height: 1, rgba: vec![1, 2, 3, 0] };
        assert_eq!(dib_to_picture(&picture_to_dib(&opaque)).unwrap().rgba, vec![1, 2, 3, 255]);
    }

    #[test]
    fn reads_bitfield_masks_after_a_plain_header() {
        let mut dib = Vec::new();
        for value in [40u32, 1, 1] {
            dib.extend_from_slice(&value.to_le_bytes());
        }
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&BI_BITFIELDS.to_le_bytes());
        dib.extend_from_slice(&[0; 20]);
        for mask in [0x0000_00ffu32, 0x0000_ff00, 0x00ff_0000] {
            dib.extend_from_slice(&mask.to_le_bytes());
        }
        dib.extend_from_slice(&[10, 20, 30, 0]);
        assert_eq!(dib_to_picture(&dib).unwrap().rgba, vec![10, 20, 30, 255]);
    }
}
