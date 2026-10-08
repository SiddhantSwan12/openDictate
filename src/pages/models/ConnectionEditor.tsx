// Port of BetterWispr's SpeechConnectionEditor: adds or edits a speech or notes API connection.
import { useCallback, useEffect, useState, type KeyboardEvent } from "react";
import { Badge, Banner, Button, Field, Modal, Select } from "../../components/ui";
import { api, errorText, LANGUAGES, type SpeechApi, type SpeechConnection } from "../../lib/api";
import { useStore } from "../../lib/store";
import { PROVIDER_NAMES, useBusy, useEscape } from "./shared";

/** Languages Smallest AI accepts (SpeechConnection.validateLanguage). */
const SMALLEST_LANGUAGES = [
  "en", "hi", "zh", "ko", "ja", "yue", "ms", "id", "tl", "it", "es", "pt", "de", "fr", "uk", "ru", "pl", "cs", "sk", "nl", "lv",
  "et", "ro", "fi", "sv", "bg", "hu", "da", "lt", "mt", "kn", "ml", "mr", "gu", "te", "or", "bn", "pa", "ta",
];

const MODEL_HINTS: Record<SpeechApi, string> = {
  sarvam: "Default: saaras:v4. Long recordings are sent in 25-second chunks.",
  smallest: "Default: pulse. Use pulse for multilingual speech or pulse-pro for English.",
  openAiCompatible: "Any model your server supports, for example whisper-1.",
};

