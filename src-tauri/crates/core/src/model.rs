//! Ports of BetterWispr's `Domain/` and `Persistence/` types.
//! JSON uses camelCase so older and newer workspaces decode with defaults.

use crate::corrections::tokenize;
use crate::style::{AppCategory, CleanupLevel, StyleContext, StyleTone};
use crate::vocabulary::VocabularyEntry;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

// MARK: Settings

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DictationMode {
    #[default]
    Hold,
    Toggle,
}

/// A global shortcut: any combination of Ctrl, Alt, Shift and Win, plus an optional key.
/// Without a key it is modifier-only (the default Ctrl + Win, like holding Fn on a Mac).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub win: bool,
    /// Windows virtual-key code of the non-modifier key, if any.
    #[serde(default)]
    pub key: Option<u32>,
    /// Display name of `key`, e.g. "Space" or "F5".
    #[serde(default)]
    pub key_name: String,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self { ctrl: true, alt: false, shift: false, win: true, key: None, key_name: String::new() }
    }
}

impl Shortcut {
    pub fn is_modifier_only(&self) -> bool {
        self.key.is_none()
    }

    fn modifier_count(&self) -> usize {
        [self.ctrl, self.alt, self.shift, self.win].iter().filter(|m| **m).count()
    }

    /// Function keys may stand alone; letters need a modifier other than Shift; modifier-only needs two.
    pub fn is_valid(&self) -> bool {
        match self.key {
            None => self.modifier_count() >= 2,
            Some(vk) if (0x70..=0x87).contains(&vk) => !self.key_name.is_empty(),
            Some(_) => !self.key_name.is_empty() && (self.ctrl || self.alt || self.win),
        }
    }

    pub fn display_name(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.win {
            parts.push("Win");
        }
        if self.key.is_some() {
            parts.push(&self.key_name);
        }
        parts.join(" + ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeechApi {
    Sarvam,
    Smallest,
    OpenAiCompatible,
}

impl SpeechApi {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Sarvam => "Sarvam AI",
            Self::Smallest => "Smallest AI",
            Self::OpenAiCompatible => "OpenAI-compatible",
        }
    }

    pub fn endpoint(&self) -> &'static str {
        match self {
            Self::Sarvam => "https://api.sarvam.ai/speech-to-text",
            Self::Smallest => "https://api.smallest.ai/waves/v1/stt/",
            Self::OpenAiCompatible => "http://localhost:8000/v1/audio/transcriptions",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Sarvam => "saaras:v4",
            Self::Smallest => "pulse",
            Self::OpenAiCompatible => "whisper-1",
        }
    }
}

/// Only connection metadata is persisted. Keys live in Windows Credential Manager.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConnection {
    pub id: String,
    pub name: String,
    pub api: SpeechApi,
    pub endpoint: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
}

impl SpeechConnection {
    pub fn new(api: SpeechApi) -> Self {
        Self {
            id: new_id(),
            name: api.name().into(),
            endpoint: api.endpoint().into(),
            model_id: api.default_model().into(),
            api,
        }
    }

    /// Credentials are bound to the exact destination and API, so editing a URL cannot forward an old key.
    pub fn credential_account(&self) -> String {
        format!("{}|{:?}|{}", self.id, self.api, self.endpoint)
    }

