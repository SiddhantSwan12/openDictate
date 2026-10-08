//! Global hold-to-talk shortcut through a low-level keyboard hook. Unlike RegisterHotKey this sees
//! key releases and modifier-only combinations such as Ctrl + Win. Replaces BetterWispr's Carbon/IOKit shortcut.

use super::INJECTED_TAG;
use opendictate_core::model::Shortcut;
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadInformation, SetThreadPriority, ThreadPowerThrottling, THREAD_POWER_THROTTLING_CURRENT_VERSION,
    THREAD_POWER_THROTTLING_EXECUTION_SPEED, THREAD_POWER_THROTTLING_STATE, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Clone, Debug, PartialEq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
    /// Another key joined a modifier-only shortcut, so the user meant a different combination.
    Interrupted,
    Captured(Shortcut),
    CaptureCancelled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Modifiers {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
}

impl Modifiers {
    fn of(shortcut: &Shortcut) -> Self {
        Self { ctrl: shortcut.ctrl, alt: shortcut.alt, shift: shortcut.shift, win: shortcut.win }
    }

    fn count(self) -> usize {
        [self.ctrl, self.alt, self.shift, self.win].iter().filter(|m| **m).count()
    }

    fn union(self, other: Self) -> Self {
        Self { ctrl: self.ctrl || other.ctrl, alt: self.alt || other.alt, shift: self.shift || other.shift, win: self.win || other.win }
    }
}

#[derive(Default)]
struct HookState {
    shortcut: Option<Shortcut>,
    sender: Option<Sender<HotkeyEvent>>,
    /// Physically held non-modifier keys.
    keys: HashSet<u32>,
    modifiers: Modifiers,
    active: bool,
    capturing: bool,
    capture_peak: Modifiers,
    suspended: bool,
    /// Reads modifier state from the keyboard; off in unit tests, which feed synthetic events.
    live_state: bool,
}

static STATE: Mutex<Option<HookState>> = Mutex::new(None);

enum Kind {
    Ctrl,
    Alt,
    Shift,
    Win,
    Key,
}

fn kind(vk: u32) -> Kind {
    match vk {
        0x11 | 0xA2 | 0xA3 => Kind::Ctrl,
        0x12 | 0xA4 | 0xA5 => Kind::Alt,
        0x10 | 0xA0 | 0xA1 => Kind::Shift,
        0x5B | 0x5C => Kind::Win,
        _ => Kind::Key,
    }
}

pub fn key_name(vk: u32, scan: u32, extended: bool) -> String {
    match vk {
        0x20 => return "Space".into(),
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        _ => {}
    }
    let mut buffer = [0u16; 64];
    let lparam = ((scan as i32) << 16) | if extended { 1 << 24 } else { 0 };
    let n = unsafe { GetKeyNameTextW(lparam, &mut buffer) };
    if n > 0 {
        let name = String::from_utf16_lossy(&buffer[..n as usize]);
        let mut chars = name.chars();
        chars.next().map(|c| c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()).unwrap_or(name)
    } else {
        format!("Key {vk}")
    }
}

