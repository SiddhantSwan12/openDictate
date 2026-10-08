//! The floating capsule: a small always-on-top window above the taskbar that never takes focus,
//! so text still lands in the app being dictated into. Also shows toasts.

use crate::app::{Phase, Session, Toast};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "capsule";
const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 170.0;

pub fn create(handle: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(handle, LABEL, WebviewUrl::App("index.html#/capsule".into()))
        .title("OpenDictate capsule")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        // Clicks on the capsule never take focus from the app being dictated into.
        .focusable(false)
        .visible(false)
        .build()?;
    Ok(())
}

/// Right edge of the work area, vertically centered, on the monitor under the pointer.
fn position(handle: &AppHandle) {
    let Some(window) = handle.get_webview_window(LABEL) else { return };
    let monitor = handle
        .cursor_position()
        .ok()
        .and_then(|p| handle.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| handle.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let size = PhysicalSize::new((WIDTH * scale) as u32, (HEIGHT * scale) as u32);
    let x = area.position.x + area.size.width as i32 - size.width as i32 - (8.0 * scale) as i32;
    let y = area.position.y + (area.size.height as i32 - size.height as i32) / 2;
    let _ = window.set_size(size);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show(handle: &AppHandle) {
    let Some(window) = handle.get_webview_window(LABEL) else { return };
    if window.is_visible().unwrap_or(false) {
        return;
    }
    position(handle);
    // Tauri's show applies the frameless, transparent styles and doesn't activate a non-focusable window.
    let _ = window.show();
    let _ = window.set_always_on_top(true);
}

pub fn hide(handle: &AppHandle) {
    if let Some(window) = handle.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

pub fn update(handle: &AppHandle, session: &Session, show_capsule: bool) {
    let _ = handle.emit_to(LABEL, "session", session);
    let card = matches!(session.phase, Phase::Failed { .. } | Phase::Unpasted { .. });
    if session.phase == Phase::Idle {
        // The capsule hides itself after any toast finishes (see `capsule_idle`).
        let _ = handle.emit_to(LABEL, "idle", ());
    } else if show_capsule || card {
        show(handle);
    }
}

/// Shows the notetaker pill while a meeting records; otherwise lets the capsule hide when idle.
pub fn meeting(handle: &AppHandle, capturing: bool) {
    if capturing {
        show(handle);
    } else {
        let _ = handle.emit_to(LABEL, "idle", ());
    }
}

pub fn toast(handle: &AppHandle, toast: &Toast) {
    show(handle);
    let _ = handle.emit_to(LABEL, "toast", toast);
}
