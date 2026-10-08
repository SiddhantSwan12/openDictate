//! Meeting notes: records the microphone as "Me" and other apps' audio (WASAPI loopback) as "Them",
//! transcribes 10–30 second chunks in order and writes notes with the chosen notes model.
//! Port of BetterWispr `MeetingModel.swift` and `MeetingRecorder.swift`; chunks stay in memory.

use crate::app::App;
use crate::credentials::{self, Purpose};
use crate::engine::Selected;
use crate::paths;
use crate::recorder::{self, Capture, Source};
use opendictate_core::model::{AppSettings, Meeting, MeetingSegment, MeetingStore, SpeechConnection, Speaker};
use opendictate_core::vocabulary::{self, VocabularyEntry};
use opendictate_core::cleaner;
use opendictate_speech::audio::{rms, to_speech_format, ChunkPolicy};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "id")]
pub enum Activity {
    Idle,
    Starting(String),
    Recording(String),
    Finishing(String),
    Generating(String),
}

impl Activity {
    fn meeting_id(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Starting(id) | Self::Recording(id) | Self::Finishing(id) | Self::Generating(id) => Some(id),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingsView {
    pub meetings: Vec<Meeting>,
    pub activity: Activity,
    pub elapsed: f64,
    pub pending_chunks: usize,
    pub generation_step: Option<(usize, usize)>,
    pub message: Option<String>,
    pub system_audio_issue: Option<String>,
    pub shows_call_audio_hint: bool,
    pub microphone: Option<String>,
    pub notes_availability: Option<String>,
    pub summaries_needing_update: Vec<String>,
}

struct Chunk {
    speaker: Speaker,
    start: f64,
    duration: f64,
    samples: Vec<f32>,
    rate: u32,
}

/// Splits one source's audio into chunks at pauses. Runs inside the audio callback, so it only does arithmetic.
struct ChunkWriter {
    speaker: Speaker,
    rate: u32,
    threshold: f32,
    policy: ChunkPolicy,
    start_offset: f64,
    elapsed_frames: usize,
    samples: Vec<f32>,
    active_frames: usize,
    silent_frames: usize,
    out: Sender<Chunk>,
    level: Arc<Mutex<f32>>,
}

impl ChunkWriter {
    fn seconds(&self, frames: usize) -> f64 {
        frames as f64 / self.rate as f64
    }

    fn write(&mut self, block: &[f32]) {
        let level = rms(block);
        *self.level.lock() = level;
        self.samples.extend_from_slice(block);
        if level >= self.threshold {
            self.active_frames += block.len();
            self.silent_frames = 0;
        } else {
            self.silent_frames += block.len();
        }
        if self.policy.should_rotate(self.seconds(self.samples.len()), self.seconds(self.silent_frames)) {
            self.close();
        }
    }

    fn close(&mut self) {
        let frames = self.samples.len();
        if frames > 0 {
            let duration = self.seconds(frames);
            // An amplitude gate decides speech; chunks without it are dropped unread.
            if self.seconds(self.active_frames) >= 0.3 && duration >= 0.5 {
                let _ = self.out.send(Chunk {
                    speaker: self.speaker,
                    start: self.start_offset + self.seconds(self.elapsed_frames),
                    duration,
                    samples: std::mem::take(&mut self.samples),
                    rate: self.rate,
                });
            }
        }
        self.samples.clear();
        self.elapsed_frames += frames;
        self.active_frames = 0;
        self.silent_frames = 0;
    }
}

struct Inner {
    meetings: Vec<Meeting>,
    activity: Activity,
    started_at: Instant,
    elapsed: f64,
    pending_chunks: usize,
    generation_step: Option<(usize, usize)>,
    message: Option<String>,
    system_audio_issue: Option<String>,
    shows_call_audio_hint: bool,
    heard: (bool, bool),
    mic: Option<(Capture, Arc<Mutex<ChunkWriter>>)>,
    them: Option<(Capture, Arc<Mutex<ChunkWriter>>)>,
    chunks: Option<Sender<Chunk>>,
    microphone: Option<String>,
    cancel_notes: Option<Arc<AtomicBool>>,
}

pub struct Meetings {
    handle: AppHandle,
    store: MeetingStore,
    inner: Mutex<Inner>,
    session: AtomicU64,
    levels: (Arc<Mutex<f32>>, Arc<Mutex<f32>>),
}

impl Meetings {
    pub fn new(handle: AppHandle) -> Self {
        let store = MeetingStore::new(paths::meetings_dir());
        let (meetings, unreadable) = store.load_all();
        let message = (!unreadable.is_empty()).then(|| {
            let n = unreadable.len();
            format!(
                "{n} meeting {} in {} couldn't be read and {} left untouched.",
                if n == 1 { "file" } else { "files" },
                store.directory.display(),
                if n == 1 { "was" } else { "were" }
            )
        });
        Self {
            handle,
            store,
            inner: Mutex::new(Inner {
                meetings,
                activity: Activity::Idle,
                started_at: Instant::now(),
                elapsed: 0.0,
                pending_chunks: 0,
                generation_step: None,
                message,
                system_audio_issue: None,
                shows_call_audio_hint: false,
                heard: (false, false),
                mic: None,
                them: None,
                chunks: None,
                microphone: None,
                cancel_notes: None,
            }),
            session: AtomicU64::new(0),
            levels: (Arc::new(Mutex::new(0.0)), Arc::new(Mutex::new(0.0))),
        }
    }

    pub fn is_capturing(&self) -> bool {
        matches!(self.inner.lock().activity, Activity::Starting(_) | Activity::Recording(_))
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().activity != Activity::Idle
    }

    pub fn view(&self, settings: &AppSettings) -> MeetingsView {
        let inner = self.inner.lock();
        MeetingsView {
            summaries_needing_update: inner.meetings.iter().filter(|m| m.summary_needs_update()).map(|m| m.id.clone()).collect(),
            meetings: inner.meetings.clone(),
            activity: inner.activity.clone(),
            elapsed: inner.elapsed,
            pending_chunks: inner.pending_chunks,
            generation_step: inner.generation_step,
            message: inner.message.clone(),
            system_audio_issue: inner.system_audio_issue.clone(),
            shows_call_audio_hint: inner.shows_call_audio_hint,
            microphone: inner.microphone.clone(),
            notes_availability: opendictate_notes::availability(settings).err(),
        }
    }

    pub fn emit(&self, app: &App) {
        let view = self.view(&app.settings());
        let capturing = matches!(view.activity, Activity::Starting(_) | Activity::Recording(_));
        let _ = self.handle.emit("meetings", &view);
        crate::capsule::meeting(&self.handle, capturing);
    }

    fn current(&self, token: u64) -> bool {
        self.session.load(Ordering::SeqCst) == token
    }

    fn save_now(&self, id: &str) {
        let meeting = self.inner.lock().meetings.iter().find(|m| m.id == id).cloned();
        if let Some(meeting) = meeting {
            if let Err(e) = self.store.save(&meeting) {
                self.inner.lock().message = Some(format!("This meeting couldn't be saved. {e}"));
            }
        }
    }

    fn edit(&self, id: &str, change: impl FnOnce(&mut Meeting)) {
        if let Some(meeting) = self.inner.lock().meetings.iter_mut().find(|m| m.id == id) {
            change(meeting);
        }
    }

    pub fn start(&self, app: &Arc<App>) -> Result<String, String> {
        if self.is_active() {
            return Err("A meeting is already running.".into());
        }
        let settings = app.settings();
        let selected = Selected::resolve(&settings, &settings.selected_model_id).ok_or("Choose a speech model in Models.")?;
        if !selected.is_installed() {
            return Err(format!("Download {} in Models first.", selected.name()));
        }
        let token = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        let meeting = Meeting::new(selected.name(), settings.language.clone());
        let id = meeting.id.clone();
        {
            let mut inner = self.inner.lock();
            inner.meetings.insert(0, meeting);
            inner.activity = Activity::Starting(id.clone());
            inner.message = None;
            inner.system_audio_issue = None;
            inner.shows_call_audio_hint = false;
            inner.heard = (false, false);
            inner.elapsed = 0.0;
            inner.pending_chunks = 0;
        }
        self.save_now(&id);
        self.emit(app);
        let app = app.clone();
        let meeting_id = id.clone();
        std::thread::spawn(move || {
            let meetings = &app.meetings;
            let result = (|| -> anyhow::Result<()> {
                app.engine.prepare(&selected, settings.language_code(), settings.use_gpu)?;
                if !meetings.current(token) {
                    return Ok(());
                }
                let (tx, rx) = channel::<Chunk>();
                meetings.open_microphone(&settings, tx.clone(), 0.0)?;
                // Loopback capture of what other apps play; if it fails, record the microphone only.
                let them = ChunkWriter::new(Speaker::Them, &settings, tx.clone(), 0.0, meetings.levels.1.clone());
                match open_writer(Source::SystemOutput, them) {
                    Ok(pair) => meetings.inner.lock().them = Some(pair),
                    Err(e) => {
                        meetings.inner.lock().system_audio_issue =
                            Some(format!("Recording your microphone only. Other apps' audio couldn't be captured: {e}"));
                    }
                }
                {
                    let mut inner = meetings.inner.lock();
                    inner.chunks = Some(tx);
                    inner.started_at = Instant::now();
                    inner.activity = Activity::Recording(meeting_id.clone());
                }
                meetings.emit(&app);
                meetings.spawn_transcriber(&app, rx, meeting_id.clone(), token, selected.clone(), settings.clone());
                meetings.tick(&app, token);
                Ok(())
            })();
            if let Err(e) = result {
                if meetings.current(token) {
                    let mut inner = meetings.inner.lock();
                    inner.mic = None;
                    inner.them = None;
                    inner.activity = Activity::Idle;
                    inner.message = Some(format!("Couldn't start the meeting. {e}"));
                    drop(inner);
                    meetings.discard_if_empty(&meeting_id);
                    meetings.emit(&app);
                }
            }
        });
        Ok(id)
    }

    fn open_microphone(&self, settings: &AppSettings, tx: Sender<Chunk>, offset: f64) -> anyhow::Result<()> {
        let writer = ChunkWriter::new(Speaker::Me, settings, tx, offset, self.levels.0.clone());
        let pair = open_writer(Source::Microphone(settings.microphone.clone()), writer)?;
        let mut inner = self.inner.lock();
        inner.microphone = Some(pair.0.device_name.clone());
        inner.mic = Some(pair);
        Ok(())
    }

    /// Publishes elapsed time and levels, shows the call-audio hint, and follows microphone changes.
    fn tick(&self, app: &Arc<App>, token: u64) {
        let app = app.clone();
        std::thread::spawn(move || {
            let meetings = &app.meetings;
            let mut last_device_check = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(100));
                if !meetings.current(token) {
                    return;
                }
                let me = *meetings.levels.0.lock();
                let them = *meetings.levels.1.lock();
                let _ = app.handle.emit("meeting-levels", ((me * 8.0).min(1.0), (them * 8.0).min(1.0)));
                let mut inner = meetings.inner.lock();
                if !matches!(inner.activity, Activity::Recording(_)) {
                    return;
                }
                if me > 0.01 {
                    inner.heard.0 = true;
                }
                if them > 0.002 {
                    inner.heard.1 = true;
                }
                inner.elapsed = inner.started_at.elapsed().as_secs_f64();
                inner.shows_call_audio_hint =
                    inner.elapsed >= 20.0 && inner.heard.0 && !inner.heard.1 && inner.system_audio_issue.is_none();
                let rebuild = last_device_check.elapsed() > Duration::from_secs(2) && {
                    last_device_check = Instant::now();
                    // Follow the microphone chosen now, so changing it mid-meeting takes effect.
                    let chosen = app.settings().microphone;
                    let resolved = recorder::resolve(chosen.as_ref(), &recorder::microphones(), recorder::default_microphone());
                    resolved.is_some_and(|r| inner.microphone.as_deref() != Some(r.id.as_str()))
                };
                let elapsed = inner.elapsed;
                drop(inner);
                if rebuild {
                    // Close the current "Me" chunk first so offsets stay continuous.
                    let old = meetings.inner.lock().mic.take();
                    let offset = old.map(|(capture, writer)| {
                        drop(capture);
                        let mut w = writer.lock();
                        w.close();
                        w.start_offset + w.seconds(w.elapsed_frames)
                    });
                    let tx = meetings.inner.lock().chunks.clone();
                    if let Some(tx) = tx {
                        let current = app.settings();
                        if let Err(e) = meetings.open_microphone(&current, tx, offset.unwrap_or(elapsed)) {
                            meetings.inner.lock().message = Some(format!("The microphone changed and couldn't be reopened: {e}"));
                        }
                    }
                }
                if (elapsed * 10.0) as u64 % 5 == 0 {
                    meetings.emit(&app);
                }
            }
        });
    }