export function ConnectionEditor({ initial, forNotes, onClose }: { initial: SpeechConnection; forNotes: boolean; onClose: () => void }) {
  const { workspace, saveSettings } = useStore();
  const { canEditConnections } = useBusy();
  const [connection, setConnection] = useState(initial);
  const [key, setKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [language, setLanguage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const close = useCallback(() => onClose(), [onClose]);
  useEscape(close);

  const list = forNotes ? workspace.settings.notesConnections : workspace.settings.speechConnections;
  const original = list.find((c) => c.id === initial.id) ?? null;

  useEffect(() => {
    if (!original) return;
    api.connectionHasKey(original, forNotes).then(setHasKey, () => setHasKey(false));
    // Only the connection as it was when the editor opened matters.
  }, []);

  // Keys are bound to the exact provider and endpoint, so a changed URL never reuses the saved key.
  const sameDestination = original !== null && original.api === connection.api && original.endpoint === connection.endpoint.trim();
  const canKeepKey = sameDestination && hasKey;
  const openAi = connection.api === "openAiCompatible";
  const smallest = connection.api === "smallest";
  const settingsLanguage = workspace.settings.language;
  const spokenLanguage = language ?? (SMALLEST_LANGUAGES.includes(settingsLanguage) ? settingsLanguage : "en");

  const update = (change: Partial<SpeechConnection>) => setConnection((c) => ({ ...c, ...change }));

  const changeProvider = async (provider: SpeechApi) => {
    try {
      const fresh = await api.newConnection(provider);
      setConnection({ ...fresh, id: connection.id });
      setKey("");
      setError(null);
      setLanguage(null);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const save = async () => {
    if (saving || !canEditConnections) return;
    const next: SpeechConnection = {
      ...connection,
      name: connection.name.trim(),
      endpoint: connection.endpoint.trim(),
      modelID: connection.modelID.trim(),
    };
    if (smallest && next.modelID === "pulse-pro" && spokenLanguage !== "en") {
      setError("Pulse Pro requires English. Choose English or use the pulse model.");
      return;
    }
    const trimmedKey = key.trim();
    setSaving(true);
    setError(null);
    try {
      await api.saveConnection(next, trimmedKey === "" ? null : trimmedKey, forNotes);
      if (smallest && settingsLanguage !== spokenLanguage) {
        await saveSettings((s) => ({ ...s, language: spokenLanguage }));
      }
      onClose();
    } catch (e) {
      setError(errorText(e));
      setSaving(false);
    }
  };

  const onEnter = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") save();
  };

  const title = forNotes
    ? original
      ? "Edit notes API connection"
      : "Add notes API connection"
    : original
      ? "Edit speech connection"
      : "Add speech connection";

  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={save} disabled={saving || !canEditConnections}>
            {saving ? "Saving…" : "Save connection"}
          </Button>
        </>
      }
    >
      {forNotes ? (
        <div className="field">
          <span>API format</span>
          <div>OpenAI-compatible chat completions</div>
        </div>
      ) : (
        <Field label="Provider">
          <Select<SpeechApi>
            value={connection.api}
            options={(Object.keys(PROVIDER_NAMES) as SpeechApi[]).map((p) => [p, PROVIDER_NAMES[p]])}
            onChange={changeProvider}
          />
        </Field>
      )}

      <Field label="Name">
        <input className="input" value={connection.name} maxLength={100} onChange={(e) => update({ name: e.target.value })} onKeyDown={onEnter} />
      </Field>

      <Field label="Model ID" hint={forNotes ? "Any chat model your provider supports." : MODEL_HINTS[connection.api]}>
        <input
          className="input"
          value={connection.modelID}
          maxLength={200}
          spellCheck={false}
          onChange={(e) => update({ modelID: e.target.value })}
          onKeyDown={onEnter}
        />
      </Field>

      {openAi ? (
        <Field
          label={forNotes ? "Chat completions URL" : "Transcription URL"}
          hint={
            forNotes
              ? "Enter a full /chat/completions endpoint and any model ID your provider supports. Works with OpenAI-compatible services and local servers. API keys use Bearer authentication. HTTPS is required except for localhost."
              : "Enter the full endpoint, for example http://localhost:8000/v1/audio/transcriptions. It must accept a WAV file and return JSON with a text field. API keys use Bearer authentication. HTTPS is required except for localhost."
          }
        >
          <input
            className="input"
            value={connection.endpoint}
            spellCheck={false}
            onChange={(e) => update({ endpoint: e.target.value })}
            onKeyDown={onEnter}
          />
        </Field>
      ) : (
        <div className="field">
          <span>Endpoint</span>
          <div className="mono selectable">{connection.endpoint}</div>
        </div>
      )}

      <Field
        label="API key"
        hint={
          canKeepKey ? (
            <span className="models-inline">
              <Badge tone="success">Key saved</Badge>
              Leave blank to keep the saved key.
            </span>
          ) : openAi ? (
            "Leave blank if your server doesn't need a key."
          ) : undefined
        }
      >
        <input
          className="input"
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={key}
          placeholder={canKeepKey ? "New API key (blank keeps saved key)" : openAi ? "API key (optional)" : "API key"}
          onChange={(e) => setKey(e.target.value)}
          onKeyDown={onEnter}
        />
      </Field>

      {smallest && (
        <Field
          label="Spoken language"
          hint="Smallest AI can't detect the language automatically. Saving sets Spoken language in Settings, which every model uses."
        >
          <Select
            value={spokenLanguage}
            options={LANGUAGES.filter(([code]) => SMALLEST_LANGUAGES.includes(code))}
            onChange={setLanguage}
          />
        </Field>
      )}

      <p className="faint models-note">
        {forNotes
          ? "Saving makes no network request. Select this connection under Notes model to send meeting text to it. Keys stay in Windows Credential Manager. Changing the endpoint of the connection in use clears the notes model choice; select the connection again to use the new URL."
          : "Saving makes no network request. Choose Use in Models to send future dictation and meeting audio to this endpoint; its pricing and retention policies then apply. Keys are stored only in Windows Credential Manager. OpenDictate never falls back to a cloud provider on its own."}
      </p>

      {!canEditConnections && <Banner tone="warning">Finish the current dictation or meeting before changing connections.</Banner>}
      {error && <Banner tone="error">{error}</Banner>}
    </Modal>
  );
}
