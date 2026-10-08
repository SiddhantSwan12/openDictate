//! Session coordination for dictation. Port of BetterWispr `AppModel.swift`.

use crate::credentials::{self, Purpose};
use crate::engine::{model_views, Engine, ModelView, Selected};
use crate::meetings::Meetings;
use crate::paths;
use crate::recorder::{self, Recorder};
use crate::win::{self, clipboard, hotkey, ForegroundApp};
use opendictate_core::corrections::{self, LearnedCorrection};
use opendictate_core::model::{AppSettings, AudioInputDevice, DictationMode, LocalStore, NotesSelection, SavedState, SpeechConnection, Transcript, UsageInsights};
use opendictate_core::style::{AppCategory, CleanupLevel};
use opendictate_core::vocabulary::{self, VocabularyEntry};
use opendictate_core::{cleaner, voice_commands};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const ONBOARDING_VERSION: u32 = 1;
const UNPASTED_CARD_SECONDS: u64 = 5;
const MAX_RECORDING_SECONDS: f64 = 120.0;
const MAX_VOCABULARY: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Phase {
    Idle,
    Preparing,
    Recording,
    Transcribing,
    Failed { title: String, message: String },
    Unpasted { text: String },
}

impl Phase {
    fn failed(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Failed { title: title.into(), message: message.into() }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: String,
    pub progress: f64,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Toast {
    pub title: String,
    pub message: String,
    pub icon: String,
}

/// Live session state, emitted as "session" whenever it changes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub phase: Phase,
    pub status_message: String,
    pub partial_transcript: String,
    pub held: bool,
    pub recording_duration: f64,
    pub installation: Option<Installation>,
    pub loading_model: bool,
}

/// Saved data and catalogs, emitted as "workspace" whenever it changes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub settings: AppSettings,
    pub history: Vec<Transcript>,
    pub vocabulary: Vec<VocabularyEntry>,
    pub models: Vec<ModelView>,
    pub insights: UsageInsights,
    pub microphones: Vec<AudioInputDevice>,
    pub default_microphone: Option<AudioInputDevice>,
    pub needs_onboarding: bool,
    pub storage_readable: bool,
    pub shortcut_name: String,
    pub gpu_name: Option<String>,
}

struct Inner {
    settings: AppSettings,
    history: Vec<Transcript>,
    vocabulary: Vec<VocabularyEntry>,
    storage_readable: bool,
    session: Session,
    recorder: Option<Recorder>,
    target: Option<ForegroundApp>,
    pressed_at: Option<Instant>,
    install_cancel: Option<Arc<AtomicBool>>,
    watcher_stop: Option<Arc<AtomicBool>>,
}

pub struct App {
    pub handle: AppHandle,
    inner: Mutex<Inner>,
    pub engine: Engine,
    generation: AtomicU64,
    store: LocalStore,
    pub meetings: Meetings,
}

impl App {
    pub fn new(handle: AppHandle) -> Arc<Self> {
        let store = LocalStore::new(paths::app_dir());
        let (state, readable, message) = match store.load() {
            Ok(state) => (state, true, String::new()),
            Err(e) => (SavedState::default(), false, format!("Could not read your saved workspace. It has been left untouched: {e}")),
        };
        let mut settings = state.settings;
        if Selected::resolve(&settings, &settings.selected_model_id).is_none() {
            settings.selected_model_id = AppSettings::default().selected_model_id;
        }
        let app = Arc::new(Self {
            meetings: Meetings::new(handle.clone()),
            handle,
            inner: Mutex::new(Inner {
                settings,
                history: state.history,
                vocabulary: state.vocabulary,
                storage_readable: readable,
                session: Session {
                    phase: Phase::Idle,
                    status_message: message,
                    partial_transcript: String::new(),
                    held: false,
                    recording_duration: 0.0,
                    installation: None,
                    loading_model: false,
                },
                recorder: None,
                target: None,
                pressed_at: None,
                install_cancel: None,
                watcher_stop: None,
            }),
            engine: Engine::default(),
            generation: AtomicU64::new(0),
            store,
        });
        app.start_hotkey();
        app.warm_up_selected_model();
        app
    }

    // MARK: Snapshots

    pub fn session(&self) -> Session {
        self.inner.lock().session.clone()
    }

    pub fn workspace(&self) -> Workspace {
        // Device enumeration can be slow; keep it outside the state lock.
        let microphones = recorder::microphones();
        let default_microphone = recorder::default_microphone();
        let inner = self.inner.lock();
        Workspace {
            settings: inner.settings.clone(),
            history: inner.history.clone(),
            vocabulary: inner.vocabulary.clone(),
            models: model_views(&inner.settings),
            insights: UsageInsights::local(&inner.history),
            microphones,
            default_microphone,
            needs_onboarding: inner.storage_readable && inner.settings.completed_onboarding_version < ONBOARDING_VERSION,
            storage_readable: inner.storage_readable,
            shortcut_name: inner.settings.shortcut.display_name(),
            gpu_name: crate::gpu_name(),
        }
    }

