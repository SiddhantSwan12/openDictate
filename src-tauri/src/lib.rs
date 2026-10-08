//! OpenDictate: free, private, offline voice dictation for Windows.
//! A Windows port of BetterWispr (Apache-2.0, https://github.com/opennookorg/betterwispr).

mod app;
mod capsule;
mod credentials;
mod engine;
mod meetings;
mod paths;
mod recorder;
mod win;

use app::{App, Session, Workspace};
use meetings::MeetingsView;
use opendictate_core::model::{AppSettings, NotesCli, SpeechConnection};
use std::sync::{Arc, OnceLock};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

type AppState<'a> = State<'a, Arc<App>>;

/// The most capable graphics adapter, shown in Models and Settings.
pub fn gpu_name() -> Option<String> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(|| unsafe {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: Option<(usize, String)> = None;
        let mut index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            index += 1;
            let Ok(desc) = adapter.GetDesc1() else { continue };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let end = desc.Description.iter().position(|c| *c == 0).unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..end]);
            if best.as_ref().is_none_or(|(memory, _)| desc.DedicatedVideoMemory > *memory) {
                best = Some((desc.DedicatedVideoMemory, name));
            }
        }
        best.map(|(_, name)| name)
    })
    .clone()
}

// MARK: Session and workspace

#[tauri::command]
fn get_session(app: AppState) -> Session {
    app.session()
}

#[tauri::command]
fn get_workspace(app: AppState) -> Workspace {
    app.workspace()
}

#[tauri::command]
fn toggle_recording(app: AppState) {
    app.toggle_recording();
}

#[tauri::command]
fn finish_recording(app: AppState) {
    app.finish_recording();
}

#[tauri::command]
fn cancel_recording(app: AppState) {
    app.cancel_recording();
}

#[tauri::command]
fn dismiss_card(app: AppState) {
    app.dismiss_card();
}

#[tauri::command]
fn capsule_idle(app: AppState) {
    if matches!(app.session().phase, app::Phase::Idle) && !app.meetings.is_capturing() {
        capsule::hide(&app.handle);
    }
}

// MARK: Models

#[tauri::command]
fn select_model(app: AppState, id: String) -> Result<(), String> {
    app.select_model(&id)
}

#[tauri::command]
fn install_model(app: AppState, id: String) -> Result<(), String> {
    app.install_model(&id)
}

#[tauri::command]
fn cancel_installation(app: AppState) {
    app.cancel_installation();
}

#[tauri::command]
fn uninstall_model(app: AppState, id: String) -> Result<(), String> {
    app.uninstall_model(&id)
}

#[tauri::command]
fn save_connection(app: AppState, connection: SpeechConnection, key: Option<String>, for_notes: bool) -> Result<(), String> {
    app.save_connection(connection, key.filter(|k| !k.is_empty()), for_notes)
}

#[tauri::command]
fn delete_connection(app: AppState, id: String, for_notes: bool) -> Result<(), String> {
    app.delete_connection(&id, for_notes)
}

#[tauri::command]
fn connection_has_key(app: AppState, connection: SpeechConnection, for_notes: bool) -> bool {
    app.has_key(&connection, for_notes)
}

#[tauri::command]
fn new_connection(api: opendictate_core::model::SpeechApi) -> SpeechConnection {
    SpeechConnection::new(api)
}

// MARK: Notes models

