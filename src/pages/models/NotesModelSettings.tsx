// Port of BetterWispr's NotesModelSettings: the model that writes meeting notes and Medium cleanup edits.
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FlaskConical, NotebookPen, Plus, RefreshCw, Sparkles, Terminal } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button, Field, Row, Section, Select, Tile } from "../../components/ui";
import { api, errorText, type AppSettings, type CliCatalog, type NotesCli, type NotesSelection, type SpeechConnection } from "../../lib/api";
import { useStore } from "../../lib/store";
import { ConnectionEditor } from "./ConnectionEditor";
import { CLI_LOGIN, CLI_NAMES, CLI_SETTING, Confirm, Spinner, useBusy } from "./shared";

const CLIS: NotesCli[] = ["claudeCode", "codex"];

interface CatalogState {
  catalog: CliCatalog | null;
  error: string | null;
  loading: boolean;
}

const EMPTY_CATALOG: CatalogState = { catalog: null, error: null, loading: false };

function encode(selection: NotesSelection): string {
  return selection.kind === "none" ? "none" : `${selection.kind}:${selection.value}`;
}

function decode(value: string): NotesSelection {
  const split = value.indexOf(":");
  const kind = split < 0 ? value : value.slice(0, split);
  const rest = value.slice(split + 1);
  switch (kind) {
    case "ollama":
      return { kind: "ollama", value: rest };
    case "connection":
      return { kind: "connection", value: rest };
    case "cli":
      return { kind: "cli", value: rest as NotesCli };
    default:
      return { kind: "none" };
  }
}

function selectionName(settings: AppSettings): string {
  const s = settings.notesSelection;
  switch (s.kind) {
    case "none":
      return "The notes model";
    case "ollama":
      return `Ollama · ${s.value}`;
    case "cli": {
      const model = settings[CLI_SETTING[s.value]].trim();
      return model ? `${CLI_NAMES[s.value]} · ${model}` : CLI_NAMES[s.value];
    }
    case "connection": {
      const c = settings.notesConnections.find((c) => c.id === s.value);
      return c ? `${c.name} · ${c.modelID}` : "The notes connection";
    }
  }
}