    pub fn settings(&self) -> AppSettings {
        self.inner.lock().settings.clone()
    }

    fn emit_session(&self) {
        let session = self.session();
        let _ = self.handle.emit("session", &session);
        crate::capsule::update(&self.handle, &session, self.inner.lock().settings.show_capsule);
    }

    pub fn emit_workspace(&self) {
        let _ = self.handle.emit("workspace", &self.workspace());
    }

    pub fn toast(&self, title: impl Into<String>, message: impl Into<String>, icon: &str) {
        let toast = Toast { title: title.into(), message: message.into(), icon: icon.into() };
        crate::capsule::toast(&self.handle, &toast);
    }

    fn set_phase(&self, phase: Phase, status: Option<String>) {
        {
            let mut inner = self.inner.lock();
            inner.session.phase = phase;
            if let Some(status) = status {
                inner.session.status_message = status;
            }
        }
        self.emit_session();
    }

    pub fn set_status(&self, status: impl Into<String>) {
        self.inner.lock().session.status_message = status.into();
        self.emit_session();
    }

    fn is_busy(&self) -> bool {
        matches!(self.inner.lock().session.phase, Phase::Preparing | Phase::Recording | Phase::Transcribing)
    }

    fn current(&self, token: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == token
    }

    fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    // MARK: Shortcut

    fn start_hotkey(self: &Arc<Self>) {
        let (tx, rx) = std::sync::mpsc::channel();
        hotkey::start(tx);
        hotkey::set_shortcut(self.inner.lock().settings.shortcut.clone());
        let app = self.clone();
        std::thread::spawn(move || {
            for event in rx {
                if !matches!(event, hotkey::HotkeyEvent::Captured(_)) {
                    log::info!("hotkey: {event:?}");
                }
                match event {
                    hotkey::HotkeyEvent::Pressed => app.shortcut_pressed(),
                    hotkey::HotkeyEvent::Released => app.shortcut_released(),
                    hotkey::HotkeyEvent::Interrupted => app.shortcut_interrupted(),
                    hotkey::HotkeyEvent::Captured(shortcut) => {
                        let _ = app.handle.emit("shortcut-captured", &shortcut);
                    }
                    hotkey::HotkeyEvent::CaptureCancelled => {
                        let _ = app.handle.emit("shortcut-capture-cancelled", ());
                    }
                }
            }
        });
    }

    fn shortcut_pressed(self: &Arc<Self>) {
        let shortcut = self.inner.lock().settings.shortcut.clone();
        if shortcut.win || shortcut.alt {
            // Keeps Windows from opening Start or a menu bar when the modifier is released.
            win::send_mask_key();
        }
        let busy = self.is_busy();
        let (pressed, mode) = {
            let inner = self.inner.lock();
            (inner.pressed_at.is_some(), inner.settings.dictation_mode)
        };
        if pressed && busy {
            return;
        }
        if mode != DictationMode::Hold || busy {
            self.toggle_recording();
            return;
        }
        self.inner.lock().pressed_at = Some(Instant::now());
        self.start_recording(true);
    }

    fn shortcut_released(self: &Arc<Self>) {
        let (pressed_at, phase, name) = {
            let mut inner = self.inner.lock();
            (inner.pressed_at.take(), inner.session.phase.clone(), inner.settings.shortcut.display_name())
        };
        let Some(pressed_at) = pressed_at else { return };
        if !matches!(phase, Phase::Preparing | Phase::Recording) {
            return;
        }
        if pressed_at.elapsed() < Duration::from_millis(300) {
            self.abandon(Phase::failed(format!("Don't tap. Hold {name}."), format!("Hold {name} while speaking, release to see text.")));
        } else if phase == Phase::Preparing {
            self.abandon(Phase::failed(format!("Keep holding {name}."), "Wait for the bars to move, then speak and release."));
        } else {
            self.finish_recording();
        }
    }

    /// The user pressed another key with a modifier-only shortcut, so they meant a different combination.
    fn shortcut_interrupted(self: &Arc<Self>) {
        let held = self.inner.lock().pressed_at.take().is_some();
        if held && matches!(self.inner.lock().session.phase, Phase::Preparing | Phase::Recording) {
            self.cancel_recording();
            self.set_status("");
        }
    }

    // MARK: Recording

