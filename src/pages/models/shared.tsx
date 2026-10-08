// Helpers shared by the Models page sections.
import { useEffect, useState, type ReactNode } from "react";
import { Button, Modal } from "../../components/ui";
import type { NotesCli, SpeechApi } from "../../lib/api";
import { useStore } from "../../lib/store";

export const PROVIDER_NAMES: Record<SpeechApi, string> = {
  sarvam: "Sarvam AI",
  smallest: "Smallest AI",
  openAiCompatible: "OpenAI-compatible",
};

export const CLI_NAMES: Record<NotesCli, string> = { claudeCode: "Claude Code", codex: "Codex" };
export const CLI_LOGIN: Record<NotesCli, string> = { claudeCode: "claude auth login", codex: "codex login" };
export const CLI_SETTING: Record<NotesCli, "claudeNotesModel" | "codexNotesModel"> = {
  claudeCode: "claudeNotesModel",
  codex: "codexNotesModel",
};

/** Mirrors BetterWispr's `isBusy` and `canEditConnections`. */
export function useBusy() {
  const { session, meetings } = useStore();
  const isBusy = session.phase.kind === "preparing" || session.phase.kind === "recording" || session.phase.kind === "transcribing";
  return { isBusy, canEditConnections: !isBusy && meetings.activity.kind === "idle" };
}

/** Closes a dialog with Esc. */
export function useEscape(onClose: () => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
}

export function Spinner({ label }: { label: string }) {
  return <span className="spinner models-spinner" role="status" aria-label={label} />;
}

/** A confirmation dialog for destructive actions. Shows the error inline if the action fails. */
export function Confirm({
  title,
  children,
  confirmLabel,
  onConfirm,
  onClose,
}: {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  onConfirm: () => Promise<void>;
  onClose: () => void;
}) {
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEscape(onClose);

  const confirm = async () => {
    setWorking(true);
    setError(null);
    try {
      await onConfirm();
      onClose();
    } catch (e) {
      setError(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
      setWorking(false);
    }
  };

  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={confirm} disabled={working}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      <div className="muted">{children}</div>
      {error && (
        <p className="models-error selectable" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
