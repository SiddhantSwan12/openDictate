// Port of BetterWispr's SettingsView, adapted for Windows.
import { ExternalLink, FolderOpen, Lock, RotateCcw, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, LANGUAGES, on, type AppSettings, type AudioInputDevice, type DictationMode, type Shortcut } from "../lib/api";
import { useStore } from "../lib/store";
import { Button, PageHeader, Row, Section, Select, ShortcutKeys, Toggle } from "../components/ui";
import "./Settings.css";

export const DEFAULT_SHORTCUT: Shortcut = { ctrl: true, alt: false, shift: false, win: true, key: null, keyName: "" };

/** Same as the backend's `Shortcut::display_name`. */
export function shortcutName(shortcut: Shortcut): string {
  const parts: string[] = [];
  if (shortcut.ctrl) parts.push("Ctrl");
  if (shortcut.alt) parts.push("Alt");
  if (shortcut.shift) parts.push("Shift");
  if (shortcut.win) parts.push("Win");
  if (shortcut.key !== null) parts.push(shortcut.keyName);
  return parts.join(" + ");
}

function sameShortcut(a: Shortcut, b: Shortcut): boolean {
  return a.ctrl === b.ctrl && a.alt === b.alt && a.shift === b.shift && a.win === b.win && a.key === b.key;
}

/** Mirrors the backend's `Shortcut::is_valid`; returns why a shortcut can't be used, or null. */
function shortcutProblem(shortcut: Shortcut): string | null {
  const modifiers = [shortcut.ctrl, shortcut.alt, shortcut.shift, shortcut.win].filter(Boolean).length;
  if (shortcut.key === null) {
    return modifiers >= 2 ? null : "Hold two modifier keys together, such as Ctrl + Win, then release them.";
  }
  if (!shortcut.keyName) return "That key can't be a shortcut. Try another.";
  const isFunctionKey = shortcut.key >= 0x70 && shortcut.key <= 0x87;
  if (isFunctionKey || shortcut.ctrl || shortcut.alt || shortcut.win) return null;
  return `${shortcutName(shortcut)} is for typing, so it can't start dictation. Hold Ctrl, Alt or Win with it.`;
}

function useBusy(): boolean {
  const { session } = useStore();
  return ["preparing", "recording", "transcribing"].includes(session.phase.kind);
}

/** Picks the input for dictation and meetings. A saved mic stays listed while it is unplugged. */
export function MicrophonePicker({ width = 280 }: { width?: number }) {
  const { workspace, saveSettings } = useStore();
  const { settings, microphones, defaultMicrophone } = workspace;
  const saved = settings.microphone;
  const choices: AudioInputDevice[] = saved && !microphones.some((m) => m.id === saved.id) ? [...microphones, saved] : microphones;
  const options: [string, string][] = [
    ["", defaultMicrophone ? `Automatic (${defaultMicrophone.name})` : "Automatic"],
    ...choices.map((d): [string, string] => [d.id, microphones.some((m) => m.id === d.id) ? d.name : `${d.name} (not connected)`]),
  ];
  return (
    <Select
      value={saved?.id ?? ""}
      options={options}
      width={width}
      onChange={(id) => saveSettings((s) => ({ ...s, microphone: choices.find((d) => d.id === id) ?? null }))}
    />
  );
}