    pub fn toggle_recording(self: &Arc<Self>) {
        if self.inner.lock().session.phase == Phase::Recording {
            self.finish_recording();
        } else {
            self.start_recording(false);
        }
    }

    fn start_recording(self: &Arc<Self>, held: bool) {
        if self.is_busy() {
            return;
        }
        self.stop_watcher();
        let settings = self.settings();
        let Some(selected) = Selected::resolve(&settings, &settings.selected_model_id) else { return };
        if !selected.is_installed() {
            let _ = self.handle.emit("navigate", "models");
            self.fail(Phase::failed("Download a model first.", format!("Download {} in Models before recording.", selected.name())));
            return;
        }
        let target = win::foreground_app().filter(|a| !win::is_own_process(a.pid));
        log::info!("start: target={target:?}");
        let token = self.next_generation();
        {
            let mut inner = self.inner.lock();
            inner.target = target;
            inner.session.held = held;
            inner.session.partial_transcript.clear();
            inner.session.recording_duration = 0.0;
            inner.session.phase = Phase::Preparing;
            inner.session.status_message = format!("Preparing {}…", selected.name());
        }
        self.emit_session();
        let app = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                app.engine.prepare(&selected, settings.language_code(), settings.use_gpu)?;
                if !app.current(token) {
                    return Ok(());
                }
                let handle = app.handle.clone();
                let recorder = Recorder::start(settings.microphone.clone(), move |level| {
                    let _ = handle.emit_to("capsule", "level", level);
                    let _ = handle.emit_to("main", "level", level);
                })?;
                if !app.current(token) {
                    return Ok(());
                }
                let name = settings.shortcut.display_name();
                {
                    let mut inner = app.inner.lock();
                    inner.recorder = Some(recorder);
                    inner.session.phase = Phase::Recording;
                    inner.session.status_message = if held {
                        format!("Listening. Release {name} to finish.")
                    } else {
                        format!("Listening. Press {name} or click the capsule to finish.")
                    };
                }
                app.emit_session();
                app.tick(token);
                Ok(())
            })();
            if let Err(e) = result {
                if app.current(token) {
                    app.inner.lock().recorder = None;
                    app.fail(Phase::failed("Couldn't start dictation.", e.to_string()));
                }
            }
        });
    }

    /// Publishes the recording time and keeps dictation bounded to two minutes.
    fn tick(self: &Arc<Self>, token: u64) {
        let app = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            if !app.current(token) {
                return;
            }
            let elapsed = {
                let mut inner = app.inner.lock();
                if inner.session.phase != Phase::Recording {
                    return;
                }
                let elapsed = inner.recorder.as_ref().map_or(0.0, Recorder::elapsed);
                inner.session.recording_duration = elapsed;
                elapsed
            };
            let _ = app.handle.emit("recording-duration", elapsed);
            if elapsed >= MAX_RECORDING_SECONDS {
                app.finish_recording();
                return;
            }
        });
    }

    pub fn finish_recording(self: &Arc<Self>) {
        let (recorder, settings, vocabulary, target) = {
            let mut inner = self.inner.lock();
            if inner.session.phase != Phase::Recording {
                return;
            }
            (inner.recorder.take(), inner.settings.clone(), inner.vocabulary.clone(), inner.target.clone())
        };
        let Some(recorder) = recorder else { return };
        let audio = recorder.stop(settings.silence_threshold);
        self.inner.lock().session.recording_duration = audio.duration;
        if !audio.has_speech {
            self.fail(Phase::failed("No speech heard.", "Move closer to your microphone and try again."));
            return;
        }
        let token = self.generation.load(Ordering::SeqCst);
        let Some(selected) = Selected::resolve(&settings, &settings.selected_model_id) else { return };
        let status = if selected.is_api() { format!("Transcribing with {}…", selected.name()) } else { "Transcribing on this PC…".into() };
        self.set_phase(Phase::Transcribing, Some(status));
        let app = self.clone();
        std::thread::spawn(move || {
            let result = app.transcribe_and_deliver(token, &selected, &audio.samples, audio.duration, &settings, &vocabulary, target);
            if let Err(e) = result {
                if app.current(token) {
                    app.fail(Phase::failed("Couldn't transcribe.", e.to_string()));
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn transcribe_and_deliver(
        self: &Arc<Self>,
        token: u64,
        selected: &Selected,
        samples: &[f32],
        duration: f64,
        settings: &AppSettings,
        entries: &[VocabularyEntry],
        target: Option<ForegroundApp>,
    ) -> anyhow::Result<()> {
        let language = settings.language_code();
        let hints: Vec<String> =
            entries.iter().map(|e| if e.replacement.is_empty() { e.phrase.clone() } else { e.replacement.clone() }).collect();
        let raw = self.engine.transcribe(selected, samples, language, &hints, settings.use_gpu)?;
        if !self.current(token) {
            return Ok(());
        }
        let app_exe = target.as_ref().and_then(|t| t.exe.clone());
        let tone = settings.tone(AppCategory::from_app(app_exe.as_deref()).style());
        let text = self.process(&raw, settings, language, tone, token);
        if !self.current(token) {
            return Ok(());
        }
        let (text, fixes) = vocabulary::corrected(entries, &text);
        if text.is_empty() {
            self.fail(Phase::failed("No speech heard.", "Move closer to your microphone and try again."));
            return Ok(());
        }
        self.inner.lock().session.partial_transcript = text.clone();
        let mut transcript =
            Transcript::new(text.clone(), raw, duration, selected.name(), language.unwrap_or("auto").to_string());
        transcript.app = app_exe;
        transcript.vocabulary_fixes = Some(fixes);
        let mut warning = None;
        if settings.save_history {
            self.inner.lock().history.insert(0, transcript);
            if let Err(e) = self.persist() {
                warning = Some(format!("History could not be saved: {e}"));
            }
            self.emit_workspace();
        }
        let result = deliver(&text, target.as_ref(), settings.auto_paste, settings.copy_to_clipboard)?;
        if !self.current(token) {
            return Ok(());
        }
        match result {
            Delivery::Blocked => self.fail(Phase::failed(
                "Copied, not pasted.",
                "The app you're dictating into runs as administrator, so Windows blocks pasting. Press Ctrl+V.",
            )),
            Delivery::CopiedForManualPaste => self.present(Phase::Unpasted { text }, UNPASTED_CARD_SECONDS, String::new()),
            Delivery::Pasted => {
                self.set_phase(Phase::Idle, Some(String::new()));
                if settings.learn_corrections {
                    if let Some(target) = target {
                        self.watch_corrections(text, target.pid);
                    }
                }
            }
            Delivery::Copied => {
                self.set_phase(Phase::Idle, Some(String::new()));
                self.toast("Copied", "Your dictation is on the clipboard.", "copy");
            }
            Delivery::Nothing => self.set_phase(Phase::Idle, Some(String::new())),
        }
        if let Some(warning) = warning {
            self.set_status(warning);
        }
        Ok(())
    }

    /// Cleanup, voice commands, optional AI edit and writing tone, in BetterWispr's fixed order.
    fn process(&self, raw: &str, settings: &AppSettings, language: Option<&str>, tone: opendictate_core::style::StyleTone, token: u64) -> String {
        let cleaned = if settings.cleanup == CleanupLevel::None { raw.trim().to_string() } else { cleaner::clean(raw, language) };
        let mut spoken = if language.is_none_or(|l| l.starts_with("en")) { voice_commands::apply(&cleaned) } else { cleaned };
        let english = cleaner::is_english(&spoken, language);
        if english && settings.cleanup == CleanupLevel::Medium && opendictate_notes::availability(settings).is_ok() {
            self.set_status(format!("Editing with {}…", settings.notes_model_name()));
            let key = |c: &SpeechConnection| credentials::read(Purpose::Notes, c).ok().flatten();
            if let Ok(model) = opendictate_notes::language_model(settings, opendictate_core::polish::INSTRUCTIONS, &key) {
                if let Some(polished) = opendictate_notes::polish(&spoken, model.as_ref()) {
                    if self.current(token) {
                        spoken = polished;
                    }
                }
            }
        }
        if english {
            spoken = opendictate_core::style::apply(tone, &spoken);
        }
        spoken
    }

    pub fn cancel_recording(self: &Arc<Self>) {
        self.next_generation();
        self.stop_watcher();
        {
            let mut inner = self.inner.lock();
            inner.recorder = None;
            inner.session.partial_transcript.clear();
            inner.session.phase = Phase::Idle;
            inner.session.status_message = "Cancelled.".into();
        }
        self.emit_session();
    }

    fn abandon(self: &Arc<Self>, failure: Phase) {
        self.cancel_recording();
        self.fail(failure);
    }

    fn fail(self: &Arc<Self>, failure: Phase) {
        let message = match &failure {
            Phase::Failed { message, .. } => message.clone(),
            _ => String::new(),
        };
        self.present(failure, 6, message);
    }

    /// Shows a capsule card that closes itself unless another phase replaced it first.
    fn present(self: &Arc<Self>, card: Phase, seconds: u64, status: String) {
        self.set_phase(card.clone(), Some(status));
        let app = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds));
            let mut inner = app.inner.lock();
            if inner.session.phase == card {
                inner.session.phase = Phase::Idle;
                drop(inner);
                app.emit_session();
            }
        });
    }

    pub fn dismiss_card(&self) {
        let mut inner = self.inner.lock();
        if matches!(inner.session.phase, Phase::Failed { .. } | Phase::Unpasted { .. }) {
            inner.session.phase = Phase::Idle;
            drop(inner);
            self.emit_session();
        }
    }

    // MARK: Correction learning

    fn watch_corrections(self: &Arc<Self>, text: String, pid: u32) {
        let stop = Arc::new(AtomicBool::new(false));
        self.inner.lock().watcher_stop = Some(stop.clone());
        let app = self.clone();
        win::uia::watch(text, pid, stop, move |found| app.learn(found));
    }

    fn stop_watcher(&self) {
        if let Some(stop) = self.inner.lock().watcher_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn learn(&self, found: Vec<LearnedCorrection>) {
        let mut added = Vec::new();
        {
            let mut inner = self.inner.lock();
            let known: std::collections::HashSet<String> =
                inner.vocabulary.iter().flat_map(|e| [e.phrase.to_lowercase(), e.replacement.to_lowercase()]).collect();
            for correction in found {
                if inner.vocabulary.len() >= MAX_VOCABULARY
                    || known.contains(&correction.heard.to_lowercase())
                    || known.contains(&correction.corrected.to_lowercase())
                {
                    continue;
                }
                inner.vocabulary.push(if corrections::is_common(&correction.heard) {
                    VocabularyEntry::new(&correction.corrected, "", true)
                } else {
                    VocabularyEntry::new(&correction.heard, &correction.corrected, true)
                });
                added.push(format!("“{}”", correction.corrected));
            }
        }
        if added.is_empty() {
            return;
        }
        if self.persist().is_ok() {
            self.emit_workspace();
            self.toast(
                format!("Learned {}", added.join(", ")),
                "OpenDictate will spell it this way next time. Manage it in Vocabulary.",
                "sparkles",
            );
        }
    }

    // MARK: Models

    pub fn warm_up_selected_model(self: &Arc<Self>) {
        let settings = self.settings();
        let Some(Selected::Local(model)) = Selected::resolve(&settings, &settings.selected_model_id) else { return };
        if !opendictate_speech::is_installed(&paths::models_dir(), &model) || self.engine.loaded_id().as_deref() == Some(&model.id) {
            return;
        }
        let app = self.clone();
        std::thread::spawn(move || {
            app.inner.lock().session.loading_model = true;
            app.emit_session();
            if let Err(e) = app.engine.ensure_loaded(&model, settings.use_gpu) {
                app.set_status(format!("Couldn't load {}: {e}", model.name));
            }
            app.inner.lock().session.loading_model = false;
            app.emit_session();
        });
    }

    pub fn select_model(self: &Arc<Self>, id: &str) -> Result<(), String> {
        if self.is_busy() || self.meetings.is_active() {
            return Err("Finish the current dictation or meeting first.".into());
        }
        {
            let mut inner = self.inner.lock();
            if Selected::resolve(&inner.settings, id).is_none() {
                return Err("Unknown model.".into());
            }
            inner.settings.selected_model_id = id.to_string();
        }
        self.engine.unload();
        self.persist()?;
        self.emit_workspace();
        self.warm_up_selected_model();
        Ok(())
    }

    /// Downloads a local model, then switches to it unless the user chose another model meanwhile.
    pub fn install_model(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let model = opendictate_speech::find_model(id).ok_or("Unknown model.")?;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock();
            if inner.session.installation.as_ref().is_some_and(|i| i.failure.is_none()) {
                return Err("Another download is running.".into());
            }
            inner.session.installation = Some(Installation { id: id.into(), progress: 0.0, failure: None });
            inner.session.status_message = format!("Downloading {}…", model.name);
            inner.install_cancel = Some(cancel.clone());
        }
        self.emit_session();
        let selection = self.settings().selected_model_id;
        let use_gpu = self.settings().use_gpu;
        let app = self.clone();
        std::thread::spawn(move || {
            let last = Mutex::new(Instant::now());
            let progress = |p: f64| {
                let mut inner = app.inner.lock();
                if let Some(installation) = inner.session.installation.as_mut() {
                    installation.progress = installation.progress.max(p.min(1.0));
                }
                drop(inner);
                let mut last = last.lock();
                if last.elapsed() > Duration::from_millis(150) || p >= 1.0 {
                    *last = Instant::now();
                    app.emit_session();
                }
            };
            let result = opendictate_speech::install(&paths::models_dir(), &model, use_gpu, &cancel, &progress);
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            match result {
                Ok(()) => {
                    app.inner.lock().session.installation = None;
                    if app.settings().selected_model_id == selection && !app.is_busy() && !app.meetings.is_active() {
                        let _ = app.select_model(&model.id);
                        app.set_status(format!("{} is ready for offline dictation.", model.name));
                    } else {
                        app.set_status(format!("{} is installed. Choose Use in Models to switch to it.", model.name));
                    }
                }
                Err(e) => {
                    let message = format!("{e:#}");
                    if let Some(installation) = app.inner.lock().session.installation.as_mut() {
                        installation.failure = Some(message.clone());
                    }
                    app.set_status(format!("Couldn't download {}: {message}", model.name));
                }
            }
            app.emit_workspace();
            app.emit_session();
        });
        Ok(())
    }

    pub fn cancel_installation(&self) {
        let mut inner = self.inner.lock();
        if let Some(cancel) = inner.install_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        inner.session.installation = None;
        inner.session.status_message = "Download cancelled.".into();
        drop(inner);
        self.emit_session();
    }

    pub fn uninstall_model(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let model = opendictate_speech::find_model(id).ok_or("Unknown model.")?;
        if self.settings().selected_model_id == id {
            if self.is_busy() || self.meetings.is_active() {
                return Err("Finish the current dictation or meeting first.".into());
            }
            self.engine.unload();
        }
        opendictate_speech::uninstall(&paths::models_dir(), &model).map_err(|e| e.to_string())?;
        self.emit_workspace();
        Ok(())
    }

    // MARK: Connections

    pub fn save_connection(self: &Arc<Self>, connection: SpeechConnection, key: Option<String>, for_notes: bool) -> Result<(), String> {
        if self.is_busy() || self.meetings.is_active() {
            return Err("Finish the current dictation or meeting first.".into());
        }
        connection.validate()?;
        let purpose = if for_notes { Purpose::Notes } else { Purpose::Speech };
        let previous_key = credentials::read(purpose, &connection)?;
        let new_key = key.or(previous_key.clone()).unwrap_or_default();
        connection.validate_key(&new_key)?;
        let previous_settings = self.settings();
        let list = if for_notes { &previous_settings.notes_connections } else { &previous_settings.speech_connections };
        let old = list.iter().find(|c| c.id == connection.id).cloned();
        credentials::save(purpose, &connection, &new_key)?;
        {
            let mut inner = self.inner.lock();
            let settings = &mut inner.settings;
            let connections = if for_notes { &mut settings.notes_connections } else { &mut settings.speech_connections };
            match connections.iter_mut().find(|c| c.id == connection.id) {
                Some(existing) => *existing = connection.clone(),
                None => connections.push(connection.clone()),
            }
            if let Some(old) = &old {
                // A key never follows a connection to a different destination.
                if old.credential_account() != connection.credential_account() {
                    if for_notes && settings.notes_selection == NotesSelection::Connection(connection.id.clone()) {
                        settings.notes_selection = NotesSelection::None;
                    }
                    if !for_notes && settings.selected_model_id == connection.speech_model_id() {
                        settings.selected_model_id = AppSettings::default().selected_model_id;
                    }
                }
            }
        }
        if let Err(e) = self.persist() {
            self.inner.lock().settings = previous_settings;
            let _ = credentials::save(purpose, &connection, &previous_key.unwrap_or_default());
            return Err(e);
        }
        if let Some(old) = old.filter(|o| o.credential_account() != connection.credential_account()) {
            let _ = credentials::save(purpose, &old, "");
        }
        self.emit_workspace();
        Ok(())
    }

    pub fn delete_connection(self: &Arc<Self>, id: &str, for_notes: bool) -> Result<(), String> {
        if self.is_busy() || self.meetings.is_active() {
            return Err("Finish the current dictation or meeting first.".into());
        }
        let purpose = if for_notes { Purpose::Notes } else { Purpose::Speech };
        let connection = {
            let inner = self.inner.lock();
            let list = if for_notes { &inner.settings.notes_connections } else { &inner.settings.speech_connections };
            list.iter().find(|c| c.id == id).cloned().ok_or("Unknown connection.")?
        };
        credentials::save(purpose, &connection, "")?;
        {
            let mut inner = self.inner.lock();
            let settings = &mut inner.settings;
            if for_notes {
                settings.notes_connections.retain(|c| c.id != id);
                if settings.notes_selection == NotesSelection::Connection(id.into()) {
                    settings.notes_selection = NotesSelection::None;
                }
            } else {
                settings.speech_connections.retain(|c| c.id != id);
                if settings.selected_model_id == connection.speech_model_id() {
                    settings.selected_model_id = AppSettings::default().selected_model_id;
                }
            }
        }
        self.persist()?;
        self.emit_workspace();
        self.warm_up_selected_model();
        Ok(())
    }

    pub fn has_key(&self, connection: &SpeechConnection, for_notes: bool) -> bool {
        let purpose = if for_notes { Purpose::Notes } else { Purpose::Speech };
        credentials::read(purpose, connection).ok().flatten().is_some_and(|k| !k.is_empty())
    }

    // MARK: Settings

    /// Replaces settings from the UI, applying side effects such as the shortcut and launch at login.
    pub fn update_settings(self: &Arc<Self>, mut next: AppSettings) -> Result<(), String> {
        let previous = self.settings();
        // Connections and model selection have their own commands with validation.
        next.speech_connections = previous.speech_connections.clone();
        next.notes_connections = previous.notes_connections.clone();
        next.selected_model_id = previous.selected_model_id.clone();
        if next.shortcut != previous.shortcut {
            if self.is_busy() {
                return Err("Finish the current dictation first.".into());
            }
            if !next.shortcut.is_valid() {
                return Err("Use a shortcut with Ctrl, Alt or Win plus a key, a function key, or two modifiers held together.".into());
            }
            hotkey::set_shortcut(next.shortcut.clone());
        }
        if next.launch_at_login != previous.launch_at_login {
            use tauri_plugin_autostart::ManagerExt;
            let manager = self.handle.autolaunch();
            let result = if next.launch_at_login { manager.enable() } else { manager.disable() };
            if let Err(e) = result {
                next.launch_at_login = previous.launch_at_login;
                self.set_status(format!("Could not change launch at login: {e}"));
            }
        }
        let gpu_changed = next.use_gpu != previous.use_gpu;
        self.inner.lock().settings = next;
        if let Err(e) = self.persist() {
            self.inner.lock().settings = previous;
            return Err(e);
        }
        if gpu_changed {
            self.engine.unload();
            self.warm_up_selected_model();
        }
        self.emit_workspace();
        // Notes-model availability is part of the meetings view.
        self.meetings.emit(self);
        Ok(())
    }

    pub fn finish_onboarding(self: &Arc<Self>) -> Result<(), String> {
        let mut settings = self.settings();
        settings.completed_onboarding_version = ONBOARDING_VERSION;
        settings.onboarding_step = 0;
        self.update_settings(settings)
    }

    pub fn show_onboarding(self: &Arc<Self>) -> Result<(), String> {
        let mut settings = self.settings();
        settings.completed_onboarding_version = 0;
        settings.onboarding_step = 0;
        self.update_settings(settings)
    }

    // MARK: History and vocabulary

    pub fn delete_transcript(&self, id: &str) -> Result<(), String> {
        let previous = self.inner.lock().history.clone();
        self.inner.lock().history.retain(|t| t.id != id);
        if let Err(e) = self.persist() {
            self.inner.lock().history = previous;
            return Err(e);
        }
        self.emit_workspace();
        self.toast("Deleted", "The dictation was removed from history.", "trash");
        Ok(())
    }

    pub fn clear_history(&self) -> Result<(), String> {
        let previous = std::mem::take(&mut self.inner.lock().history);
        if let Err(e) = self.persist() {
            self.inner.lock().history = previous;
            return Err(e);
        }
        self.emit_workspace();
        Ok(())
    }

    pub fn update_transcript(&self, id: &str, text: &str) -> Result<(), String> {
        let text = text.trim().to_string();
        let original = {
            let mut inner = self.inner.lock();
            let Some(transcript) = inner.history.iter_mut().find(|t| t.id == id) else { return Ok(()) };
            if text.is_empty() || text == transcript.text {
                return Ok(());
            }
            std::mem::replace(&mut transcript.text, text.clone())
        };
        if let Err(e) = self.persist() {
            if let Some(t) = self.inner.lock().history.iter_mut().find(|t| t.id == id) {
                t.text = original;
            }
            return Err(e);
        }
        self.emit_workspace();
        if self.settings().learn_corrections {
            self.learn(corrections::corrections(&original, &text));
        }
        Ok(())
    }

    pub fn restore_original(&self, id: &str) -> Result<(), String> {
        {
            let mut inner = self.inner.lock();
            let Some(transcript) = inner.history.iter_mut().find(|t| t.id == id) else { return Ok(()) };
            let original = transcript.raw_text.trim().to_string();
            if original.is_empty() || transcript.text == original {
                return Ok(());
            }
            transcript.text = original;
        }
        self.persist()?;
        self.emit_workspace();
        Ok(())
    }

    pub fn add_vocabulary(&self, phrase: &str, replacement: &str) -> Result<(), String> {
        let phrase = phrase.trim();
        let replacement = replacement.trim();
        if phrase.is_empty() || phrase.chars().count() > 100 || replacement.chars().count() > 100 {
            return Err("Use a phrase and spelling of at most 100 characters.".into());
        }
        {
            let mut inner = self.inner.lock();
            if inner.vocabulary.len() >= MAX_VOCABULARY {
                return Err("Keep your vocabulary to 200 entries for focused hints.".into());
            }
            if inner.vocabulary.iter().any(|e| e.phrase.to_lowercase() == phrase.to_lowercase()) {
                return Err("That phrase is already in your vocabulary.".into());
            }
            inner.vocabulary.push(VocabularyEntry::new(phrase, replacement, false));
        }
        self.persist()?;
        self.emit_workspace();
        Ok(())
    }

    pub fn delete_vocabulary(&self, id: &str) -> Result<(), String> {
        let removed = {
            let mut inner = self.inner.lock();
            let index = inner.vocabulary.iter().position(|e| e.id == id).ok_or("Unknown word.")?;
            inner.vocabulary.remove(index)
        };
        self.persist()?;
        self.emit_workspace();
        self.toast("Deleted", format!("“{}” was removed from your vocabulary.", removed.phrase), "trash");
        Ok(())
    }

    pub fn export_history(&self, path: &str) -> Result<(), String> {
        let history = self.inner.lock().history.clone();
        let json = serde_json::to_vec_pretty(&history).map_err(|e| e.to_string())?;
        opendictate_core::model::write_atomic(std::path::Path::new(path), &json)?;
        self.set_status("History exported.");
        Ok(())
    }

    pub fn copy_text(&self, text: &str) {
        match clipboard::set_text(text) {
            Ok(()) => self.toast("Copied", "The text is on your clipboard.", "copy"),
            Err(e) => self.toast("Couldn't copy", e, "warning"),
        }
    }

    pub fn persist(&self) -> Result<(), String> {
        let inner = self.inner.lock();
        if !inner.storage_readable {
            return Err(format!(
                "The existing workspace could not be read, so it will not be overwritten. Back it up in {} and restart.",
                self.store.directory.display()
            ));
        }
        let state = SavedState {
            version: 1,
            settings: inner.settings.clone(),
            history: inner.history.clone(),
            vocabulary: inner.vocabulary.clone(),
        };
        drop(inner);
        self.store.save(&state)
    }

    pub fn show_main_window(&self, page: Option<&str>) {
        if let Some(window) = self.handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        if let Some(page) = page {
            let _ = self.handle.emit("navigate", page);
        }
    }
}

