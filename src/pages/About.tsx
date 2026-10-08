// Port of BetterWispr's AboutView (Features/About/AboutView.swift), without Sparkle update checks.
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { BookOpen, ExternalLink, FolderOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import { BrandMark } from "../components/BrandMark";
import { Button, Card, Row, Section, ShortcutKeys } from "../components/ui";

const UPSTREAM = "https://github.com/opennookorg/betterwispr";
const REPO = "https://github.com/SiddhantSwan12/openDictate";

const CREDITS: { name: string; detail: string; url: string }[] = [
  { name: "BetterWispr", detail: "The macOS app OpenDictate is ported from, by Kartik Labhshetwar. Apache 2.0.", url: UPSTREAM },
  {
    name: "NVIDIA Parakeet",
    detail: "Fast English and European speech recognition, using the ONNX exports by istupakov.",
    url: "https://huggingface.co/istupakov",
  },
  { name: "OpenAI Whisper", detail: "Multilingual speech recognition, run with whisper.cpp (ggml models).", url: "https://github.com/ggml-org/whisper.cpp" },
  { name: "transcribe-rs", detail: "Rust speech-to-text engine bindings. MIT license.", url: "https://github.com/cjpais/transcribe-rs" },
];

export function About() {
  const { workspace, run } = useStore();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  const open = (url: string) => run(() => openUrl(url));

  return (
    <>
      <Card pad>
        <div className="stack" style={{ alignItems: "center", textAlign: "center", padding: "12px 0", gap: 10 }}>
          <BrandMark size={72} />
          <h1>OpenDictate</h1>
          <span className="muted selectable">{version ? `Version ${version}` : "Development"}</span>
          <p className="muted" style={{ maxWidth: 480, margin: 0 }}>
            Private dictation for Windows. Hold <ShortcutKeys name={workspace.shortcutName} /> in any app, speak, and your words appear where
            you're typing. Speech is processed on this PC: free, unlimited and offline.
          </p>
        </div>
      </Card>

      <Section title="Help">
        <Row title="Welcome guide" detail="Walk through setup again: microphone, model and shortcut.">
          <Button icon={BookOpen} onClick={() => run(() => api.showOnboarding())}>
            Show welcome guide
          </Button>
        </Row>
        <Row title="Data folder" detail="Your settings, history, vocabulary and downloaded models are stored here.">
          <Button icon={FolderOpen} onClick={() => run(() => api.openDataFolder())}>
            Open data folder
          </Button>
        </Row>
      </Section>

      <Section
        title="Open source"
        footer="OpenDictate is free and open source under the Apache 2.0 license. If it saves you some typing, a star on GitHub helps others find it."
      >
        <Row title="Source code" detail="OpenDictate is a free, open-source Windows port of BetterWispr. Browse the code and every release.">
          <Button icon={ExternalLink} onClick={() => open(REPO)}>
            View on GitHub
          </Button>
        </Row>
        <Row title="Feedback" detail="Report a bug or suggest a feature for OpenDictate.">
          <Button icon={ExternalLink} onClick={() => open(`${REPO}/issues`)}>
            Open an issue
          </Button>
        </Row>
      </Section>

      <Section title="Credits" footer="Model weights are downloaded only when you choose to install them, and keep their own licenses.">
        {CREDITS.map((c) => (
          <Row key={c.name} title={c.name} detail={c.detail}>
            <Button variant="ghost" size="icon" icon={ExternalLink} title={`Open ${c.name} website`} onClick={() => open(c.url)} />
          </Row>
        ))}
        <Row title="BetterWispr created by" detail="Kartik Labhshetwar">
          <Button variant="ghost" onClick={() => open("https://x.com/code_kartik")}>
            @code_kartik on X
          </Button>
        </Row>
      </Section>
    </>
  );
}
