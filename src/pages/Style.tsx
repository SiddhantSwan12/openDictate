// Port of BetterWispr's StyleView: automatic cleanup and a writing tone for each kind of app.
import { TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { api, type AppSettings, type CleanupLevel, type StyleContext, type StyleTone } from "../lib/api";
import { useStore } from "../lib/store";
import { Button, PageHeader, Row, Section, Segmented } from "../components/ui";
import "./Style.css";

const CONTEXTS: StyleContext[] = ["personal", "work", "email", "other"];

const CONTEXT_TITLES: Record<StyleContext, string> = {
  personal: "Personal messages",
  work: "Work messages",
  email: "Email",
  other: "Other apps",
};

const CONTEXT_APPS: Record<StyleContext, string> = {
  personal: "WhatsApp, Telegram, Discord, Signal and Messenger.",
  work: "Slack, Microsoft Teams, Zoom, Webex and Mattermost.",
  email: "Outlook, Mail, Thunderbird and Superhuman.",
  other:
    "ChatGPT, Claude, Cursor, Word, Notion, Notepad, browsers and every other app. Browsers use this tone because OpenDictate can't see which website is open. Tones only change English dictation.",
};

const TONES: Record<StyleContext, StyleTone[]> = {
  personal: ["formal", "casual", "veryCasual"],
  work: ["formal", "casual", "excited"],
  email: ["formal", "casual", "excited"],
  other: ["formal", "casual", "excited"],
};

const TONE_TITLES: Record<StyleTone, string> = {
  formal: "Formal",
  casual: "Casual",
  veryCasual: "Very casual",
  excited: "Excited",
};

const TONE_DETAILS: Record<StyleTone, string> = {
  formal: "Caps and punctuation, exactly as cleaned up.",
  casual: "Caps, fewer commas and no final period.",
  veryCasual: "No caps, fewer commas and no final period.",
  excited: "Ends on an exclamation mark.",
};

/** Each context's sample, already run through the backend's style formatter for every tone it offers. */
const SAMPLES: Record<StyleContext, Partial<Record<StyleTone, string>>> = {
  personal: {
    formal: "Hey, are you around for dinner tonight? Let's do 7 if that works for you.",
    casual: "Hey are you around for dinner tonight? Let's do 7 if that works for you",
    veryCasual: "hey are you around for dinner tonight? let's do 7 if that works for you",
  },
  work: {
    formal: "Hey, when you have a minute, let's go over the launch numbers.",
    casual: "Hey when you have a minute let's go over the launch numbers",
    excited: "Hey, when you have a minute, let's go over the launch numbers!",
  },
  email: {
    formal: "Hi Sam,\n\nThanks for the quick call today. Looking forward to working together.\n\nBest,\nAlex",
    casual: "Hi Sam, thanks for the quick call today. Looking forward to working together.\n\nBest,\nAlex",
    excited: "Hi Sam,\n\nThanks for the quick call today. Looking forward to working together!\n\nBest,\nAlex",
  },
  other: {
    formal: "So far, the new plan is working well.\n\nTomorrow I want to finish the draft, especially the summary.",
    casual: "So far the new plan is working well.\n\nTomorrow I want to finish the draft especially the summary",
    excited: "So far, the new plan is working well.\n\nTomorrow I want to finish the draft, especially the summary!",
  },
};

const CLEANUP_LEVELS: [CleanupLevel, string][] = [
  ["none", "None"],
  ["light", "Light"],
  ["medium", "Medium"],
];

const CLEANUP_DETAILS: Record<CleanupLevel, string> = {
  none: "Keeps exactly what you said, including filler words.",
  light: "Removes filler words like “um” and “you know”, stutters like “we we”, and corrections like “at 5, no, at 6”.",
  medium: "Also edits English dictation for clarity and conciseness with your notes model.",
};

const SPOKEN_SAMPLE = "um hey Sam are we are we still on for lunch? I think uh we should leave early to beat the traffic.";
const CLEANED_SAMPLES: Record<CleanupLevel, string> = {
  none: SPOKEN_SAMPLE,
  light: "Hey Sam are we still on for lunch? I think we should leave early to beat the traffic.",
  medium: "Hey Sam, are we still on for lunch? We should leave early to beat the traffic.",
};

const CLI_NAMES = { claudeCode: "Claude Code", codex: "Codex" } as const;

/** Mirrors the backend's `notes_model_name`. */
function notesModelName(settings: AppSettings): string {
  const selection = settings.notesSelection;
  switch (selection.kind) {
    case "none":
      return "No notes model";
    case "ollama":
      return `Ollama · ${selection.value}`;
    case "connection": {
      const connection = settings.notesConnections.find((c) => c.id === selection.value);
      return connection ? `${connection.name} · ${connection.modelID}` : "Missing notes connection";
    }
    case "cli": {
      const model = selection.value === "claudeCode" ? settings.claudeNotesModel : settings.codexNotesModel;
      return `${CLI_NAMES[selection.value]} · ${model || "Choose a model"}`;
    }
  }
}

function cleanupFooter(settings: AppSettings): string {
  const original = "Your original words are never lost. In History, open Original transcription and choose Use original.";
  if (settings.cleanup !== "medium") return original;
  const selection = settings.notesSelection;
  let destination = "";
  switch (selection.kind) {
    case "ollama":
      destination = "Medium edits with Ollama on this PC. ";
      break;
    case "cli":
      destination = `Medium sends English dictation to ${CLI_NAMES[selection.value]} through your subscription, which adds a few seconds. `;
      break;
    case "connection":
      destination = `Medium sends English dictation to ${notesModelName(settings)} with your API key. `;
      break;
    case "none":
      break;
  }
  return `${destination}It uses the same model as meeting notes, and results vary by model. Other languages, very short dictations and edits that fail, take too long or rewrite too much get Light cleanup. ${original}`;
}

function Example({ title, text }: { title: string; text: string }) {
  return (
    <div className="style-example" role="group" aria-label={title}>
      <div>{title}</div>
      <p className="muted selectable">{text}</p>
    </div>
  );
}

export function Style() {
  const { workspace, saveSettings, setPage } = useStore();
  const { settings } = workspace;
  const [notesProblem, setNotesProblem] = useState<string | null>(null);
  const selectionKey = JSON.stringify(settings.notesSelection);

  useEffect(() => {
    if (settings.cleanup !== "medium") return;
    let live = true;
    api
      .notesAvailability()
      .then((reason) => live && setNotesProblem(reason))
      .catch(() => live && setNotesProblem(null));
    return () => {
      live = false;
    };
  }, [settings.cleanup, selectionKey, settings.claudeNotesModel, settings.codexNotesModel, settings.notesConnections.length]);

  const tone = (context: StyleContext): StyleTone => settings.styles[context] ?? "formal";

  return (
    <>
      <PageHeader title="Style" subtitle="Choose how much OpenDictate tidies up your words, and the tone it writes in for each kind of app." />

      <Section title="Every app" footer={cleanupFooter(settings)}>
        <Row title="Auto cleanup" detail={CLEANUP_DETAILS[settings.cleanup]}>
          <Segmented value={settings.cleanup} options={CLEANUP_LEVELS} onChange={(cleanup) => saveSettings((s) => ({ ...s, cleanup }))} />
        </Row>
        <div className="row style-examples">
          <Example title="You say" text={SPOKEN_SAMPLE} />
          <Example title="OpenDictate types" text={CLEANED_SAMPLES[settings.cleanup]} />
        </div>
        {settings.cleanup === "medium" && (
          <Row title="Notes model" detail={notesModelName(settings)}>
            <Button onClick={() => setPage("models")}>Choose in Models</Button>
          </Row>
        )}
        {settings.cleanup === "medium" && notesProblem && (
          <div className="row style-warning" role="status">
            <TriangleAlert size={16} aria-hidden="true" />
            <span className="muted">{notesProblem} Until then, Medium dictations get Light cleanup.</span>
          </div>
        )}
      </Section>

      {CONTEXTS.map((context) => {
        const current = tone(context);
        return (
          <Section key={context} title={CONTEXT_TITLES[context]} footer={CONTEXT_APPS[context]}>
            <Row title="Tone" detail={TONE_DETAILS[current]}>
              <Segmented
                value={current}
                options={TONES[context].map((t) => [t, TONE_TITLES[t]] as [StyleTone, string])}
                onChange={(next) => saveSettings((s) => ({ ...s, styles: { ...s.styles, [context]: next } }))}
              />
            </Row>
            <div className="row style-examples">
              <Example title="Example" text={SAMPLES[context][current] ?? SAMPLES[context].formal ?? ""} />
            </div>
          </Section>
        );
      })}
    </>
  );
}
