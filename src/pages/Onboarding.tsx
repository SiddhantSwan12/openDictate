// Port of BetterWispr's OnboardingView: a short welcome guide shown in place of the main window.
import type { LucideIcon } from "lucide-react";
import { BookText, Check, Cpu, ExternalLink, Gift, Infinity as InfinityIcon, Keyboard, Lock, Mic, MicVocal, ShieldCheck, WifiOff } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { api, type ModelView } from "../lib/api";
import { useLevel, useStore } from "../lib/store";
import { Badge, Banner, Button, Progress, Row, Section, Tile } from "../components/ui";
import { BrandMark } from "../components/BrandMark";
import { MicrophonePicker } from "./Settings";
import "./Onboarding.css";

type StepId = "welcome" | "model" | "microphone" | "practice";

const STEPS: { id: StepId; title: string; subtitle: string; icon?: LucideIcon; color: string }[] = [
  {
    id: "welcome",
    title: "Talk instead of type",
    subtitle: "Speak in any app and your words appear where your cursor is.",
    color: "var(--accent)",
  },
  {
    id: "model",
    title: "Get a speech model",
    subtitle: "Download one model once and dictate offline. You can switch any time in Models.",
    icon: Cpu,
    color: "#a855f7",
  },
  {
    id: "microphone",
    title: "Choose your microphone",
    subtitle: "OpenDictate only listens while you use your shortcut.",
    icon: Mic,
    color: "#0a84ff",
  },
  {
    id: "practice",
    title: "Try it now",
    subtitle: "Say a sentence and watch it turn into text.",
    icon: MicVocal,
    color: "#ef4444",
  },
];

/** Models offered in the guide, in order, with why you'd pick each. */
const CHOICES: { id: string; tag: string; note: string }[] = [
  {
    id: "parakeet-v3",
    tag: "Fastest",
    note: "Recommended for speed. Understands English and 24 other European languages.",
  },
  {
    id: "whisper-turbo",
    tag: "Most accurate",
    note: "Recommended for accuracy and for languages Parakeet doesn't cover, such as Hindi, Japanese and Chinese. Fastest with a graphics card.",
  },
];

function clampStep(step: number): number {
  return Math.min(STEPS.length - 1, Math.max(0, Math.floor(step) || 0));
}

function progressLabel(progress: number): string {
  return progress < 0.9 ? `Downloading… ${Math.floor((progress / 0.9) * 100)}%` : "Setting up on this PC…";
}

export function Onboarding() {
  const { workspace, saveSettings, run, error, setError } = useStore();
  const [step, setStep] = useState(() => clampStep(workspace.settings.onboardingStep));
  const current = STEPS[step];
  const last = step === STEPS.length - 1;

  function go(target: number) {
    const next = clampStep(target);
    setStep(next);
    saveSettings((s) => ({ ...s, onboardingStep: next }));
  }

  const finish = () => run(() => api.finishOnboarding());

  return (
    <div className="onboarding">
      <main className="onboarding-scroll">
        <div className="onboarding-page" key={current.id}>
          {error && (
            <Banner tone="error" onClose={() => setError(null)}>
              {error}
            </Banner>
          )}
          <div className="onboarding-hero" aria-hidden="true">
            {current.icon ? (
              <Tile icon={current.icon} color={current.color} size={88} />
            ) : (
              <>
                <BrandMark size={88} />
                <div className="onboarding-bars">
                  {[0.5, 0.9, 0.65, 1, 0.55].map((h, i) => (
                    <span key={i} style={{ height: `${h * 100}%`, animationDelay: `${i * 0.12}s` }} />
                  ))}
                </div>
              </>
            )}
          </div>
          <div className="onboarding-heading">
            <h1>{current.title}</h1>
            <p>{current.subtitle}</p>
          </div>
          {current.id === "welcome" && <Welcome />}
          {current.id === "model" && <ModelStep />}
          {current.id === "microphone" && <MicrophoneStep />}
          {current.id === "practice" && <PracticeStep onChooseModel={() => go(1)} />}
        </div>
      </main>

      <footer className="onboarding-bar">
        <div className="hstack">
          {step > 0 && (
            <Button size="large" onClick={() => go(step - 1)}>
              Back
            </Button>
          )}
          {!last && (
            <Button size="large" variant="ghost" onClick={finish}>
              Skip
            </Button>
          )}
        </div>
        <div className="onboarding-dots" role="img" aria-label={`Step ${step + 1} of ${STEPS.length}`}>
          {STEPS.map((s, i) => (
            <span key={s.id} className={i === step ? "on" : ""} />
          ))}
        </div>
        <div className="hstack" style={{ justifyContent: "flex-end" }}>
          {last ? (
            <Button size="large" variant="primary" onClick={finish}>
              Done
            </Button>
          ) : (
            <Button size="large" variant="primary" onClick={() => go(step + 1)}>
              Continue
            </Button>
          )}
        </div>
      </footer>
    </div>
  );
}