pub enum Delivery {
    Copied,
    CopiedForManualPaste,
    /// The target runs as administrator; Windows drops synthetic input into it.
    Blocked,
    Pasted,
    Nothing,
}

/// Pastes only while the app that had focus at the start still has it; otherwise copies for manual paste.
/// With "Copy to clipboard" off, restores the previous clipboard unless something newer replaced it.
fn deliver(text: &str, target: Option<&ForegroundApp>, auto_paste: bool, keep_copy: bool) -> anyhow::Result<Delivery> {
    if !auto_paste {
        if keep_copy {
            clipboard::set_text(text).map_err(anyhow::Error::msg)?;
            return Ok(Delivery::Copied);
        }
        return Ok(Delivery::Nothing);
    }
    log::info!("deliver: target={target:?} foreground={:?}", win::foreground_app());
    let Some(target) = target.filter(|t| win::still_focused(t)) else {
        clipboard::set_text(text).map_err(anyhow::Error::msg)?;
        return Ok(Delivery::CopiedForManualPaste);
    };
    if !win::can_send_input_to(target.pid) {
        clipboard::set_text(text).map_err(anyhow::Error::msg)?;
        return Ok(Delivery::Blocked);
    }
    let previous = (!keep_copy).then(clipboard::snapshot);
    clipboard::set_text(text).map_err(anyhow::Error::msg)?;
    let ours = clipboard::sequence();
    let released = win::wait_for_modifiers_released(Duration::from_millis(1500));
    log::info!("deliver: modifiers released={released} foreground={:?}", win::foreground_app());
    if !win::still_focused(target) || !win::send_paste() {
        return Ok(Delivery::CopiedForManualPaste);
    }
    if let Some(previous) = previous {
        // Apps don't acknowledge a paste. Electron apps (VS Code, Slack, Discord) read the clipboard
        // asynchronously, so allow 1.5 s before restoring.
        std::thread::sleep(Duration::from_millis(1500));
        if clipboard::sequence() == ours {
            let _ = clipboard::restore(&previous);
        }
    }
    Ok(Delivery::Pasted)
}
