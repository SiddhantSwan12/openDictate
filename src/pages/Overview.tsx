// Port of BetterWispr's OverviewView (Features/Dashboard/DashboardView.swift), adapted to Windows.
import { Copy, Download, Keyboard, Mic, MousePointerClick, Square } from "lucide-react";
import { api, durationLabel, wordCount, type ModelView, type Transcript } from "../lib/api";
import { useRecordingDuration, useStore } from "../lib/store";
import { BrandMark } from "../components/BrandMark";
import { Badge, Banner, Button, Card, Progress, Row, Section, ShortcutKeys, Toggle } from "../components/ui";
import "./Overview.css";

export function Overview() {
  const { session, workspace, setPage, run, saveSettings } = useStore();
  const { settings, history } = workspace;
  const phase = session.phase.kind;
  const busy = phase === "preparing" || phase === "recording" || phase === "transcribing";
  const selected = workspace.models.find((m) => m.id === settings.selectedModelID);
  const totalWords = history.reduce((sum, t) => sum + wordCount(t.text), 0);
  const totalDuration = history.reduce((sum, t) => sum + t.duration, 0);
  const hold = settings.dictationMode === "hold";

  return (
    <>
      <Card pad className="overview-hero">
        <BrandMark size={52} />
        <div className="overview-hero-text">
          <h1 style={{ fontSize: 22, lineHeight: "28px" }}>OpenDictate</h1>
          <span className="muted">
            {hold ? "Hold " : "Press "}
            <ShortcutKeys name={workspace.shortcutName} />
            {" in any app to turn your voice into text."}
          </span>
        </div>
        <RecordButton />
      </Card>

      {session.statusMessage && (
        <Banner tone={phase === "failed" ? "error" : "info"}>
          {session.phase.kind === "failed" && <strong>{session.phase.title}. </strong>}
          {session.statusMessage}
        </Banner>
      )}

      {selected && <ModelPrompt model={selected} />}

      {session.partialTranscript && !busy && !settings.saveHistory && (
        <Section
          title="Latest dictation"
          action={
            <Button size="small" icon={Copy} onClick={() => run(() => api.copyText(session.partialTranscript))}>
              Copy
            </Button>
          }
          footer="History is off. Copy these words before starting another dictation."
        >
          <div className="card-pad selectable">{session.partialTranscript}</div>
        </Section>
      )}

      <HowTo hold={hold} shortcutName={workspace.shortcutName} />

      <Section title="Usage">
        <div className="grid-3 overview-stats">
          <Stat label="Saved words" value={totalWords.toLocaleString()} />
          <Stat label="Dictation time" value={durationLabel(totalDuration)} />
          <Stat label="Saved dictations" value={history.length.toLocaleString()} />
        </div>
      </Section>

      <Section title="Setup">
        <Row
          title="Microphone"
          detail={
            workspace.microphones.length === 0
              ? "No microphone found. Plug one in, or check Windows privacy settings."
              : `Captures your voice. Using ${settings.microphone?.name ?? `the Windows default${workspace.defaultMicrophone ? ` (${workspace.defaultMicrophone.name})` : ""}`}.`
          }
        >
          <Button onClick={() => run(() => api.openWindowsSettings("privacy-microphone"))}>Privacy settings…</Button>
        </Row>
        <Row title="Paste into apps" detail="Types your words where you're working, using Ctrl+V. No extra permission needed.">
          <Toggle label="Paste into apps" on={settings.autoPaste} onChange={(on) => saveSettings((s) => ({ ...s, autoPaste: on }))} />
        </Row>
        <Row title="Speech model" detail={selected ? modelDetail(selected, workspace.gpuName, settings.useGpu) : "No model selected."}>
          {selected && !selected.installed && <Badge tone="warning">Not downloaded</Badge>}
          <Button onClick={() => setPage("models")}>Change…</Button>
        </Row>
      </Section>

      <Section
        title="Recent dictations"
        action={
          history.length > 0 && (
            <Button size="small" variant="ghost" onClick={() => setPage("history")}>
              Show all
            </Button>
          )
        }
      >
        {history.length === 0 ? (
          <div className="card-pad muted">Start a dictation and your words will appear here, ready to copy or use again.</div>
        ) : (
          history.slice(0, 3).map((t) => <RecentRow key={t.id} transcript={t} />)
        )}
      </Section>
    </>
  );
}