export function NotesModelSettings() {
  const { workspace, meetings, saveSettings, run } = useStore();
  const { canEditConnections } = useBusy();
  const settings = workspace.settings;
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const idleRef = useRef(meetings.activity.kind === "idle");
  idleRef.current = meetings.activity.kind === "idle";

  const [ollama, setOllama] = useState<{ models: string[]; error: string | null; loading: boolean }>({ models: [], error: null, loading: true });
  const [catalogs, setCatalogs] = useState<Record<NotesCli, CatalogState>>({ claudeCode: EMPTY_CATALOG, codex: EMPTY_CATALOG });
  const [installed, setInstalled] = useState<Partial<Record<NotesCli, boolean>>>({});
  const [availability, setAvailability] = useState<string | null>(meetings.notesAvailability);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const testRun = useRef(0);
  const [editing, setEditing] = useState<SpeechConnection | null>(null);
  const [removing, setRemoving] = useState<SpeechConnection | null>(null);

  const refreshOllama = useCallback(async () => {
    setOllama((o) => ({ ...o, loading: true }));
    try {
      setOllama({ models: await api.ollamaModels(), error: null, loading: false });
    } catch (e) {
      setOllama({ models: [], error: errorText(e), loading: false });
    }
  }, []);

  const refreshCatalog = useCallback(
    async (cli: NotesCli) => {
      setCatalogs((c) => ({ ...c, [cli]: { ...c[cli], loading: true, error: null } }));
      api.cliInstalled(cli).then((ok) => setInstalled((i) => ({ ...i, [cli]: ok })), () => {});
      try {
        const catalog = await api.cliCatalog(cli);
        setCatalogs((c) => ({ ...c, [cli]: { catalog, error: null, loading: false } }));
        // Like BetterWispr, fill in the CLI's own default when no model has been chosen yet.
        const key = CLI_SETTING[cli];
        if (!settingsRef.current[key].trim() && catalog.defaultId && idleRef.current) {
          const defaultId = catalog.defaultId;
          await saveSettings((s) => (s[key].trim() ? s : { ...s, [key]: defaultId }));
        }
      } catch (e) {
        setCatalogs((c) => ({ ...c, [cli]: { ...c[cli], error: errorText(e), loading: false } }));
      }
    },
    [saveSettings],
  );

  useEffect(() => {
    refreshOllama();
    for (const cli of CLIS) api.cliInstalled(cli).then((ok) => setInstalled((i) => ({ ...i, [cli]: ok })), () => {});
    const s = settingsRef.current.notesSelection;
    if (s.kind === "cli") refreshCatalog(s.value);
  }, [refreshOllama, refreshCatalog]);

  const selection = settings.notesSelection;
  const selectionKey = encode(selection);
  const availabilityKey = [
    selectionKey,
    settings.claudeNotesModel,
    settings.codexNotesModel,
    settings.notesConnections.map((c) => c.id).join(","),
    installed.claudeCode,
    installed.codex,
  ].join("|");

  useEffect(() => {
    let current = true;
    api.notesAvailability().then(
      (reason) => current && setAvailability(reason),
      () => {},
    );
    return () => {
      current = false;
    };
  }, [availabilityKey]);

  // A changed choice makes an earlier test result misleading.
  useEffect(() => {
    testRun.current += 1;
    setResult(null);
    setTesting(false);
  }, [availabilityKey]);

  const choose = (value: string) => {
    const next = decode(value);
    saveSettings((s) => ({ ...s, notesSelection: next }));
    if (next.kind === "cli") refreshCatalog(next.value);
  };

  const test = async () => {
    const id = ++testRun.current;
    const name = selectionName(settings);
    setResult(null);
    setTesting(true);
    try {
      const sample = await api.testNotesModel();
      if (id === testRun.current) setResult({ ok: true, text: `${name} is working. Sample summary: ${sample}` });
    } catch (e) {
      if (id === testRun.current) setResult({ ok: false, text: errorText(e) });
    } finally {
      if (id === testRun.current) setTesting(false);
    }
  };

  const addConnection = () =>
    run(async () => {
      const fresh = await api.newConnection("openAiCompatible");
      setEditing({ ...fresh, name: "My notes model", endpoint: "https://api.openai.com/v1/chat/completions", modelID: "" });
    });

  // Picker options, keeping a saved choice visible even when it's no longer listed.
  const options: [string, string][] = [];
  if (selection.kind === "none") options.push(["none", "Choose a notes model"]);
  const ollamaNames = [...ollama.models];
  if (selection.kind === "ollama" && !ollamaNames.includes(selection.value)) ollamaNames.push(selection.value);
  for (const name of ollamaNames) options.push([`ollama:${name}`, `Ollama · ${name} · Free on this PC`]);
  for (const cli of CLIS) options.push([`cli:${cli}`, `${CLI_NAMES[cli]} · Subscription`]);
  for (const c of settings.notesConnections) options.push([`connection:${c.id}`, `${c.name} · ${c.modelID}`]);
  if (selection.kind === "connection" && !settings.notesConnections.some((c) => c.id === selection.value)) {
    options.push([selectionKey, "Missing notes connection"]);
  }

  return (
    <>
      <Section
        title="Meeting notes · Bring your own LLM"
        footer="The selected notes model processes the transcript and your notes after recording, and whenever you generate a summary. Ollama runs free on this PC. Claude Code and Codex use their signed-in subscriptions. API connections use your key and provider billing. Cloud choices send meeting text to that provider; speech recognition is selected separately. When Auto cleanup in Style is Medium, this model also edits your English dictation."
      >
        <Row
          icon={<Tile icon={NotebookPen} color="#eab308" size={32} />}
          title="Notes model"
          detail={
            <>
              Writes meeting titles, summaries and action items.
              {availability && (
                <span className="models-warning selectable" role="status">
                  {availability}
                </span>
              )}
            </>
          }
        >
          <Select value={selectionKey} options={options} onChange={choose} disabled={!canEditConnections || testing} width={300} />
        </Row>

        {selection.kind === "cli" && (
          <CliSettings
            cli={selection.value}
            state={catalogs[selection.value]}
            installed={installed[selection.value]}
            disabled={!canEditConnections || testing}
            onRefresh={() => refreshCatalog(selection.value)}
          />
        )}

        <OllamaRow state={ollama} onRefresh={refreshOllama} />

        <Row
          icon={<Tile icon={Sparkles} color="#0a84ff" size={32} />}
          title="Notes API connections"
          detail="Optional. OpenAI-compatible chat completions only, from a provider or a local server. Uses your key and provider billing."
        >
          <Button icon={Plus} onClick={addConnection} disabled={!canEditConnections}>
            Add notes API connection…
          </Button>
        </Row>
        {settings.notesConnections.map((c) => (
          <Row
            key={c.id}
            title={
              <span className="models-name">
                {c.name}
                {selection.kind === "connection" && selection.value === c.id && <Badge tone="success">Active</Badge>}
              </span>
            }
            detail={
              <>
                {c.modelID}
                <span className="mono selectable models-line">{c.endpoint}</span>
              </>
            }
          >
            <Button onClick={() => setEditing(c)} disabled={!canEditConnections}>
              Edit…
            </Button>
            <Button variant="danger" onClick={() => setRemoving(c)} disabled={!canEditConnections}>
              Remove
            </Button>
          </Row>
        ))}

        <div className="row models-row">
          <Tile icon={FlaskConical} color="#14b8a6" size={32} />
          <div className="models-info">
            <div>Test notes model</div>
            <span className="faint">Test sends only a short built-in sample to the selected notes model.</span>
            {testing && (
              <span className="models-inline faint" role="status">
                <Spinner label="Testing" />
                Waiting for {selectionName(settings)}… CLIs and large local models can take a minute.
              </span>
            )}
            {result && (
              <p className={`selectable ${result.ok ? "models-result" : "models-error"}`} role={result.ok ? "status" : "alert"}>
                {result.text}
              </p>
            )}
          </div>
          <div className="row-control">
            <Button onClick={test} disabled={testing || !canEditConnections || availability !== null}>
              {testing ? "Testing…" : "Test"}
            </Button>
          </div>
        </div>
      </Section>

      {editing && <ConnectionEditor initial={editing} forNotes onClose={() => setEditing(null)} />}
      {removing && (
        <Confirm
          title={`Remove ${removing.name}?`}
          confirmLabel="Remove"
          onClose={() => setRemoving(null)}
          onConfirm={() => api.deleteConnection(removing.id, true)}
        >
          The connection and its API key are deleted from this PC and Windows Credential Manager. If it writes your notes now, choose another notes
          model afterwards.
        </Confirm>
      )}
    </>
  );
}

