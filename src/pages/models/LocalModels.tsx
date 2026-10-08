// Built-in models that run on this PC.
import { Gauge } from "lucide-react";
import { useState } from "react";
import { Row, Section, Tile, Toggle } from "../../components/ui";
import { api, type ModelView } from "../../lib/api";
import { useStore } from "../../lib/store";
import { ModelRow } from "./ModelRow";
import { Confirm } from "./shared";

export function LocalModels() {
  const { workspace, saveSettings } = useStore();
  const [removing, setRemoving] = useState<ModelView | null>(null);
  const models = workspace.models.filter((m) => m.engine !== "api");
  const { useGpu } = workspace.settings;

  return (
    <>
      <Section
        title="On this PC"
        footer="These models process audio on this PC. Parakeet and Whisper need an internet connection only for the download. After that they're free, unlimited and work offline."
      >
        {models.map((model) => (
          <ModelRow
            key={model.id}
            model={model}
            onRemove={model.installed && model.id !== workspace.settings.selectedModelID ? () => setRemoving(model) : undefined}
          />
        ))}
        <Row
          icon={<Tile icon={Gauge} color="#14b8a6" size={32} />}
          title="Use the graphics card (GPU)"
          detail={
            workspace.gpuName
              ? `Transcribes faster on ${workspace.gpuName}. Parakeet uses DirectML on any graphics card; Whisper uses CUDA on NVIDIA graphics cards. Turn off to use only the processor (CPU).`
              : "No compatible graphics card was detected, so models use the processor (CPU). Parakeet uses DirectML on any graphics card; Whisper uses CUDA on NVIDIA graphics cards."
          }
        >
          <Toggle on={useGpu} label="Use the graphics card (GPU)" onChange={(on) => saveSettings((s) => ({ ...s, useGpu: on }))} />
        </Row>
      </Section>

      {removing && (
        <Confirm
          title={`Remove ${removing.name}?`}
          confirmLabel="Remove"
          onClose={() => setRemoving(null)}
          onConfirm={() => api.uninstallModel(removing.id)}
        >
          This deletes the downloaded model from this PC and frees about {removing.sizeLabel.replace(/^~/, "")} of disk space. You can download
          it again at any time.
        </Confirm>
      )}
    </>
  );
}