function Feature({ icon: Icon, children }: { icon: LucideIcon; children: ReactNode }) {
  return (
    <li>
      <Icon size={18} aria-hidden="true" />
      <span>{children}</span>
    </li>
  );
}

function Welcome() {
  const { workspace } = useStore();
  const verb = workspace.settings.dictationMode === "hold" ? "Hold" : "Press";
  return (
    <ul className="onboarding-features">
      <Feature icon={Keyboard}>
        {verb} <strong>{workspace.shortcutName}</strong> to start dictating, in any app
      </Feature>
      <Feature icon={Gift}>Free, with no account or subscription</Feature>
      <Feature icon={Lock}>Private: your voice is turned into text on this PC and never uploaded</Feature>
      <Feature icon={WifiOff}>Works offline once a speech model is downloaded</Feature>
      <Feature icon={InfinityIcon}>Unlimited: dictate as much as you like</Feature>
      <Feature icon={BookText}>Add names and terms in Vocabulary so they're spelled right</Feature>
    </ul>
  );
}

function ModelStep() {
  const { workspace } = useStore();
  const choices = CHOICES.map((c) => ({ ...c, model: workspace.models.find((m) => m.id === c.id) })).filter(
    (c): c is (typeof CHOICES)[number] & { model: ModelView } => c.model !== undefined,
  );
  const selected = workspace.models.find((m) => m.id === workspace.settings.selectedModelID);
  const usingOther = selected && selected.installed && !CHOICES.some((c) => c.id === selected.id);

  return (
    <div className="stack onboarding-wide">
      {choices.map((c) => (
        <ModelChoice key={c.id} model={c.model} tag={c.tag} note={c.note} />
      ))}
      {choices.length === 0 && <Banner tone="warning">No built-in models were found. Open Models after setup to choose one.</Banner>}
      <p className="faint onboarding-center">
        {usingOther
          ? `You're using ${selected.name}, which is ready. You can keep it or download one of these.`
          : "A download keeps going if you continue. Everything stays on this PC."}
      </p>
    </div>
  );
}