const openOllama = () => {
  openUrl("https://ollama.com").catch(() => {});
};

function OllamaRow({ state, onRefresh }: { state: { models: string[]; error: string | null; loading: boolean }; onRefresh: () => void }) {
  let detail;
  if (state.error) {
    detail = (
      <>
        <span className="models-warning selectable">{state.error}</span>
        <span className="models-line">
          Ollama is free and runs models on this PC.{" "}
          <a role="link" tabIndex={0} onClick={openOllama} onKeyDown={(e) => e.key === "Enter" && openOllama()}>
            Get Ollama at ollama.com <ExternalLink size={12} style={{ verticalAlign: -1 }} />
          </a>
        </span>
      </>
    );
  } else if (state.loading && state.models.length === 0) {
    detail = "Looking for Ollama on this PC…";
  } else if (state.models.length === 0) {
    detail = "Ollama is running but has no text models yet. Download one from a terminal, for example ollama pull llama3.2, then refresh.";
  } else {
    detail = `Free and private. ${state.models.length} local ${state.models.length === 1 ? "model" : "models"} found; choose one under Notes model.`;
  }
  return (
    <Row icon={<Tile icon={NotebookPen} color="#71717a" size={32} />} title="Ollama · On this PC" detail={detail}>
      {state.loading && <Spinner label="Looking for Ollama models" />}
      <Button icon={RefreshCw} onClick={onRefresh} disabled={state.loading}>
        Refresh local models
      </Button>
    </Row>
  );
}

function CliSettings({
  cli,
  state,
  installed,
  disabled,
  onRefresh,
}: {
  cli: NotesCli;
  state: CatalogState;
  installed: boolean | undefined;
  disabled: boolean;
  onRefresh: () => void;
}) {
  const { workspace, saveSettings } = useStore();
  const key = CLI_SETTING[cli];
  const saved = workspace.settings[key];
  const [draft, setDraft] = useState(saved);
  useEffect(() => setDraft(saved), [saved]);
  const name = CLI_NAMES[cli];
  const models = state.catalog?.models ?? [];

  const setModel = (id: string) => {
    const value = id.trim();
    if (value === saved) return;
    saveSettings((s) => ({ ...s, [key]: value }));
  };

  const options: [string, string][] = [];
  if (!saved) options.push(["", "Choose a model"]);
  for (const m of models) options.push([m.id, `${m.name} · ${m.id}`]);
  if (saved && !models.some((m) => m.id === saved)) options.push([saved, saved]);
  const chosen = models.find((m) => m.id === saved);

  return (
    <div className="row models-row">
      <Tile icon={Terminal} color={cli === "claudeCode" ? "#d97706" : "#10a37f"} size={32} />
      <div className="models-info models-cli">
        <div className="models-cli-fields">
          <Field label={`${name} model`}>
            <Select value={saved} options={options} onChange={setModel} disabled={disabled} />
          </Field>
          <Field label="Model ID">
            <input
              className="input"
              value={draft}
              maxLength={200}
              spellCheck={false}
              placeholder="Exact model ID"
              disabled={disabled}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={() => setModel(draft)}
              onKeyDown={(e) => e.key === "Enter" && setModel(draft)}
            />
          </Field>
        </div>
        <div className="hstack">
          <Button size="small" icon={RefreshCw} onClick={onRefresh} disabled={state.loading}>
            Refresh {name} models
          </Button>
          {state.loading && <Spinner label={`Loading ${name} models`} />}
        </div>
        {installed === false && (
          <span className="models-warning">
            {name} wasn't found on this PC. Install the latest {name} CLI, then sign in from a terminal with {CLI_LOGIN[cli]}.
          </span>
        )}
        {state.error ? <span className="faint selectable">{state.error}</span> : chosen?.detail ? <span className="faint">{chosen.detail}</span> : null}
        <span className="faint">
          Install the latest {name} CLI and run <code className="mono">{CLI_LOGIN[cli]}</code> in a terminal. OpenDictate uses that sign-in and your
          existing {cli === "claudeCode" ? "Claude" : "ChatGPT"} subscription; it never uses or stores API keys. Subscription limits apply. Type any
          exact model ID the CLI accepts if yours isn't listed.
        </span>
      </div>
    </div>
  );
}