    fn spawn_transcriber(
        &self,
        app: &Arc<App>,
        rx: std::sync::mpsc::Receiver<Chunk>,
        id: String,
        token: u64,
        selected: Selected,
        settings: AppSettings,
    ) {
        let app = app.clone();
        let vocabulary: Vec<VocabularyEntry> = app.workspace().vocabulary;
        std::thread::spawn(move || {
            let meetings = &app.meetings;
            let language = settings.language_code();
            let hints: Vec<String> =
                vocabulary.iter().map(|e| if e.replacement.is_empty() { e.phrase.clone() } else { e.replacement.clone() }).collect();
            for chunk in rx {
                if !meetings.current(token) {
                    return;
                }
                meetings.inner.lock().pending_chunks += 1;
                meetings.emit(&app);
                let samples = to_speech_format(&chunk.samples, 1, chunk.rate);
                match app.engine.transcribe(&selected, &samples, language, &hints, settings.use_gpu) {
                    Ok(raw) if meetings.current(token) => {
                        let text = vocabulary::apply(&vocabulary, &cleaner::clean(&raw, language));
                        if !text.is_empty() {
                            meetings.edit(&id, |m| m.insert(MeetingSegment::new(chunk.speaker, chunk.start, chunk.duration, text, raw)));
                            meetings.save_now(&id);
                        }
                    }
                    Ok(_) => return,
                    Err(e) => {
                        if meetings.current(token) {
                            meetings.inner.lock().message = Some(format!(
                                "A few seconds from {} couldn't be transcribed. {e}",
                                chunk.speaker.label()
                            ));
                        }
                    }
                }
                let mut inner = meetings.inner.lock();
                inner.pending_chunks = inner.pending_chunks.saturating_sub(1);
                drop(inner);
                meetings.emit(&app);
            }
            // All sources closed: the meeting has stopped and every chunk is transcribed.
            if meetings.current(token) {
                meetings.finish(&app, &id, token);
            }
        });
    }