function ShortcutRecorder() {
  const { workspace, saveSettings, run } = useStore();
  const busy = useBusy();
  const shortcut = workspace.settings.shortcut;
  const mode = workspace.settings.dictationMode;
  const [capturing, setCapturing] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const capturingRef = useRef(false);
  capturingRef.current = capturing;

  // Listen first, then ask the backend to capture, so no key press is missed.
  useEffect(() => {
    if (!capturing) return;
    let live = true;
    const subscriptions = [
      on<Shortcut>("shortcut-captured", (captured) => {
        const problem = shortcutProblem(captured);
        if (problem) {
          // Keep listening, like BetterWispr does after an unusable key.
          setFeedback(problem);
          api.beginShortcutCapture();
          return;
        }
        setCapturing(false);
        setFeedback(null);
        if (!sameShortcut(captured, shortcut)) saveSettings((s) => ({ ...s, shortcut: captured }));
      }),
      on<null>("shortcut-capture-cancelled", () => {
        setCapturing(false);
        setFeedback(null);
      }),
    ];
    Promise.all(subscriptions).then(() => {
      if (live) run(() => api.beginShortcutCapture()).then((ok) => ok || setCapturing(false));
    });
    // Capture swallows every key on the PC, so stop as soon as the user switches away.
    const stopOnBlur = () => {
      api.cancelShortcutCapture();
      setCapturing(false);
      setFeedback(null);
    };
    window.addEventListener("blur", stopOnBlur);
    return () => {
      live = false;
      window.removeEventListener("blur", stopOnBlur);
      subscriptions.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [capturing]);

  useEffect(
    () => () => {
      if (capturingRef.current) api.cancelShortcutCapture();
    },
    [],
  );

  function cancel() {
    api.cancelShortcutCapture();
    setCapturing(false);
    setFeedback(null);
  }

  const name = shortcutName(shortcut);
  const isDefault = sameShortcut(shortcut, DEFAULT_SHORTCUT);

  return (
    <Row
      title="Keyboard shortcut"
      detail={
        capturing ? (
          <span aria-live="polite">
            {feedback ??
              "Press a key combination such as Ctrl + Shift + D, press an F-key, or hold two modifiers such as Ctrl + Win and release them. Esc cancels."}
          </span>
        ) : (
          <>
            {mode === "hold" ? "Hold to speak. Release to finish." : "Press once to speak. Press again or click the capsule to finish."}
            {shortcut.key === null && " Press both keys together. Pressing another key while holding them keeps the usual Windows shortcut."}
          </>
        )
      }
    >
      {capturing ? (
        <>
          <span className="settings-capturing" aria-live="polite">
            Press a shortcut…
          </span>
          <Button onClick={cancel}>Cancel</Button>
        </>
      ) : (
        <>
          {!isDefault && (
            <Button
              variant="ghost"
              size="icon"
              icon={RotateCcw}
              title={`Reset to ${shortcutName(DEFAULT_SHORTCUT)}`}
              disabled={busy}
              onClick={() => saveSettings((s) => ({ ...s, shortcut: DEFAULT_SHORTCUT }))}
            />
          )}
          <span aria-label={`Current shortcut: ${name}`}>
            <ShortcutKeys name={name} />
          </span>
          <Button
            disabled={busy}
            onClick={() => {
              setFeedback(null);
              setCapturing(true);
            }}
          >
            Change
          </Button>
        </>
      )}
    </Row>
  );
}

const SENSITIVITY: [string, string][] = [
  ["0.002", "Standard"],
  ["0.0005", "Quiet voice"],
  ["0", "No silence filter"],
];

function sensitivityValue(threshold: number): string {
  const match = SENSITIVITY.find(([v]) => Math.abs(Number(v) - threshold) < 1e-6);
  return match ? match[0] : String(threshold);
}

const VOICE_COMMANDS: [string, string][] = [
  ["“comma”, “question mark”, “full stop”", ", ? ."],
  ["“add a period”, “add a colon”", ". :"],
  ["“new line”, “new paragraph”", "Line breaks"],
  ["“scratch that”, “sorry, remove that”", "Deletes the last sentence"],
  ["“at the rate KV”, “at sign KV”", "@KV"],
];

export function Settings() {
  const { workspace, saveSettings, run } = useStore();
  const busy = useBusy();
  const { settings, gpuName, models } = workspace;
  const shortcut = shortcutName(settings.shortcut);
  const set = <K extends keyof AppSettings>(key: K) => (value: AppSettings[K]) => saveSettings((s) => ({ ...s, [key]: value }));

  const model = models.find((m) => m.id === settings.selectedModelID);
  const languageName = LANGUAGES.find(([code]) => code === settings.language)?.[1] ?? settings.language;
  const unsupported =
    model && settings.language !== "auto" && model.languages.length > 0 && !model.languages.includes(settings.language);

  const sensitivityOptions = SENSITIVITY.some(([v]) => v === sensitivityValue(settings.silenceThreshold))
    ? SENSITIVITY
    : [...SENSITIVITY, [String(settings.silenceThreshold), "Custom"] as [string, string]];

  return (
    <>
      <PageHeader title="Settings" />

      <Section title="Dictation">
        <Row
          title="Microphone"
          detail="Used for dictation and meeting notes. A chosen mic is used whenever it's connected; otherwise OpenDictate follows your Windows sound settings."
        >
          <MicrophonePicker />
        </Row>
        <ShortcutRecorder />
        <Row
          title="Shortcut behavior"
          detail={`Hold ${shortcut} while you speak, or press it once and click the capsule when you're done.`}
        >
          <Select<DictationMode>
            value={settings.dictationMode}
            options={[
              ["hold", "Hold to talk"],
              ["toggle", "Press to toggle"],
            ]}
            disabled={busy}
            onChange={set("dictationMode")}
          />
        </Row>
        <Row
          title="Spoken language"
          detail={
            unsupported
              ? `${model.name} doesn't recognize ${languageName}. Choose a Whisper model in Models, or pick another language.`
              : "Choose a language for more consistent recognition."
          }
        >
          <Select value={settings.language} options={LANGUAGES} width={220} onChange={set("language")} />
        </Row>
        <Row title="Input sensitivity" detail="Quiet voices may need a lower filter. More background noise can pass through.">
          <Select
            value={sensitivityValue(settings.silenceThreshold)}
            options={sensitivityOptions}
            disabled={busy}
            onChange={(v) => saveSettings((s) => ({ ...s, silenceThreshold: Number(v) }))}
          />
        </Row>
        <Row
          title="Paste into the active app"
          detail="Types the text where your cursor is with Ctrl+V. Apps running as administrator can't receive it, so the text is copied for you to paste instead."
        >
          <Toggle label="Paste into the active app" on={settings.autoPaste} onChange={set("autoPaste")} />
        </Row>
        <Row title="Copy to clipboard" detail="Keep each dictation ready to paste again. When off, your clipboard is left as it was.">
          <Toggle label="Copy to clipboard" on={settings.copyToClipboard} onChange={set("copyToClipboard")} />
        </Row>
        <Row
          title="Learn from my corrections"
          detail="When you fix a misheard word in History, or in the text field within 30 seconds of a paste, it is added to Vocabulary."
        >
          <Toggle label="Learn from my corrections" on={settings.learnCorrections} onChange={set("learnCorrections")} />
        </Row>
      </Section>

      <Section
        title="Voice commands"
        footer="Say these while dictating in English or with Detect automatically. Words like “the Oxford comma” stay as written."
      >
        {VOICE_COMMANDS.map(([said, result]) => (
          <div className="row settings-command" key={said}>
            <div className="row-label">{said}</div>
            <div className="row-control muted">{result}</div>
          </div>
        ))}
      </Section>

      <Section title="Workspace">
        <Row title="Floating recording capsule" detail="Keep a small voice control just above the taskbar.">
          <Toggle label="Floating recording capsule" on={settings.showCapsule} onChange={set("showCapsule")} />
        </Row>
        <Row title="Open at sign-in" detail="Have OpenDictate ready in the system tray when Windows starts.">
          <Toggle label="Open at sign-in" on={settings.launchAtLogin} onChange={set("launchAtLogin")} />
        </Row>
        <Row title="Save dictation history" detail="Store text on this PC so you can find and reuse it later.">
          <Toggle label="Save dictation history" on={settings.saveHistory} onChange={set("saveHistory")} />
        </Row>
      </Section>

      <Section title="Performance">
        <Row
          title="Use graphics card"
          detail={
            gpuName
              ? `Speeds up transcription with ${gpuName}. Turn this off if dictation fails or other apps slow down.`
              : "No compatible graphics card was found, so speech models run on the processor."
          }
        >
          <Toggle label="Use graphics card" on={settings.useGpu} onChange={set("useGpu")} disabled={busy} />
        </Row>
      </Section>

      <Section
        title="Privacy"
        footer={
          <span className="hstack" style={{ alignItems: "flex-start", gap: 6 }}>
            <Lock size={12} style={{ marginTop: 3, flexShrink: 0 }} aria-hidden="true" />
            Built-in models process speech on this PC. A selected API connection sends audio to its endpoint, and its key is kept in
            Windows Credential Manager. Temporary recordings are removed after processing; external providers control their own
            retention.
          </span>
        }
      >
        <Row
          title="Microphone access"
          detail="Needed to hear your voice. If dictation can't hear you, turn on “Let desktop apps access your microphone” in Windows Settings."
        >
          <Button icon={ExternalLink} onClick={() => run(() => api.openWindowsSettings("privacy-microphone"))}>
            Open Windows Settings
          </Button>
        </Row>
      </Section>

      <Section title="Help">
        <Row title="Welcome guide" detail="Walk through choosing a model, setting up your microphone and trying your first dictation.">
          <Button icon={Sparkles} onClick={() => run(() => api.showOnboarding())}>
            Show welcome guide
          </Button>
        </Row>
        <Row title="Data folder" detail="Your settings, history, vocabulary and downloaded models are stored here, on this PC.">
          <Button icon={FolderOpen} onClick={() => run(() => api.openDataFolder())}>
            Open folder
          </Button>
        </Row>
      </Section>
    </>
  );
}