    pub fn speech_model_id(&self) -> String {
        format!("connection-{}", self.id)
    }

    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty()
            || name.chars().count() > 100
            || self.model_id.is_empty()
            || self.model_id.chars().count() > 200
            || self.model_id.chars().any(char::is_control)
        {
            return Err("Enter a name and a model ID (up to 100 and 200 characters).".into());
        }
        let bad_url = || {
            "Use a full HTTPS endpoint URL without credentials, query parameters or fragments. \
             HTTP is allowed only for localhost, 127.0.0.1 or [::1]."
                .to_string()
        };
        if self.endpoint.len() > 2048 {
            return Err(bad_url());
        }
        let url = url::Url::parse(&self.endpoint).map_err(|_| bad_url())?;
        let host = url.host_str().unwrap_or("").to_lowercase();
        let loopback = ["localhost", "127.0.0.1", "[::1]", "::1"].contains(&host.as_str());
        let ok = !host.is_empty()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && (url.scheme() == "https" || (url.scheme() == "http" && loopback))
            && (self.api == SpeechApi::OpenAiCompatible || self.endpoint == self.api.endpoint());
        if ok { Ok(()) } else { Err(bad_url()) }
    }

    pub fn validate_key(&self, key: &str) -> Result<(), String> {
        if self.api != SpeechApi::OpenAiCompatible && key.is_empty() {
            return Err("Add your API key in Models → Bring your own model.".into());
        }
        if key.len() > 8192 || !key.bytes().all(|b| (33..=126).contains(&b)) {
            return Err("The API key contains spaces or unsupported characters. Paste only the key.".into());
        }
        Ok(())
    }

    /// Checked before recording and again at the request boundary.
    pub fn validate_language(&self, language: Option<&str>) -> Result<(), String> {
        let code = language.filter(|l| *l != "auto").map(crate::cleaner::base_language);
        if self.api == SpeechApi::Smallest {
            let Some(code) = code.as_deref() else {
                return Err("Smallest AI needs a specific spoken language. Open Settings and change Spoken language from Detect automatically to English, Hindi or another language.".into());
            };
            const SUPPORTED: &[&str] = &[
                "en", "hi", "zh", "ko", "ja", "yue", "ms", "id", "tl", "it", "es", "pt", "de", "fr", "uk", "ru", "pl",
                "cs", "sk", "nl", "lv", "et", "ro", "fi", "sv", "bg", "hu", "da", "lt", "mt", "kn", "ml", "mr", "gu",
                "te", "or", "bn", "pa", "ta",
            ];
            if !SUPPORTED.contains(&code) {
                return Err("Smallest AI does not support the selected language. Open Settings and choose a supported Spoken language.".into());
            }
            if self.model_id == "pulse-pro" && code != "en" {
                return Err("Pulse Pro requires English. Choose English in Settings or use the pulse model.".into());
            }
        }
        if self.api == SpeechApi::Sarvam {
            if let Some(code) = code.as_deref() {
                const SUPPORTED: &[&str] = &[
                    "en", "hi", "bn", "kn", "ml", "mr", "od", "pa", "ta", "te", "gu", "as", "ur", "ne", "kok", "ks",
                    "sd", "sa", "sat", "mni", "brx", "mai", "doi",
                ];
                if !SUPPORTED.contains(&code) {
                    return Err("Sarvam does not support the selected language. Choose a supported language or Detect automatically in Settings.".into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotesCli {
    ClaudeCode,
    Codex,
}

impl NotesCli {
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}

/// Which model writes meeting notes and Medium cleanup edits. Local Ollama replaces
/// Apple Intelligence as the free, on-device default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum NotesSelection {
    /// No notes model chosen yet.
    None,
    Ollama(String),
    Connection(String),
    Cli(NotesCli),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDevice {
    /// Stable device name as reported by Windows.
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    #[serde(rename = "selectedModelID")]
    pub selected_model_id: String,
    pub speech_connections: Vec<SpeechConnection>,
    pub notes_connections: Vec<SpeechConnection>,
    pub notes_selection: NotesSelection,
    pub claude_notes_model: String,
    pub codex_notes_model: String,
    pub language: String,
    pub auto_paste: bool,
    pub copy_to_clipboard: bool,
    pub save_history: bool,
    pub show_capsule: bool,
    pub launch_at_login: bool,
    pub silence_threshold: f32,
    pub dictation_mode: DictationMode,
    pub shortcut: Shortcut,
    pub learn_corrections: bool,
    pub cleanup: CleanupLevel,
    /// Missing contexts use `Formal`, which leaves the transcript as recognized.
    pub styles: BTreeMap<StyleContext, StyleTone>,
    /// None follows the Windows default input.
    pub microphone: Option<AudioInputDevice>,
    pub use_gpu: bool,
    /// The newest onboarding the user finished or skipped; 0 shows it again.
    pub completed_onboarding_version: u32,
    pub onboarding_step: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            selected_model_id: "parakeet-v3".into(),
            speech_connections: Vec::new(),
            notes_connections: Vec::new(),
            notes_selection: NotesSelection::None,
            claude_notes_model: String::new(),
            codex_notes_model: String::new(),
            language: "auto".into(),
            auto_paste: true,
            copy_to_clipboard: false,
            save_history: true,
            show_capsule: true,
            launch_at_login: false,
            silence_threshold: 0.002,
            dictation_mode: DictationMode::Hold,
            shortcut: Shortcut::default(),
            learn_corrections: true,
            cleanup: CleanupLevel::Light,
            styles: BTreeMap::new(),
            microphone: None,
            use_gpu: true,
            completed_onboarding_version: 0,
            onboarding_step: 0,
        }
    }
}

impl AppSettings {
    pub fn tone(&self, context: StyleContext) -> StyleTone {
        self.styles.get(&context).copied().unwrap_or(StyleTone::Formal)
    }

    pub fn language_code(&self) -> Option<&str> {
        (self.language != "auto").then_some(self.language.as_str())
    }

    pub fn cli_model(&self, cli: NotesCli) -> &str {
        match cli {
            NotesCli::ClaudeCode => &self.claude_notes_model,
            NotesCli::Codex => &self.codex_notes_model,
        }
    }

    pub fn notes_model_name(&self) -> String {
        match &self.notes_selection {
            NotesSelection::None => "No notes model".into(),
            NotesSelection::Ollama(name) => format!("Ollama · {name}"),
            NotesSelection::Connection(id) => self
                .notes_connections
                .iter()
                .find(|c| &c.id == id)
                .map_or("Missing notes connection".into(), |c| format!("{} · {}", c.name, c.model_id)),
            NotesSelection::Cli(cli) => {
                let model = self.cli_model(*cli);
                format!("{} · {}", cli.name(), if model.is_empty() { "Choose a model" } else { model })
            }
        }
    }
}

// MARK: History

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub text: String,
    pub raw_text: String,
    pub duration: f64,
    pub model_name: String,
    pub language: String,
    /// Executable name of the app that received the dictation, e.g. "slack.exe".
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub vocabulary_fixes: Option<usize>,
}

impl Transcript {
    pub fn new(text: String, raw_text: String, duration: f64, model_name: String, language: String) -> Self {
        Self {
            id: new_id(),
            created_at: Utc::now(),
            text,
            raw_text,
            duration,
            model_name,
            language,
            app: None,
            vocabulary_fixes: None,
        }
    }

    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

// MARK: Insights

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageInsights {
    pub total_words: usize,
    pub words_per_minute: usize,
    pub words_cleaned: usize,
    pub dictionary_fixes: usize,
    pub dictations_by_category: BTreeMap<AppCategory, usize>,
    pub apps_used: usize,
    pub words_by_day: BTreeMap<NaiveDate, usize>,
    pub current_streak: usize,
    pub longest_streak: usize,
}

impl UsageInsights {
    /// Derives usage from saved history in local days.
    pub fn new(history: &[Transcript], today: NaiveDate, to_day: impl Fn(&DateTime<Utc>) -> NaiveDate) -> Self {
        let mut insights = Self::default();
        let mut seconds = 0.0;
        let mut apps = HashSet::new();
        for transcript in history {
            let words = transcript.word_count();
            insights.total_words += words;
            seconds += transcript.duration;
            insights.words_cleaned += removed_words(&transcript.raw_text, &transcript.text);
            insights.dictionary_fixes += transcript.vocabulary_fixes.unwrap_or(0);
            *insights.words_by_day.entry(to_day(&transcript.created_at)).or_default() += words;
            let Some(app) = &transcript.app else { continue };
            apps.insert(app.to_lowercase());
            *insights.dictations_by_category.entry(AppCategory::from_app(Some(app))).or_default() += 1;
        }
        insights.apps_used = apps.len();
        insights.words_per_minute =
            if seconds > 0.0 { (insights.total_words as f64 / (seconds / 60.0)).round() as usize } else { 0 };
        let days: HashSet<NaiveDate> = insights.words_by_day.keys().copied().collect();
        (insights.current_streak, insights.longest_streak) = streaks(&days, today);
        insights
    }

    pub fn local(history: &[Transcript]) -> Self {
        Self::new(history, Local::now().date_naive(), |d| d.with_timezone(&Local).date_naive())
    }
}

/// Counts recognized words that cleanup, rewriting or vocabulary removed or replaced, ignoring case and punctuation.
pub fn removed_words(raw: &str, text: &str) -> usize {
    let mut kept: HashMap<String, usize> = HashMap::new();
    for word in tokenize(text) {
        *kept.entry(word.to_lowercase()).or_default() += 1;
    }
    let mut removed = 0;
    for word in tokenize(raw) {
        match kept.get_mut(&word.to_lowercase()) {
            Some(count) if *count > 0 => *count -= 1,
            _ => removed += 1,
        }
    }
    removed
}

/// A streak still counts today until the day ends without a dictation.
fn streaks(days: &HashSet<NaiveDate>, today: NaiveDate) -> (usize, usize) {
    let run = |mut day: NaiveDate| {
        let mut length = 0;
        while days.contains(&day) {
            length += 1;
            day -= Duration::days(1);
        }
        length
    };
    let longest = days.iter().filter(|d| !days.contains(&(**d + Duration::days(1)))).map(|d| run(*d)).max().unwrap_or(0);
    (run(today).max(run(today - Duration::days(1))), longest)
}

// MARK: Workspace

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedState {
    pub version: u32,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub history: Vec<Transcript>,
    #[serde(default)]
    pub vocabulary: Vec<VocabularyEntry>,
}

impl Default for SavedState {
    fn default() -> Self {
        Self { version: 1, settings: AppSettings::default(), history: Vec::new(), vocabulary: Vec::new() }
    }
}

/// Atomic JSON storage that refuses to overwrite a workspace it could not read.
pub struct LocalStore {
    pub directory: PathBuf,
}

impl LocalStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn state_path(&self) -> PathBuf {
        self.directory.join("workspace.json")
    }

    pub fn load(&self) -> Result<SavedState, String> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(SavedState::default());
        }
        let data = fs::read(&path).map_err(|e| e.to_string())?;
        let state: SavedState = serde_json::from_slice(&data).map_err(|e| format!("The workspace is not valid: {e}"))?;
        if state.version != 1 {
            return Err(format!("This workspace uses version {}. Update OpenDictate before opening it.", state.version));
        }
        Ok(state)
    }

    pub fn save(&self, state: &SavedState) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        write_atomic(&self.state_path(), &json)
    }
}