    pub fn stop(&self, app: &Arc<App>) {
        let mut inner = self.inner.lock();
        match inner.activity.clone() {
            Activity::Starting(id) => {
                self.session.fetch_add(1, Ordering::SeqCst);
                inner.mic = None;
                inner.them = None;
                inner.chunks = None;
                inner.activity = Activity::Idle;
                drop(inner);
                self.discard_if_empty(&id);
                self.emit(app);
            }
            Activity::Recording(id) => {
                let duration = inner.started_at.elapsed().as_secs_f64();
                inner.elapsed = duration;
                inner.activity = Activity::Finishing(id.clone());
                // Closing both writers hands off their last chunks; dropping the sender ends the queue.
                let mic = inner.mic.take();
                let them = inner.them.take();
                inner.chunks = None;
                drop(inner);
                for (capture, writer) in [mic, them].into_iter().flatten() {
                    drop(capture);
                    writer.lock().close();
                }
                self.edit(&id, |m| m.duration = duration);
                self.save_now(&id);
                self.emit(app);
            }
            _ => {}
        }
    }

    fn finish(&self, app: &Arc<App>, id: &str, token: u64) {
        self.save_now(id);
        let settings = app.settings();
        let has_content = self.inner.lock().meetings.iter().any(|m| m.id == id && m.has_content());
        if opendictate_notes::availability(&settings).is_ok() && has_content {
            self.write_notes(app, id, token);
        } else {
            self.inner.lock().activity = Activity::Idle;
            self.emit(app);
        }
        let _ = self.handle.emit("open-meeting", id);
    }

