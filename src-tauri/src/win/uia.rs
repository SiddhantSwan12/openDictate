//! Watches the field that received a dictation and reports word fixes the user types there.
//! UI Automation replaces BetterWispr's AXUIElement polling; password fields are never read.

use opendictate_core::corrections::{corrections, edit_of, LearnedCorrection, SettledText};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_TextPatternId,
    UIA_ValuePatternId,
};

fn value(element: &IUIAutomationElement) -> Option<String> {
    unsafe {
        if element.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(true) {
            return None;
        }
        if let Ok(pattern) = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
            if let Ok(value) = pattern.CurrentValue() {
                return Some(value.to_string());
            }
        }
        let pattern = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId).ok()?;
        let range = pattern.DocumentRange().ok()?;
        range.GetText(200_000).ok().map(|t| t.to_string())
    }
}

fn focused_field(automation: &IUIAutomation, pid: u32, stop: &AtomicBool) -> Option<IUIAutomationElement> {
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(500));
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let Ok(element) = (unsafe { automation.GetFocusedElement() }) else { continue };
        if unsafe { element.CurrentProcessId() }.ok() != Some(pid as i32) {
            continue;
        }
        if unsafe { element.CurrentIsPassword() }.map(|b| b.as_bool()).unwrap_or(true) {
            return None;
        }
        if value(&element).is_some() {
            return Some(element);
        }
    }
    None
}

/// Starts watching on a background thread. Set `stop` to end early.
pub fn watch(
    inserted: String,
    pid: u32,
    stop: Arc<AtomicBool>,
    on_corrections: impl Fn(Vec<LearnedCorrection>) + Send + 'static,
) {
    if inserted.is_empty() {
        return;
    }
    std::thread::spawn(move || unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return;
        }
        let run = || -> Option<()> {
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let field = focused_field(&automation, pid, &stop)?;
            let mut inserted = inserted;
            let mut baseline: Option<String> = None;
            let mut settled = SettledText::new("");
            for _ in 0..60 {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let current = value(&field)?;
                if current.chars().count() > 100_000 {
                    break;
                }
                match &baseline {
                    Some(base) => {
                        if let Some(text) = settled.observe(&current) {
                            if &text != base {
                                if let Some(edited) = edit_of(&inserted, base, &text) {
                                    let found = corrections(&inserted, &edited);
                                    if !found.is_empty() {
                                        on_corrections(found);
                                    }
                                    inserted = edited;
                                    baseline = Some(text);
                                }
                            }
                        }
                        if current.is_empty() {
                            break;
                        }
                    }
                    None if current.contains(&inserted) => {
                        settled = SettledText::new(&current);
                        baseline = Some(current);
                    }
                    None => {}
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Some(())
        };
        run();
        CoUninitialize();
    });
}