fn physically_down(vk: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

/// Returns true to swallow the event.
fn handle(state: &mut HookState, vk: u32, scan: u32, extended: bool, down: bool) -> bool {
    let send = |state: &HookState, event: HotkeyEvent| {
        if let Some(sender) = &state.sender {
            let _ = sender.send(event);
        }
    };
    let was_down = match kind(vk) {
        Kind::Ctrl => std::mem::replace(&mut state.modifiers.ctrl, down),
        Kind::Alt => std::mem::replace(&mut state.modifiers.alt, down),
        Kind::Shift => std::mem::replace(&mut state.modifiers.shift, down),
        Kind::Win => std::mem::replace(&mut state.modifiers.win, down),
        Kind::Key => {
            if down {
                !state.keys.insert(vk)
            } else {
                state.keys.remove(&vk)
            }
        }
    };
    let is_modifier = !matches!(kind(vk), Kind::Key);
    let repeat = down && was_down;
    if state.live_state {
        // Windows may skip a hook call; trust the live state of the other modifiers so one missed event can't wedge the shortcut.
        let current = kind(vk);
        let live = |a: u32, b: u32| physically_down(a) || physically_down(b);
        if !matches!(current, Kind::Ctrl) {
            state.modifiers.ctrl = live(0xA2, 0xA3);
        }
        if !matches!(current, Kind::Alt) {
            state.modifiers.alt = live(0xA4, 0xA5);
        }
        if !matches!(current, Kind::Shift) {
            state.modifiers.shift = live(0xA0, 0xA1);
        }
        if !matches!(current, Kind::Win) {
            state.modifiers.win = live(0x5B, 0x5C);
        }
    }


    if state.capturing {
        if down && !is_modifier {
            if vk == 0x1B && state.modifiers.count() == 0 {
                state.capturing = false;
                send(state, HotkeyEvent::CaptureCancelled);
                return true;
            }
            let m = state.modifiers;
            let shortcut = Shortcut {
                ctrl: m.ctrl,
                alt: m.alt,
                shift: m.shift,
                win: m.win,
                key: Some(vk),
                key_name: key_name(vk, scan, extended),
            };
            state.capturing = false;
            send(state, HotkeyEvent::Captured(shortcut));
            return true;
        }
        if is_modifier {
            state.capture_peak = state.capture_peak.union(state.modifiers);
            if !down && state.modifiers.count() == 0 && state.capture_peak.count() >= 2 {
                let m = state.capture_peak;
                state.capturing = false;
                send(
                    state,
                    HotkeyEvent::Captured(Shortcut { ctrl: m.ctrl, alt: m.alt, shift: m.shift, win: m.win, key: None, key_name: String::new() }),
                );
            }
        }
        return true;
    }

    let Some(shortcut) = state.shortcut.clone() else { return false };
    if state.suspended {
        return false;
    }
    let required = Modifiers::of(&shortcut);
    match shortcut.key {
        Some(key) => {
            if vk == key {
                if down && !repeat && !state.active && state.modifiers == required {
                    state.active = true;
                    send(state, HotkeyEvent::Pressed);
                    return true;
                }
                if state.active {
                    if !down {
                        state.active = false;
                        send(state, HotkeyEvent::Released);
                    }
                    return true;
                }
            } else if state.active && !down && is_modifier && Modifiers::of(&shortcut).count() > 0 && state.modifiers != required {
                state.active = false;
                send(state, HotkeyEvent::Released);
            }
            false
        }
        None => {
            if is_modifier && down && !state.keys.is_empty() {
                // A key-up can be missed (e.g. while a secure desktop had focus); trust the live key state.
                state.keys.retain(|vk| physically_down(*vk));
            }
            if is_modifier {
                if down && !repeat && !state.active && state.modifiers == required && state.keys.is_empty() {
                    state.active = true;
                    send(state, HotkeyEvent::Pressed);
                } else if !down && state.active && !(required.union(state.modifiers) == state.modifiers) {
                    state.active = false;
                    send(state, HotkeyEvent::Released);
                }
            } else if down && state.active {
                state.active = false;
                send(state, HotkeyEvent::Interrupted);
            }
            false
        }
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if info.dwExtraInfo != INJECTED_TAG {
            let message = wparam.0 as u32;
            let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
            let up = message == WM_KEYUP || message == WM_SYSKEYUP;
            if down || up {
                let extended = info.flags.0 & LLKHF_EXTENDED.0 != 0;
                let swallow = STATE
                    .lock()
                    .ok()
                    .and_then(|mut guard| guard.as_mut().map(|s| handle(s, info.vkCode, info.scanCode, extended, down)))
                    .unwrap_or(false);
                if swallow {
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Installs the hook on its own thread with a message loop. Events arrive on `sender`.
pub fn start(sender: Sender<HotkeyEvent>) {
    *STATE.lock().unwrap() = Some(HookState { sender: Some(sender), live_state: true, ..Default::default() });
    std::thread::Builder::new()
        .name("keyboard-hook".into())
        .spawn(|| unsafe {
            // Windows skips a low-level hook that answers slowly, so keep this thread fast and unthrottled.
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
            let throttling = THREAD_POWER_THROTTLING_STATE {
                Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: 0,
            };
            let _ = SetThreadInformation(
                GetCurrentThread(),
                ThreadPowerThrottling,
                &throttling as *const _ as *const _,
                std::mem::size_of::<THREAD_POWER_THROTTLING_STATE>() as u32,
            );
            let module = GetModuleHandleW(None).ok();
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module.map(Into::into), 0);
            if hook.is_err() {
                log::error!("Couldn't install the keyboard hook: {hook:?}");
                return;
            }
            log::info!("keyboard hook installed");
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        })
        .expect("keyboard hook thread");
}

fn with_state(f: impl FnOnce(&mut HookState)) {
    if let Ok(mut guard) = STATE.lock() {
        if let Some(state) = guard.as_mut() {
            f(state);
        }
    }
}

pub fn set_shortcut(shortcut: Shortcut) {
    with_state(|s| {
        s.shortcut = Some(shortcut);
        s.active = false;
    });
}

/// Pauses the shortcut, e.g. while a settings field records a new one.
pub fn set_suspended(suspended: bool) {
    with_state(|s| {
        s.suspended = suspended;
        s.active = false;
    });
}

pub fn begin_capture() {
    with_state(|s| {
        s.capturing = true;
        s.capture_peak = Modifiers::default();
        s.active = false;
    });
}

pub fn cancel_capture() {
    with_state(|s| s.capturing = false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn state(shortcut: Shortcut) -> (HookState, std::sync::mpsc::Receiver<HotkeyEvent>) {
        let (tx, rx) = channel();
        (HookState { shortcut: Some(shortcut), sender: Some(tx), ..Default::default() }, rx)
    }

    #[test]
    fn ctrl_win_hold_presses_and_releases() {
        let (mut s, rx) = state(Shortcut::default());
        handle(&mut s, 0xA2, 0, false, true);
        handle(&mut s, 0x5B, 0, false, true);
        handle(&mut s, 0x5B, 0, false, true); // auto-repeat
        handle(&mut s, 0x5B, 0, false, false);
        handle(&mut s, 0xA2, 0, false, false);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [HotkeyEvent::Pressed, HotkeyEvent::Released]);
    }

    #[test]
    fn another_key_interrupts_a_modifier_only_shortcut() {
        let (mut s, rx) = state(Shortcut::default());
        handle(&mut s, 0xA2, 0, false, true);
        handle(&mut s, 0x5B, 0, false, true);
        handle(&mut s, 0x44, 0, false, true); // D
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [HotkeyEvent::Pressed, HotkeyEvent::Interrupted]);
    }

    #[test]
    fn key_shortcut_swallows_its_key() {
        let space = Shortcut { ctrl: false, alt: true, shift: false, win: false, key: Some(0x20), key_name: "Space".into() };
        let (mut s, rx) = state(space);
        assert!(!handle(&mut s, 0xA4, 0, false, true));
        assert!(handle(&mut s, 0x20, 0, false, true));
        assert!(handle(&mut s, 0x20, 0, false, true));
        assert!(handle(&mut s, 0x20, 0, false, false));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [HotkeyEvent::Pressed, HotkeyEvent::Released]);
        // Space alone types normally.
        handle(&mut s, 0xA4, 0, false, false);
        assert!(!handle(&mut s, 0x20, 0, false, true));
    }

    #[test]
    fn capture_records_modifier_only_combinations() {
        let (mut s, rx) = state(Shortcut::default());
        s.capturing = true;
        handle(&mut s, 0xA3, 0, false, true);
        handle(&mut s, 0xA5, 0, false, true);
        handle(&mut s, 0xA5, 0, false, false);
        handle(&mut s, 0xA3, 0, false, false);
        let events: Vec<_> = rx.try_iter().collect();
        assert!(matches!(&events[..], [HotkeyEvent::Captured(sc)] if sc.ctrl && sc.alt && sc.key.is_none()));
    }
}
