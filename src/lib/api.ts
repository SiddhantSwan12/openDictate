// Typed bridge to the Rust backend. Field names mirror the serde (camelCase) output exactly.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type CleanupLevel = "none" | "light" | "medium";
export type StyleContext = "personal" | "work" | "email" | "other";
export type StyleTone = "formal" | "casual" | "veryCasual" | "excited";
export type AppCategory = "aiPrompts" | "work" | "personal" | "documents" | "email" | "other";
export type DictationMode = "hold" | "toggle";
export type SpeechApi = "sarvam" | "smallest" | "openAiCompatible";
export type NotesCli = "claudeCode" | "codex";
export type EngineKind = "whisper" | "parakeet" | "api";

export interface Shortcut {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  win: boolean;
  /** Windows virtual-key code; null for modifier-only shortcuts such as Ctrl + Win. */
  key: number | null;
  keyName: string;
}

export interface AudioInputDevice {
  id: string;
  name: string;
}

export interface SpeechConnection {
  id: string;
  name: string;
  api: SpeechApi;
  endpoint: string;
  modelID: string;
}

export type NotesSelection =
  | { kind: "none" }
  | { kind: "ollama"; value: string }
  | { kind: "connection"; value: string }
  | { kind: "cli"; value: NotesCli };

export interface AppSettings {
  selectedModelID: string;
  speechConnections: SpeechConnection[];
  notesConnections: SpeechConnection[];
  notesSelection: NotesSelection;
  claudeNotesModel: string;
  codexNotesModel: string;
  /** "auto" or a language code such as "en". */
  language: string;
  autoPaste: boolean;
  copyToClipboard: boolean;
  saveHistory: boolean;
  showCapsule: boolean;
  launchAtLogin: boolean;
  silenceThreshold: number;
  dictationMode: DictationMode;
  shortcut: Shortcut;
  learnCorrections: boolean;
  cleanup: CleanupLevel;
  styles: Partial<Record<StyleContext, StyleTone>>;
  microphone: AudioInputDevice | null;
  useGpu: boolean;
  completedOnboardingVersion: number;
  onboardingStep: number;
}

export interface Transcript {
  id: string;
  /** RFC 3339 timestamp. */
  createdAt: string;
  text: string;
  rawText: string;
  duration: number;
  modelName: string;
  language: string;
  /** Executable that received the dictation, e.g. "slack.exe". */
  app: string | null;
  vocabularyFixes: number | null;
}

export interface VocabularyEntry {
  id: string;
  phrase: string;
  replacement: string;
  learned: boolean;
}

export interface ModelView {
  id: string;
  name: string;
  detail: string;
  sizeLabel: string;
  engine: EngineKind;
  installed: boolean;
  languages: string[];
}

export interface UsageInsights {
  totalWords: number;
  wordsPerMinute: number;
  wordsCleaned: number;
  dictionaryFixes: number;
  dictationsByCategory: Partial<Record<AppCategory, number>>;
  appsUsed: number;
  /** Keys are local dates "YYYY-MM-DD". */
  wordsByDay: Record<string, number>;
  currentStreak: number;
  longestStreak: number;
}

export interface Workspace {
  settings: AppSettings;
  history: Transcript[];
  vocabulary: VocabularyEntry[];
  models: ModelView[];
  insights: UsageInsights;
  microphones: AudioInputDevice[];
  defaultMicrophone: AudioInputDevice | null;
  needsOnboarding: boolean;
  storageReadable: boolean;
  shortcutName: string;
  gpuName: string | null;
}

export type Phase =
  | { kind: "idle" }
  | { kind: "preparing" }
  | { kind: "recording" }
  | { kind: "transcribing" }
  | { kind: "failed"; title: string; message: string }
  | { kind: "unpasted"; text: string };

export interface Installation {
  id: string;
  /** 0...1; above 0.9 the model is being verified. */
  progress: number;
  failure: string | null;
}

export interface Session {
  phase: Phase;
  statusMessage: string;
  partialTranscript: string;
  held: boolean;
  recordingDuration: number;
  installation: Installation | null;
  loadingModel: boolean;
}

export interface Toast {
  title: string;
  message: string;
  /** "copy" | "trash" | "sparkles" | "warning" */
  icon: string;
}

export type Speaker = "me" | "them";

export interface MeetingSegment {
  id: string;
  speaker: Speaker;
  start: number;
  duration: number;
  text: string;
  rawText: string;
}

export interface ActionItem {
  id: string;
  text: string;
  isDone: boolean;
}

export interface MeetingSummary {
  overview: string;
  keyPoints: string[];
  decisions: string[];
  actionItems: ActionItem[];
  modelName: string | null;
  sourceFingerprint: string | null;
  generatedAt: string;
}

export interface Meeting {
  id: string;
  title: string;
  createdAt: string;
  duration: number;
  notes: string;
  summary: MeetingSummary | null;
  segments: MeetingSegment[];
  modelName: string;
  language: string;
}

export type MeetingActivity =
  | { kind: "idle" }
  | { kind: "starting"; id: string }
  | { kind: "recording"; id: string }
  | { kind: "finishing"; id: string }
  | { kind: "generating"; id: string };

export interface MeetingsView {
  meetings: Meeting[];
  activity: MeetingActivity;
  elapsed: number;
  pendingChunks: number;
  generationStep: [number, number] | null;
  message: string | null;
  systemAudioIssue: string | null;
  showsCallAudioHint: boolean;
  microphone: string | null;
  /** Why notes can't be written right now; null when the notes model is ready. */
  notesAvailability: string | null;
  summariesNeedingUpdate: string[];
}

export interface CliCatalog {
  models: { id: string; name: string; detail: string }[];
  defaultId: string | null;
}