function RecordButton() {
  const { session, run } = useStore();
  const phase = session.phase.kind;
  const recording = phase === "recording";
  const duration = useRecordingDuration(session.recordingDuration);
  return (
    <div className="overview-record">
      {recording && (
        <span className="hstack faint" aria-live="polite">
          <span className="pulse-dot" />
          {durationLabel(duration)}
        </span>
      )}
      <Button
        variant="primary"
        size="large"
        icon={recording ? Square : Mic}
        disabled={phase === "preparing" || phase === "transcribing"}
        onClick={() => run(() => api.toggleRecording())}
      >
        {recording ? "Finish dictation" : phase === "preparing" ? "Preparing…" : phase === "transcribing" ? "Transcribing…" : "Start dictating"}
      </Button>
    </div>
  );
}

/** Shows download progress, model loading, or a prompt when the selected model isn't on this PC yet. */
function ModelPrompt({ model }: { model: ModelView }) {
  const { session, setPage, run } = useStore();
  const install = session.installation;
  if (install) {
    const name = install.id === model.id ? model.name : "the model";
    return (
      <Card pad>
        <div className="stack" style={{ gap: 8 }}>
          <div className="hstack">
            <span style={{ flex: 1 }}>
              {install.failure
                ? `Couldn't download ${name}: ${install.failure}`
                : install.progress > 0.9
                  ? `Verifying ${name}…`
                  : `Downloading ${name}… ${Math.round(install.progress * 100)}%`}
            </span>
            {!install.failure && (
              <Button size="small" onClick={() => run(() => api.cancelInstallation())}>
                Cancel
              </Button>
            )}
          </div>
          {!install.failure && <Progress value={install.progress} />}
        </div>
      </Card>
    );
  }
  if (session.loadingModel) {
    return (
      <Banner tone="info">
        Loading {model.name}. Your first dictation starts as soon as it's ready.
      </Banner>
    );
  }
  if (!model.installed) {
    return (
      <Banner tone="warning">
        <div className="hstack" style={{ alignItems: "flex-start", gap: 12 }}>
          <div style={{ flex: 1 }}>
            <strong>{model.name} isn't downloaded yet.</strong>
            <div className="muted">
              Download it once ({model.sizeLabel}) and dictation works offline, free and unlimited. Your voice never leaves this PC.
            </div>
          </div>
          <Button size="small" icon={Download} onClick={() => run(() => api.installModel(model.id))}>
            Download
          </Button>
          <Button size="small" variant="ghost" onClick={() => setPage("models")}>
            Models
          </Button>
        </div>
      </Banner>
    );
  }
  return null;
}

function HowTo({ hold, shortcutName }: { hold: boolean; shortcutName: string }) {
  const steps = [
    { icon: MousePointerClick, title: "Click where you type", detail: "Any app: chat, email, documents, AI prompts or the browser." },
    {
      icon: Keyboard,
      title: hold ? "Hold the shortcut and speak" : "Press the shortcut and speak",
      detail: <ShortcutKeys name={shortcutName} />,
    },
    {
      icon: Mic,
      title: hold ? "Release to paste" : "Press again to paste",
      detail: "Your words appear where you were typing. Press Esc to cancel.",
    },
  ];
  return (
    <Section title="How to dictate" footer="Speech is turned into text on this PC. It's free, private and works offline.">
      <div className="grid-3 overview-steps">
        {steps.map((step, i) => (
          <div key={i} className="overview-step">
            <span className="overview-step-number" aria-hidden="true">
              {i + 1}
            </span>
            <div className="stack" style={{ gap: 4 }}>
              <h3>{step.title}</h3>
              <div className="faint">{step.detail}</div>
            </div>
          </div>
        ))}
      </div>
    </Section>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <div className="stat-value">{value}</div>
      <div className="faint">{label}</div>
    </div>
  );
}

function RecentRow({ transcript }: { transcript: Transcript }) {
  const { run } = useStore();
  const date = new Date(transcript.createdAt);
  const when = date.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  return (
    <div className="list-item overview-recent">
      <div style={{ flex: 1, minWidth: 0 }}>
        <div className="faint">
          {when} · {durationLabel(transcript.duration)}
          {transcript.app && ` · ${transcript.app}`}
        </div>
        <div className="clamp-3 selectable">{transcript.text}</div>
      </div>
      <Button variant="ghost" size="icon" icon={Copy} title="Copy dictation" onClick={() => run(() => api.copyText(transcript.text))} />
    </div>
  );
}

function modelDetail(model: ModelView, gpuName: string | null, useGpu: boolean): string {
  if (model.engine === "api") {
    const endpoint = model.detail.split(" · ").slice(1).join(" · ");
    return `${model.name}. Audio sent to ${endpoint || "your endpoint"}.`;
  }
  const where = useGpu && gpuName ? `Runs on this PC using ${gpuName}.` : "Runs on this PC.";
  return `${model.name}. ${where}`;
}
