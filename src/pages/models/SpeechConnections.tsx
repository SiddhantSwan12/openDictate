// "Bring your own model": optional speech API connections.
import { Plus } from "lucide-react";
import { useState } from "react";
import { Badge, Button, Row, Section } from "../../components/ui";
import { api, type ModelView, type SpeechConnection } from "../../lib/api";
import { useStore } from "../../lib/store";
import { ConnectionEditor } from "./ConnectionEditor";
import { ModelRow } from "./ModelRow";
import { Confirm, PROVIDER_NAMES, useBusy } from "./shared";

export function SpeechConnections() {
  const { workspace, run } = useStore();
  const { canEditConnections } = useBusy();
  const [editing, setEditing] = useState<SpeechConnection | null>(null);
  const [removing, setRemoving] = useState<SpeechConnection | null>(null);
  const connections = workspace.settings.speechConnections;

  const add = () => run(async () => setEditing(await api.newConnection("sarvam")));

  const modelFor = (c: SpeechConnection): ModelView =>
    workspace.models.find((m) => m.id === `connection-${c.id}`) ?? {
      id: `connection-${c.id}`,
      name: c.name,
      detail: `${PROVIDER_NAMES[c.api]} · ${c.endpoint}`,
      sizeLabel: "Uses your endpoint",
      engine: "api",
      installed: true,
      languages: [],
    };

  return (
    <>
      <Section
        title="Bring your own model"
        action={<Badge>Optional</Badge>}
        footer="Choosing Use sends dictation and meeting audio to that endpoint. Provider charges and retention policies apply. API keys are kept in Windows Credential Manager; vocabulary replacements stay on this PC. Open-source models need a server with a compatible transcription API. Switch back to a built-in model to return to fully local transcription. OpenDictate never falls back to a cloud provider on its own."
      >
        <Row
          title="Use a speech API instead"
          detail="You don't need this. The built-in models above are free, unlimited and private. Add a connection only if you want to send audio to Sarvam AI, Smallest AI or an OpenAI-compatible server you choose."
        >
          <Button icon={Plus} onClick={add} disabled={!canEditConnections}>
            Add connection…
          </Button>
        </Row>
        {connections.map((c) => {
          const model = modelFor(c);
          return (
            <ModelRow
              key={c.id}
              model={model}
              meta={`${model.sizeLabel} · API connection · ${c.modelID}`}
              extra={
                <>
                  <Button size="small" onClick={() => setEditing(c)} disabled={!canEditConnections}>
                    Edit…
                  </Button>
                  <Button size="small" variant="danger" onClick={() => setRemoving(c)} disabled={!canEditConnections}>
                    Remove
                  </Button>
                </>
              }
            />
          );
        })}
      </Section>

      {editing && <ConnectionEditor initial={editing} forNotes={false} onClose={() => setEditing(null)} />}
      {removing && (
        <Confirm
          title={`Remove ${removing.name}?`}
          confirmLabel="Remove"
          onClose={() => setRemoving(null)}
          onConfirm={() => api.deleteConnection(removing.id, false)}
        >
          The connection and its API key are deleted from this PC and Windows Credential Manager. If it's in use, OpenDictate switches back to a
          built-in model.
        </Confirm>
      )}
    </>
  );
}