export type Page =
  | "overview"
  | "insights"
  | "meetings"
  | "history"
  | "models"
  | "vocabulary"
  | "style"
  | "settings"
  | "about";

export const api = {
  getSession: () => invoke<Session>("get_session"),
  getWorkspace: () => invoke<Workspace>("get_workspace"),
  getMeetings: () => invoke<MeetingsView>("get_meetings"),

  toggleRecording: () => invoke<void>("toggle_recording"),
  finishRecording: () => invoke<void>("finish_recording"),
  cancelRecording: () => invoke<void>("cancel_recording"),
  dismissCard: () => invoke<void>("dismiss_card"),
  capsuleIdle: () => invoke<void>("capsule_idle"),

  selectModel: (id: string) => invoke<void>("select_model", { id }),
  installModel: (id: string) => invoke<void>("install_model", { id }),
  cancelInstallation: () => invoke<void>("cancel_installation"),
  uninstallModel: (id: string) => invoke<void>("uninstall_model", { id }),
  newConnection: (api: SpeechApi) => invoke<SpeechConnection>("new_connection", { api }),
  /** Pass key = null to keep the stored key. */
  saveConnection: (connection: SpeechConnection, key: string | null, forNotes: boolean) =>
    invoke<void>("save_connection", { connection, key, forNotes }),
  deleteConnection: (id: string, forNotes: boolean) => invoke<void>("delete_connection", { id, forNotes }),
  connectionHasKey: (connection: SpeechConnection, forNotes: boolean) =>
    invoke<boolean>("connection_has_key", { connection, forNotes }),

  ollamaModels: () => invoke<string[]>("ollama_models"),
  cliCatalog: (cli: NotesCli) => invoke<CliCatalog>("cli_catalog", { cli }),
  cliInstalled: (cli: NotesCli) => invoke<boolean>("cli_installed", { cli }),
  notesAvailability: () => invoke<string | null>("notes_availability"),
  testNotesModel: () => invoke<string>("test_notes_model"),

  updateSettings: (settings: AppSettings) => invoke<void>("update_settings", { settings }),
  finishOnboarding: () => invoke<void>("finish_onboarding"),
  showOnboarding: () => invoke<void>("show_onboarding"),
  beginShortcutCapture: () => invoke<void>("begin_shortcut_capture"),
  cancelShortcutCapture: () => invoke<void>("cancel_shortcut_capture"),
  /** e.g. "privacy-microphone", "sound". */
  openWindowsSettings: (page: string) => invoke<void>("open_windows_settings", { page }),
  openDataFolder: () => invoke<void>("open_data_folder"),

  addVocabulary: (phrase: string, replacement: string) => invoke<void>("add_vocabulary", { phrase, replacement }),
  deleteVocabulary: (id: string) => invoke<void>("delete_vocabulary", { id }),
  deleteTranscript: (id: string) => invoke<void>("delete_transcript", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  updateTranscript: (id: string, text: string) => invoke<void>("update_transcript", { id, text }),
  restoreOriginal: (id: string) => invoke<void>("restore_original", { id }),
  exportHistory: (path: string) => invoke<void>("export_history", { path }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),

  meetingStart: () => invoke<string>("meeting_start"),
  meetingStop: () => invoke<void>("meeting_stop"),
  meetingGenerateNotes: (id: string) => invoke<void>("meeting_generate_notes", { id }),
  meetingCancelNotes: () => invoke<void>("meeting_cancel_notes"),
  meetingUpdate: (id: string, title: string | null, notes: string | null) =>
    invoke<void>("meeting_update", { id, title, notes }),
  meetingToggleAction: (id: string, item: string) => invoke<void>("meeting_toggle_action", { id, item }),
  meetingDelete: (id: string) => invoke<void>("meeting_delete", { id }),
  meetingCopy: (id: string, transcriptOnly: boolean) => invoke<void>("meeting_copy", { id, transcriptOnly }),
  meetingExport: (id: string, path: string) => invoke<void>("meeting_export", { id, path }),
  meetingDismissMessage: () => invoke<void>("meeting_dismiss_message"),

  showMain: (page: Page | null) => invoke<void>("show_main", { page }),
  quit: () => invoke<void>("quit_app"),
};

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}

/** Formats an error thrown by `invoke` (commands reject with a string). */
export function errorText(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : String(error);
}

export function durationLabel(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function wordCount(text: string): number {
  return text.split(/\s+/).filter(Boolean).length;
}

/** Spoken languages offered in Settings. "auto" detects the language. */
export const LANGUAGES: [string, string][] = [
  ["auto", "Detect automatically"],
  ["en", "English"],
  ["hi", "Hindi"],
  ["es", "Spanish"],
  ["fr", "French"],
  ["de", "German"],
  ["it", "Italian"],
  ["pt", "Portuguese"],
  ["nl", "Dutch"],
  ["pl", "Polish"],
  ["ru", "Russian"],
  ["uk", "Ukrainian"],
  ["sv", "Swedish"],
  ["da", "Danish"],
  ["fi", "Finnish"],
  ["cs", "Czech"],
  ["ro", "Romanian"],
  ["el", "Greek"],
  ["hu", "Hungarian"],
  ["tr", "Turkish"],
  ["ar", "Arabic"],
  ["bn", "Bengali"],
  ["mr", "Marathi"],
  ["ta", "Tamil"],
  ["te", "Telugu"],
  ["gu", "Gujarati"],
  ["kn", "Kannada"],
  ["ml", "Malayalam"],
  ["pa", "Punjabi"],
  ["ur", "Urdu"],
  ["ja", "Japanese"],
  ["ko", "Korean"],
  ["zh", "Chinese"],
  ["id", "Indonesian"],
  ["vi", "Vietnamese"],
  ["th", "Thai"],
];
