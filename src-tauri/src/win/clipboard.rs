//! Clipboard access with snapshot and restore, so automated paste can put back what the user had copied.

use std::time::Duration;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;

/// Clipboard formats whose handles are not global memory (GDI objects, metafiles, owner-drawn).
fn is_memory_format(format: u32) -> bool {
    !matches!(format, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E) && !(0x300..=0x3FF).contains(&format)
}

struct Open;

impl Open {
    /// Another app may hold the clipboard briefly; retry for up to half a second.
    fn new() -> Result<Self, String> {
        for _ in 0..25 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Open);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("Couldn't open the clipboard. Another app is using it.".into())
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Changes whenever any app writes to the clipboard.
pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

fn alloc(bytes: &[u8]) -> Result<HGLOBAL, String> {
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(|e| e.to_string())?;
        let pointer = GlobalLock(handle) as *mut u8;
        if pointer.is_null() {
            return Err("Couldn't lock clipboard memory.".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
        let _ = GlobalUnlock(handle);
        Ok(handle)
    }
}

pub fn set_text(text: &str) -> Result<(), String> {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes: Vec<u8> = wide.iter().flat_map(|w| w.to_le_bytes()).collect();
    let _open = Open::new()?;
    unsafe {
        EmptyClipboard().map_err(|e| e.to_string())?;
        let handle = alloc(&bytes)?;
        SetClipboardData(CF_UNICODETEXT, Some(HANDLE(handle.0))).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn get_text() -> Option<String> {
    let _open = Open::new().ok()?;
    unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
        let global = HGLOBAL(handle.0);
        let pointer = GlobalLock(global) as *const u16;
        if pointer.is_null() {
            return None;
        }
        let max = GlobalSize(global) / 2;
        let slice = std::slice::from_raw_parts(pointer, max);
        let end = slice.iter().position(|c| *c == 0).unwrap_or(max);
        let text = String::from_utf16_lossy(&slice[..end]);
        let _ = GlobalUnlock(global);
        Some(text)
    }
}

/// Everything on the clipboard that can be copied byte for byte.
pub struct Snapshot(Vec<(u32, Vec<u8>)>);

pub fn snapshot() -> Snapshot {
    let Ok(_open) = Open::new() else { return Snapshot(vec![]) };
    let mut items = Vec::new();
    unsafe {
        let mut format = EnumClipboardFormats(0);
        while format != 0 {
            if is_memory_format(format) {
                if let Ok(handle) = GetClipboardData(format) {
                    let global = HGLOBAL(handle.0);
                    let pointer = GlobalLock(global) as *const u8;
                    if !pointer.is_null() {
                        let size = GlobalSize(global);
                        items.push((format, std::slice::from_raw_parts(pointer, size).to_vec()));
                        let _ = GlobalUnlock(global);
                    }
                }
            }
            format = EnumClipboardFormats(format);
        }
    }
    Snapshot(items)
}

pub fn restore(snapshot: &Snapshot) -> Result<(), String> {
    let _open = Open::new()?;
    unsafe {
        EmptyClipboard().map_err(|e| e.to_string())?;
        for (format, bytes) in &snapshot.0 {
            if let Ok(handle) = alloc(bytes) {
                let _ = SetClipboardData(*format, Some(HANDLE(handle.0)));
            }
        }
    }
    Ok(())
}
