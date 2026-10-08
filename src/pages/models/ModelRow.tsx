// One speech model: name, status, download progress and the action that fits its state.
import { AudioWaveform, CircleCheck, Globe } from "lucide-react";
import type { ReactNode } from "react";
import { Badge, Button, Progress, Tile } from "../../components/ui";
import { api, type EngineKind, type ModelView } from "../../lib/api";
import { useStore } from "../../lib/store";
import { Spinner, useBusy } from "./shared";

function engineLabel(engine: EngineKind): string {
  switch (engine) {
    case "parakeet":
      return "Parakeet · DirectML on the graphics card";
    case "whisper":
      return "Whisper · CUDA on NVIDIA graphics cards";
    case "api":
      return "API connection";
  }
}

function languagesLabel(model: ModelView): string | null {
  if (model.engine === "api") return null;
  if (model.languages.length === 0) return "Many languages";
  if (model.languages.length === 1 && model.languages[0] === "en") return "English only";
  return `${model.languages.length} languages`;
}

/** BetterWispr's `ModelInstallation.progressLabel`. */
export function progressLabel(progress: number): string {
  return progress < 0.9 ? `Downloading… ${Math.floor((progress / 0.9) * 100)}%` : "Setting up on this PC…";
}

export function ModelRow({
  model,
  meta,
  onRemove,
  extra,
}: {
  model: ModelView;
  /** Replaces the size and engine line. */
  meta?: string;
  /** Offered for installed models that aren't in use. */
  onRemove?: () => void;
  /** More actions shown below the description, such as Edit and Remove for connections. */
  extra?: ReactNode;
}) {
  const { workspace, session, run } = useStore();
  const { isBusy } = useBusy();
  const selected = model.id === workspace.settings.selectedModelID;
  const installation = session.installation?.id === model.id ? session.installation : null;
  const installing = installation !== null && installation.failure === null;
  const anyInstalling = session.installation !== null && session.installation.failure === null;
  const installed = model.installed;
  const languages = languagesLabel(model);
  const isApi = model.engine === "api";

  let action: ReactNode;
  if (installing) {
    action = <Button onClick={() => run(() => api.cancelInstallation())}>Cancel</Button>;
  } else if (selected && installed) {
    action = (
      <span className="models-check" role="img" aria-label="Active model" title="Active model">
        <CircleCheck size={22} strokeWidth={2} />
      </span>
    );
  } else if (installed) {
    action = (
      <>
        {onRemove && (
          <Button variant="danger" onClick={onRemove} disabled={isBusy}>
            Remove
          </Button>
        )}
        <Button onClick={() => run(() => api.selectModel(model.id))} disabled={isBusy}>
          Use
        </Button>
      </>
    );
  } else {
    action = (
      <Button variant={selected ? "primary" : undefined} onClick={() => run(() => api.installModel(model.id))} disabled={anyInstalling}>
        {installation ? "Retry" : "Download"}
      </Button>
    );
  }

  return (
    <div className="row models-row" role="group" aria-label={model.name}>
      <Tile icon={isApi ? Globe : AudioWaveform} color={isApi ? "#0a84ff" : "#a855f7"} size={32} />
      <div className="models-info">
        <div className="models-name">
          <span>{model.name}</span>
          {selected && <Badge tone="success">Active</Badge>}
          {!isApi && installed && !selected && <Badge>Downloaded</Badge>}
        </div>
        <div className="models-detail">{model.detail}</div>
        <div className="faint">{meta ?? [model.sizeLabel, engineLabel(model.engine), languages].filter(Boolean).join(" · ")}</div>
        {installation && (
          <div className="models-status">
            {installing && <Progress value={installation.progress} />}
            <span className={installation.failure ? "models-error selectable" : "faint"} role={installation.failure ? "alert" : undefined}>
              {installation.failure ? `Download stopped: ${installation.failure}` : progressLabel(installation.progress)}
            </span>
          </div>
        )}
        {selected && installed && !isApi && session.loadingModel && (
          <div className="models-inline faint">
            <Spinner label="Loading model" />
            Loading into memory…
          </div>
        )}
        {extra && <div className="hstack models-extra">{extra}</div>}
      </div>
      <div className="row-control">{action}</div>
    </div>
  );
}