function ModelChoice({ model, tag, note }: { model: ModelView; tag: string; note: string }) {
  const { workspace, session, run } = useStore();
  const installation = session.installation?.id === model.id ? session.installation : null;
  const installing = installation !== null && installation.failure === null;
  const busyElsewhere = session.installation !== null && session.installation.failure === null && !installation;
  const selected = workspace.settings.selectedModelID === model.id;
  const ready = model.installed && selected;

  let control;
  if (installing) {
    control = <Button onClick={() => run(() => api.cancelInstallation())}>Cancel</Button>;
  } else if (ready) {
    control = (
      <Badge tone="success">
        <Check size={12} aria-hidden="true" /> Ready
      </Badge>
    );
  } else if (model.installed) {
    control = <Button onClick={() => run(() => api.selectModel(model.id))}>Use {model.name}</Button>;
  } else {
    control = (
      <Button
        variant="primary"
        disabled={busyElsewhere}
        title={busyElsewhere ? "Another model is downloading" : undefined}
        onClick={() => run(() => api.installModel(model.id))}
      >
        {installation ? "Retry" : `Download ${model.sizeLabel}`}
      </Button>
    );
  }

  return (
    <div className={`card onboarding-model ${ready ? "selected" : ""}`}>
      <div className="hstack" style={{ alignItems: "flex-start", gap: 16 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div className="hstack" style={{ gap: 8 }}>
            <h2>{model.name}</h2>
            <Badge tone="accent">{tag}</Badge>
          </div>
          <p className="muted">{note}</p>
          <span className="faint">{model.sizeLabel} download</span>
        </div>
        <div className="row-control">{control}</div>
      </div>
      {installation && (
        <div className="stack" style={{ gap: 6, marginTop: 12 }}>
          {installing && <Progress value={installation.progress} />}
          <span className={installing ? "faint" : "onboarding-error"}>
            {installation.failure ? `Download stopped: ${installation.failure}` : progressLabel(installation.progress)}
          </span>
        </div>
      )}
    </div>
  );
}

function MicrophoneStep() {
  const { run } = useStore();
  return (
    <div className="stack onboarding-wide">
      <Section>
        <Row title="Microphone" detail="Automatic follows the default input in Windows sound settings.">
          <MicrophonePicker width={260} />
        </Row>
        <Row title="Microphone access" detail="If Windows blocks it, turn on “Let desktop apps access your microphone”.">
          <Button icon={ExternalLink} onClick={() => run(() => api.openWindowsSettings("privacy-microphone"))}>
            Privacy settings
          </Button>
        </Row>
      </Section>
      <div className="onboarding-note">
        <ShieldCheck size={18} aria-hidden="true" />
        <span className="muted">
          That's the only permission OpenDictate needs. Text is pasted with Ctrl+V. Apps running as administrator can't receive
          pasted text, so in those your words are copied and you can paste them yourself.
        </span>
      </div>
    </div>
  );
}

function Waveform({ active }: { active: boolean }) {
  const level = useLevel();
  const [heights, setHeights] = useState<number[]>(Array(11).fill(4));
  useEffect(() => {
    if (!active) {
      setHeights(Array(11).fill(4));
      return;
    }
    setHeights((prev) => [...prev.slice(1), 4 + Math.min(1, level * 1.5) * 36 * (0.6 + Math.random() * 0.4)]);
  }, [level, active]);
  return (
    <div className="onboarding-wave" aria-hidden="true">
      {heights.map((h, i) => (
        <span key={i} style={{ height: h }} />
      ))}
    </div>
  );
}

function PracticeStep({ onChooseModel }: { onChooseModel: () => void }) {
  const { workspace, session } = useStore();
  const [draft, setDraft] = useState("");
  const shortcut = workspace.shortcutName;
  const phase = session.phase;
  const busy = phase.kind === "preparing" || phase.kind === "recording" || phase.kind === "transcribing";
  const model = workspace.models.find((m) => m.id === workspace.settings.selectedModelID);
  const modelMissing = !model || (model.engine !== "api" && !model.installed);

  let text: string;
  let tone: "idle" | "live" | "problem" = "idle";
  if (phase.kind === "failed") {
    text = `${phase.title} ${phase.message}`;
    tone = "problem";
  } else if (phase.kind === "unpasted") {
    text = "Copied, not pasted. Click the box below and press Ctrl+V.";
    tone = "problem";
  } else if (session.partialTranscript) {
    text = session.partialTranscript;
    tone = "live";
  } else if (busy) {
    text = session.statusMessage || (phase.kind === "transcribing" ? "Transcribing on this PC…" : "Listening…");
  } else if (workspace.settings.dictationMode === "hold") {
    text = `Hold ${shortcut}, say something, then let go.`;
  } else {
    text = `Press ${shortcut}, say something, then press it again.`;
  }

  return (
    <div className="stack onboarding-wide">
      {modelMissing && (
        <Banner tone="warning">
          <div className="hstack" style={{ justifyContent: "space-between" }}>
            <span>
              {session.installation && !session.installation.failure
                ? "Your speech model is still downloading. You can try dictating as soon as it's ready."
                : "Download a speech model first."}
            </span>
            <Button size="small" onClick={onChooseModel}>
              Choose a model
            </Button>
          </div>
        </Banner>
      )}
      <div className={`onboarding-practice ${tone}`}>
        {phase.kind === "transcribing" ? <div className="spinner" aria-hidden="true" /> : <Waveform active={phase.kind === "recording"} />}
        <p aria-live="polite">{text}</p>
      </div>
      <label className="field">
        <span>Click here, then dictate. Your words are pasted where the cursor is.</span>
        <textarea
          className="textarea"
          rows={4}
          value={draft}
          placeholder="Your dictation appears here"
          onChange={(e) => setDraft(e.target.value)}
          autoFocus
        />
      </label>
    </div>
  );
}