pub fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("Invalid path")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(data).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

// MARK: Meetings

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Speaker {
    Me,
    Them,
}

impl Speaker {
    pub fn label(self) -> &'static str {
        match self {
            Self::Me => "Me",
            Self::Them => "Them",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSegment {
    pub id: String,
    pub speaker: Speaker,
    pub start: f64,
    pub duration: f64,
    pub text: String,
    pub raw_text: String,
}

impl MeetingSegment {
    pub fn new(speaker: Speaker, start: f64, duration: f64, text: String, raw_text: String) -> Self {
        Self { id: new_id(), speaker, start, duration, text, raw_text }
    }

    pub fn timestamp(&self) -> String {
        let seconds = self.start.max(0.0) as u64;
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub is_done: bool,
}

impl ActionItem {
    pub fn new(text: String) -> Self {
        Self { id: new_id(), text, is_done: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummary {
    pub overview: String,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub source_fingerprint: Option<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub summary: Option<MeetingSummary>,
    #[serde(default)]
    pub segments: Vec<MeetingSegment>,
    pub model_name: String,
    pub language: String,
}

impl Meeting {
    pub fn new(model_name: String, language: String) -> Self {
        Self {
            id: new_id(),
            title: String::new(),
            created_at: Utc::now(),
            duration: 0.0,
            notes: String::new(),
            summary: None,
            segments: Vec::new(),
            model_name,
            language,
        }
    }

    pub fn display_title(&self) -> String {
        let title = self.title.trim();
        if title.is_empty() { "New Meeting".into() } else { title.into() }
    }

    pub fn has_content(&self) -> bool {
        self.segments.iter().any(|s| !s.text.is_empty()) || !self.notes.trim().is_empty()
    }

    /// Keeps segments ordered by start time.
    pub fn insert(&mut self, segment: MeetingSegment) {
        let index = self.segments.iter().position(|s| s.start > segment.start).unwrap_or(self.segments.len());
        self.segments.insert(index, segment);
    }

    /// Keep the original segments on disk; hide speaker playback picked up again by the microphone.
    pub fn transcript_segments(&self) -> Vec<&MeetingSegment> {
        removing_echoes(&self.segments)
    }

    pub fn transcript(&self) -> String {
        self.transcript_segments()
            .iter()
            .map(|s| format!("[{}] {}: {}", s.timestamp(), s.speaker.label(), s.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn summary_source_fingerprint(&self) -> String {
        // Length-prefix each input so different transcript/thought pairs cannot share a boundary.
        let transcript = self.transcript();
        let source = format!("{}:{}{}:{}", transcript.len(), transcript, self.notes.len(), self.notes);
        Sha256::digest(source.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn summary_needs_update(&self) -> bool {
        self.summary.as_ref().is_some_and(|s| s.source_fingerprint.as_deref() != Some(&self.summary_source_fingerprint()))
    }

    pub fn markdown(&self) -> String {
        let created = self.created_at.with_timezone(&Local).format("%B %-d, %Y at %-I:%M %p");
        let mut blocks = vec![format!("# {}", self.display_title()), created.to_string()];
        if let Some(summary) = &self.summary {
            if !summary.overview.is_empty() {
                blocks.push(format!("## Summary\n\n{}", summary.overview));
            }
            let list = |items: &[String]| items.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n");
            if !summary.key_points.is_empty() {
                blocks.push(format!("## Key points\n\n{}", list(&summary.key_points)));
            }
            if !summary.decisions.is_empty() {
                blocks.push(format!("## Decisions\n\n{}", list(&summary.decisions)));
            }
            if !summary.action_items.is_empty() {
                let items = summary
                    .action_items
                    .iter()
                    .map(|a| format!("- [{}] {}", if a.is_done { "x" } else { " " }, a.text))
                    .collect::<Vec<_>>()
                    .join("\n");
                blocks.push(format!("## Action items\n\n{items}"));
            }
        }
        let notes = self.notes.trim();
        if !notes.is_empty() {
            blocks.push(format!("## My notes\n\n{notes}"));
        }
        if !self.segments.is_empty() {
            blocks.push(format!("## Transcript\n\n{}", self.transcript()));
        }
        blocks.join("\n\n") + "\n"
    }
}

/// Hides microphone segments of eight or more words that exactly repeat system audio within 30 seconds.
pub fn removing_echoes(segments: &[MeetingSegment]) -> Vec<&MeetingSegment> {
    fn words(text: &str) -> String {
        text.to_lowercase()
            .split(|c: char| !c.is_alphabetic() && !c.is_numeric())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
    let mut system_speech: HashMap<String, Vec<f64>> = HashMap::new();
    for segment in segments.iter().filter(|s| s.speaker == Speaker::Them) {
        let normalized = words(&segment.text);
        if normalized.split(' ').count() >= 8 {
            system_speech.entry(normalized).or_default().push(segment.start);
        }
    }
    segments
        .iter()
        .filter(|segment| {
            segment.speaker != Speaker::Me
                || !system_speech
                    .get(&words(&segment.text))
                    .is_some_and(|starts| starts.iter().any(|s| (s - segment.start).abs() <= 30.0))
        })
        .collect()
}

/// One JSON file per meeting. Unreadable files are reported and left untouched.
pub struct MeetingStore {
    pub directory: PathBuf,
}

impl MeetingStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn load_all(&self) -> (Vec<Meeting>, Vec<String>) {
        let mut meetings = Vec::new();
        let mut unreadable = Vec::new();
        let Ok(entries) = fs::read_dir(&self.directory) else { return (meetings, unreadable) };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match fs::read(&path).ok().and_then(|d| serde_json::from_slice::<Meeting>(&d).ok()) {
                    Some(meeting) => meetings.push(meeting),
                    None => unreadable.push(path.file_name().unwrap_or_default().to_string_lossy().into_owned()),
                }
            }
        }
        meetings.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        (meetings, unreadable)
    }

    pub fn save(&self, meeting: &Meeting) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(meeting).map_err(|e| e.to_string())?;
        write_atomic(&self.directory.join(format!("{}.json", meeting.id)), &json)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.directory.join(format!("{id}.json"));
        if path.exists() { fs::remove_file(path).map_err(|e| e.to_string()) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn workspace_round_trips_and_refuses_corrupt_data() {
        let dir = std::env::temp_dir().join(format!("od-test-{}", new_id()));
        let store = LocalStore::new(dir.clone());
        assert_eq!(store.load().unwrap(), SavedState::default());
        let mut state = SavedState::default();
        state.history.push(Transcript::new("Hello".into(), "hello".into(), 2.0, "Test".into(), "en".into()));
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), state);
        fs::write(store.state_path(), "broken").unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read_to_string(store.state_path()).unwrap(), "broken");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn older_settings_decode_with_defaults() {
        let saved = r#"{"autoPaste":true,"language":"auto","launchAtLogin":false,"saveHistory":true,"selectedModelID":"parakeet-v3","showCapsule":true,"silenceThreshold":0.002}"#;
        let mut settings: AppSettings = serde_json::from_str(saved).unwrap();
        assert_eq!(settings.cleanup, CleanupLevel::Light);
        assert!(StyleContext::ALL.iter().all(|c| settings.tone(*c) == StyleTone::Formal));
        assert_eq!(settings.dictation_mode, DictationMode::Hold);
        assert!(settings.learn_corrections);
        settings.cleanup = CleanupLevel::Medium;
        settings.styles.insert(StyleContext::Personal, StyleTone::VeryCasual);
        let restored: AppSettings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored.cleanup, CleanupLevel::Medium);
        assert_eq!(restored.tone(StyleContext::Personal), StyleTone::VeryCasual);
        assert_eq!(restored.tone(StyleContext::Email), StyleTone::Formal);
    }

    #[test]
    fn shortcuts_validate_and_display() {
        assert_eq!(Shortcut::default().display_name(), "Ctrl + Win");
        assert!(Shortcut::default().is_valid());
        let d = Shortcut { ctrl: false, alt: false, shift: true, win: false, key: Some(0x44), key_name: "D".into() };
        assert!(!d.is_valid());
        let f5 = Shortcut { ctrl: false, alt: false, shift: false, win: false, key: Some(0x74), key_name: "F5".into() };
        assert!(f5.is_valid());
        assert_eq!(f5.display_name(), "F5");
    }

    #[test]
    fn insights_summarize_history() {
        let now = Utc.with_ymd_and_hms(2026, 10, 8, 12, 0, 0).unwrap();
        let days_ago = |d: i64| now - Duration::days(d);
        let make = |d: i64, text: &str, raw: &str, app: Option<&str>, fixes: Option<usize>| Transcript {
            created_at: days_ago(d),
            app: app.map(String::from),
            vocabulary_fixes: fixes,
            ..Transcript::new(text.into(), raw.into(), 60.0, "m".into(), "en".into())
        };
        let history = vec![
            make(1, "Kartik said hello there", "um cardic said hello hello there", Some("slack.exe"), Some(1)),
            make(2, "one two three four", "one two three four", Some("ChatGPT.exe"), None),
            make(5, "a b c d e f", "a b c d e f", Some("ChatGPT.exe"), None),
            make(6, "a b", "a b", None, None),
            make(7, "a b", "a b", None, None),
        ];
        let utc_day = |d: &DateTime<Utc>| d.date_naive();
        let insights = UsageInsights::new(&history, now.date_naive(), utc_day);
        assert_eq!(insights.total_words, 18);
        assert_eq!(insights.words_per_minute, 4);
        assert_eq!(insights.words_cleaned, 3);
        assert_eq!(insights.dictionary_fixes, 1);
        assert_eq!(
            insights.dictations_by_category,
            BTreeMap::from([(AppCategory::Work, 1), (AppCategory::AiPrompts, 2)])
        );
        assert_eq!(insights.apps_used, 2);
        assert_eq!(insights.current_streak, 2);
        assert_eq!(insights.longest_streak, 3);
        assert_eq!(UsageInsights::new(&history, days_ago(-2).date_naive(), utc_day).current_streak, 0);
    }

    #[test]
    fn echoes_of_system_audio_are_hidden() {
        let line = "we should ship the new onboarding flow on friday morning";
        let segments = vec![
            MeetingSegment::new(Speaker::Them, 10.0, 5.0, line.into(), line.into()),
            MeetingSegment::new(Speaker::Me, 12.0, 5.0, format!("{line}."), line.into()),
            MeetingSegment::new(Speaker::Me, 80.0, 5.0, line.into(), line.into()),
            MeetingSegment::new(Speaker::Me, 11.0, 1.0, "Sounds good".into(), "Sounds good".into()),
        ];
        let kept = removing_echoes(&segments);
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().all(|s| s.start != 12.0));
    }
}