#[tauri::command]
async fn ollama_models() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| opendictate_notes::ollama_models().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn cli_catalog(cli: NotesCli) -> Result<opendictate_notes::cli::CliCatalog, String> {
    tauri::async_runtime::spawn_blocking(move || opendictate_notes::cli::catalog(cli).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cli_installed(cli: NotesCli) -> bool {
    opendictate_notes::cli::executable(cli).is_some()
}

#[tauri::command]
fn notes_availability(app: AppState) -> Option<String> {
    opendictate_notes::availability(&app.settings()).err()
}

/// Sends a short synthetic sample to the selected notes model so the user can check it works.
#[tauri::command]
async fn test_notes_model(app: AppState<'_>) -> Result<String, String> {
    let settings = app.settings();
    tauri::async_runtime::spawn_blocking(move || {
        let key = |c: &SpeechConnection| credentials::read(credentials::Purpose::Notes, c).ok().flatten();
        let model = opendictate_notes::language_model(&settings, opendictate_notes::INSTRUCTIONS, &key).map_err(|e| e.to_string())?;
        let me = opendictate_core::model::MeetingSegment::new(
            opendictate_core::model::Speaker::Me,
            0.0,
            5.0,
            "Let's ship the beta on Friday. Priya will send the checklist by Thursday.".into(),
            String::new(),
        );
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let notes = opendictate_notes::generate(&[&me], "", model.as_ref(), &cancel, &|_, _| {}).map_err(|e| e.to_string())?;
        Ok(format!("{} — {}", notes.title, notes.summary.overview))
    })
    .await
    .map_err(|e| e.to_string())?
}

// MARK: Settings

#[tauri::command]
fn update_settings(app: AppState, settings: AppSettings) -> Result<(), String> {
    app.update_settings(settings)
}

#[tauri::command]
fn finish_onboarding(app: AppState) -> Result<(), String> {
    app.finish_onboarding()
}

#[tauri::command]
fn show_onboarding(app: AppState) -> Result<(), String> {
    app.show_onboarding()
}

#[tauri::command]
fn begin_shortcut_capture() {
    win::hotkey::begin_capture();
}

#[tauri::command]
fn cancel_shortcut_capture() {
    win::hotkey::cancel_capture();
}

#[tauri::command]
fn open_windows_settings(page: String) {
    win::open_settings(&page);
}

#[tauri::command]
fn open_data_folder() {
    let _ = std::process::Command::new("explorer.exe").arg(paths::app_dir()).spawn();
}

// MARK: History and vocabulary

#[tauri::command]
fn add_vocabulary(app: AppState, phrase: String, replacement: String) -> Result<(), String> {
    app.add_vocabulary(&phrase, &replacement)
}

#[tauri::command]
fn delete_vocabulary(app: AppState, id: String) -> Result<(), String> {
    app.delete_vocabulary(&id)
}

#[tauri::command]
fn delete_transcript(app: AppState, id: String) -> Result<(), String> {
    app.delete_transcript(&id)
}

#[tauri::command]
fn clear_history(app: AppState) -> Result<(), String> {
    app.clear_history()
}

#[tauri::command]
fn update_transcript(app: AppState, id: String, text: String) -> Result<(), String> {
    app.update_transcript(&id, &text)
}

#[tauri::command]
fn restore_original(app: AppState, id: String) -> Result<(), String> {
    app.restore_original(&id)
}

#[tauri::command]
fn export_history(app: AppState, path: String) -> Result<(), String> {
    app.export_history(&path)
}

#[tauri::command]
fn copy_text(app: AppState, text: String) {
    app.copy_text(&text);
}

// MARK: Meetings

#[tauri::command]
fn get_meetings(app: AppState) -> MeetingsView {
    app.meetings.view(&app.settings())
}

#[tauri::command]
fn meeting_start(app: AppState) -> Result<String, String> {
    app.meetings.start(&app)
}

#[tauri::command]
fn meeting_stop(app: AppState) {
    app.meetings.stop(&app);
}

#[tauri::command]
fn meeting_generate_notes(app: AppState, id: String) -> Result<(), String> {
    app.meetings.generate_notes(&app, &id)
}

#[tauri::command]
fn meeting_cancel_notes(app: AppState) {
    app.meetings.cancel_notes(&app);
}

#[tauri::command]
fn meeting_update(app: AppState, id: String, title: Option<String>, notes: Option<String>) {
    app.meetings.update(&app, &id, title, notes);
}

#[tauri::command]
fn meeting_toggle_action(app: AppState, id: String, item: String) {
    app.meetings.toggle_action_item(&app, &id, &item);
}

#[tauri::command]
fn meeting_delete(app: AppState, id: String) -> Result<(), String> {
    app.meetings.delete(&app, &id)
}

#[tauri::command]
fn meeting_copy(app: AppState, id: String, transcript_only: bool) {
    if let Some(text) = app.meetings.text(&id, transcript_only) {
        app.copy_text(&text);
    }
}

#[tauri::command]
fn meeting_export(app: AppState, id: String, path: String) -> Result<(), String> {
    let text = app.meetings.text(&id, false).ok_or("Unknown meeting.")?;
    opendictate_core::model::write_atomic(std::path::Path::new(&path), text.as_bytes())
}

#[tauri::command]
fn meeting_dismiss_message(app: AppState) {
    app.meetings.dismiss_message(&app);
}

#[tauri::command]
fn show_main(app: AppState, page: Option<String>) {
    app.show_main_window(page.as_deref());
}

#[tauri::command]
fn quit_app(handle: AppHandle) {
    quit(&handle);
}

fn quit(handle: &AppHandle) {
    if let Some(app) = handle.try_state::<Arc<App>>() {
        app.meetings.end_for_quit();
    }
    handle.exit(0);
}

fn build_tray(handle: &AppHandle) -> tauri::Result<()> {
    let dictate = MenuItem::with_id(handle, "dictate", "Start or stop dictation", true, None::<&str>)?;
    let meeting = MenuItem::with_id(handle, "meeting", "Start or stop a meeting", true, None::<&str>)?;
    let open = MenuItem::with_id(handle, "open", "Open OpenDictate", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(handle, "quit", "Quit OpenDictate", true, None::<&str>)?;
    let menu = Menu::with_items(handle, &[&dictate, &meeting, &PredefinedMenuItem::separator(handle)?, &open, &quit_item])?;
    TrayIconBuilder::with_id("main")
        .icon(handle.default_window_icon().cloned().expect("app icon"))
        .tooltip("OpenDictate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|handle, event| {
            let app = handle.state::<Arc<App>>();
            match event.id.as_ref() {
                "dictate" => app.toggle_recording(),
                "meeting" => {
                    if app.meetings.is_active() {
                        app.meetings.stop(&app);
                    } else if let Err(e) = app.meetings.start(&app) {
                        app.toast("Couldn't start the meeting", e, "warning");
                    } else {
                        app.show_main_window(Some("meetings"));
                    }
                }
                "open" => app.show_main_window(None),
                "quit" => quit(handle),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                tray.app_handle().state::<Arc<App>>().show_main_window(None);
            }
        })
        .build(tray_handle(handle))?;
    Ok(())
}

fn tray_handle(handle: &AppHandle) -> &AppHandle {
    handle
}

/// Logs to %APPDATA%\OpenDictate\opendictate.log (recreated each launch) so problems can be diagnosed.
/// Never logs transcript text.
fn init_logging() {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,transcribe_rs=warn,ort=warn"));
    let _ = std::fs::create_dir_all(paths::app_dir());
    if let Ok(file) = std::fs::File::create(paths::app_dir().join("opendictate.log")) {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    builder.init();
}

/// Windows 11 throttles background apps (Efficiency mode). A dictation app waits in the tray, so opt out:
/// throttling delays the keyboard hook enough for Windows to skip key events.
fn disable_power_throttling() {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    };
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: 0,
    };
    let result = unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &state as *const _ as *const _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    if let Err(e) = result {
        log::warn!("Couldn't opt out of power throttling: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    disable_power_throttling();
    let minimized = std::env::args().any(|a| a == "--minimized");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|handle, _args, _cwd| {
            handle.state::<Arc<App>>().show_main_window(None);
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |tauri_app| {
            let handle = tauri_app.handle().clone();
            let app = App::new(handle.clone());
            tauri_app.manage(app.clone());
            capsule::create(&handle)?;
            build_tray(&handle)?;
            if let Some(main) = handle.get_webview_window("main") {
                if !minimized {
                    let _ = main.show();
                }
            }
            let _ = handle.emit("ready", ());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the dashboard keeps dictation running from the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_session,
            get_workspace,
            toggle_recording,
            finish_recording,
            cancel_recording,
            dismiss_card,
            capsule_idle,
            select_model,
            install_model,
            cancel_installation,
            uninstall_model,
            save_connection,
            delete_connection,
            connection_has_key,
            new_connection,
            ollama_models,
            cli_catalog,
            cli_installed,
            notes_availability,
            test_notes_model,
            update_settings,
            finish_onboarding,
            show_onboarding,
            begin_shortcut_capture,
            cancel_shortcut_capture,
            open_windows_settings,
            open_data_folder,
            add_vocabulary,
            delete_vocabulary,
            delete_transcript,
            clear_history,
            update_transcript,
            restore_original,
            export_history,
            copy_text,
            get_meetings,
            meeting_start,
            meeting_stop,
            meeting_generate_notes,
            meeting_cancel_notes,
            meeting_update,
            meeting_toggle_action,
            meeting_delete,
            meeting_copy,
            meeting_export,
            meeting_dismiss_message,
            show_main,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenDictate");
}
