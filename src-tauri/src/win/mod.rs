//! Windows integration: the app that receives a dictation, clipboard delivery and guarded paste.
//! Replaces BetterWispr's NSWorkspace, NSPasteboard and CGEvent code.

pub mod clipboard;
pub mod hotkey;
pub mod uia;

use std::time::{Duration, Instant};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Marks input this app injects so the keyboard hook ignores it.
pub const INJECTED_TAG: usize = 0x4F44_5054; // "ODPT"

/// The app that had focus when a dictation started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForegroundApp {
    pub hwnd: isize,
    pub pid: u32,
    /// Executable file name, e.g. "slack.exe".
    pub exe: Option<String>,
}

pub fn foreground_app() -> Option<ForegroundApp> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        Some(ForegroundApp { hwnd: hwnd.0 as isize, pid, exe: exe_name(pid) })
    }
}

pub fn is_own_process(pid: u32) -> bool {
    unsafe { GetCurrentProcessId() == pid }
}

fn exe_name(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        path.rsplit(['\\', '/']).next().map(String::from)
    }
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok.then_some(elevation.TokenIsElevated != 0)
    }
}

/// Windows blocks synthetic input into apps running as administrator unless this app is too.
/// When the target can't be inspected, assume it is protected and copy instead of pasting.
pub fn can_send_input_to(pid: u32) -> bool {
    unsafe {
        let ours = token_elevated(GetCurrentProcess()).unwrap_or(false);
        if ours {
            return true;
        }
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let theirs = token_elevated(process);
        let _ = CloseHandle(process);
        theirs == Some(false)
    }
}

/// True while the foreground window still belongs to the app that was focused when dictation started.
pub fn still_focused(target: &ForegroundApp) -> bool {
    foreground_app().is_some_and(|now| now.pid == target.pid && !is_own_process(now.pid))
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    }
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
}

/// Waits until the user lets go of modifiers, so Ctrl+V isn't turned into Win+Ctrl+V or Alt+Ctrl+V.
pub fn wait_for_modifiers_released(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let modifiers = [VK_LWIN, VK_RWIN, VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_RCONTROL];
    while Instant::now() < deadline {
        if !modifiers.iter().any(|vk| is_down(*vk)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    false
}

/// Sends Ctrl+V to the focused window.
pub fn send_paste() -> bool {
    let inputs = [key_input(VK_CONTROL, false), key_input(VK_V, false), key_input(VK_V, true), key_input(VK_CONTROL, true)];
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len() }
}

/// Sends an unassigned key so releasing Win or Alt doesn't open the Start menu or a menu bar.
pub fn send_mask_key() {
    let vk = VIRTUAL_KEY(0xE8);
    let inputs = [key_input(vk, false), key_input(vk, true)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Opens a Windows Settings page, e.g. "privacy-microphone".
pub fn open_settings(page: &str) {
    let _ = std::process::Command::new("explorer.exe").arg(format!("ms-settings:{page}")).spawn();
}