    pub fn generate_notes(&self, app: &Arc<App>, id: &str) -> Result<(), String> {
        if self.is_active() {
            return Err("Wait for the current meeting to finish.".into());
        }
        let token = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut inner = self.inner.lock();
            inner.activity = Activity::Generating(id.into());
            inner.message = None;
        }
        self.emit(app);
        let app = app.clone();
        let id = id.to_string();
        std::thread::spawn(move || app.meetings.write_notes(&app, &id, token));
        Ok(())
    }

    pub fn cancel_notes(&self, app: &App) {
        let mut inner = self.inner.lock();
        let Activity::Generating(id) = inner.activity.clone() else { return };
        self.session.fetch_add(1, Ordering::SeqCst);
        if let Some(cancel) = inner.cancel_notes.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        inner.generation_step = None;
        inner.activity = Activity::Idle;
        drop(inner);
        self.save_now(&id);
        self.emit(app);
    }

    fn write_notes(&self, app: &Arc<App>, id: &str, token: u64) {
        let Some(meeting) = self.inner.lock().meetings.iter().find(|m| m.id == id).cloned() else {
            self.inner.lock().activity = Activity::Idle;
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock();
            inner.activity = Activity::Generating(id.into());
            inner.generation_step = None;
            inner.cancel_notes = Some(cancel.clone());
        }
        self.emit(app);
        let settings = app.settings();
        let fingerprint = meeting.summary_source_fingerprint();
        let model_name = settings.notes_model_name();
        let key = |c: &SpeechConnection| credentials::read(Purpose::Notes, c).ok().flatten();
        let result = opendictate_notes::language_model(&settings, opendictate_notes::INSTRUCTIONS, &key).and_then(|model| {
            let on_step = |step: usize, total: usize| {
                if self.current(token) {
                    self.inner.lock().generation_step = Some((step, total));
                    self.emit(app);
                }
            };
            opendictate_notes::generate(&meeting.transcript_segments(), &meeting.notes, model.as_ref(), &cancel, &on_step)
        });
        if !self.current(token) {
            return;
        }
        match result {
            Ok(notes) => {
                self.edit(id, |m| {
                    let mut summary = notes.summary;
                    if summary.model_name.is_none() {
                        summary.model_name = Some(model_name);
                    }
                    summary.source_fingerprint = Some(fingerprint);
                    m.summary = Some(summary);
                    if m.title.trim().is_empty() {
                        m.title = notes.title;
                    }
                });
                self.save_now(id);
            }
            Err(e) => self.inner.lock().message = Some(format!("Couldn't write notes. {e:#}")),
        }
        let mut inner = self.inner.lock();
        inner.generation_step = None;
        inner.activity = Activity::Idle;
        inner.cancel_notes = None;
        drop(inner);
        self.emit(app);
    }

    pub fn update(&self, app: &App, id: &str, title: Option<String>, notes: Option<String>) {
        self.edit(id, |m| {
            if let Some(title) = title {
                m.title = title;
            }
            if let Some(notes) = notes {
                m.notes = notes;
            }
        });
        self.save_now(id);
        self.emit(app);
    }

    pub fn toggle_action_item(&self, app: &App, id: &str, item: &str) {
        self.edit(id, |m| {
            if let Some(a) = m.summary.as_mut().and_then(|s| s.action_items.iter_mut().find(|a| a.id == item)) {
                a.is_done = !a.is_done;
            }
        });
        self.save_now(id);
        self.emit(app);
    }

    pub fn delete(&self, app: &App, id: &str) -> Result<(), String> {
        if self.inner.lock().activity.meeting_id() == Some(id) {
            return Err("Stop this meeting before deleting it.".into());
        }
        self.store.delete(id)?;
        self.inner.lock().meetings.retain(|m| m.id != id);
        self.emit(app);
        Ok(())
    }

    pub fn text(&self, id: &str, transcript_only: bool) -> Option<String> {
        let inner = self.inner.lock();
        let meeting = inner.meetings.iter().find(|m| m.id == id)?;
        Some(if transcript_only { meeting.transcript() } else { meeting.markdown() })
    }

    pub fn dismiss_message(&self, app: &App) {
        self.inner.lock().message = None;
        self.emit(app);
    }

    fn discard_if_empty(&self, id: &str) {
        let empty = self.inner.lock().meetings.iter().any(|m| m.id == id && !m.has_content() && m.title.trim().is_empty());
        if empty {
            let _ = self.store.delete(id);
            self.inner.lock().meetings.retain(|m| m.id != id);
        }
    }

    /// Saves whatever was captured when the app quits mid-meeting.
    pub fn end_for_quit(&self) {
        self.session.fetch_add(1, Ordering::SeqCst);
        let mut inner = self.inner.lock();
        let id = inner.activity.meeting_id().map(String::from);
        if matches!(inner.activity, Activity::Recording(_)) {
            inner.elapsed = inner.started_at.elapsed().as_secs_f64();
        }
        inner.mic = None;
        inner.them = None;
        inner.chunks = None;
        inner.activity = Activity::Idle;
        let elapsed = inner.elapsed;
        drop(inner);
        if let Some(id) = id {
            self.edit(&id, |m| {
                if m.duration == 0.0 {
                    m.duration = elapsed;
                }
            });
            self.save_now(&id);
        }
    }
}

impl ChunkWriter {
    fn new(speaker: Speaker, settings: &AppSettings, out: Sender<Chunk>, start_offset: f64, level: Arc<Mutex<f32>>) -> Self {
        Self {
            speaker,
            rate: 48_000,
            threshold: if settings.silence_threshold.is_finite() { settings.silence_threshold.max(0.0) } else { 0.002 },
            policy: ChunkPolicy::default(),
            start_offset,
            elapsed_frames: 0,
            samples: Vec::new(),
            active_frames: 0,
            silent_frames: 0,
            out,
            level,
        }
    }
}

fn open_writer(source: Source, writer: ChunkWriter) -> anyhow::Result<(Capture, Arc<Mutex<ChunkWriter>>)> {
    let writer = Arc::new(Mutex::new(writer));
    let sink = writer.clone();
    let capture = Capture::open(source, move |mono| sink.lock().write(mono))?;
    writer.lock().rate = capture.rate;
    Ok((capture, writer))
}
