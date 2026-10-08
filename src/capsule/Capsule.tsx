// The floating capsule above the taskbar. Port of BetterWispr's CapsuleView.
import { AlertTriangle, Check, ClipboardCopy, Copy, Sparkles, Square, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, durationLabel, on, type MeetingsView, type Session, type Toast } from "../lib/api";

const BARS = 9;

function Waveform({ level }: { level: number }) {
  const [heights, setHeights] = useState<number[]>(Array(BARS).fill(3));
  useEffect(() => {
    // Shift the newest level in from the right, with a little variation per bar.
    setHeights((prev) => [...prev.slice(1), 3 + Math.min(1, level) * 21 * (0.65 + Math.random() * 0.35)]);
  }, [level]);
  return (
    <div className="wave" aria-hidden="true">
      {heights.map((h, i) => (
        <span key={i} style={{ height: h }} />
      ))}
    </div>
  );
}

function ToastCard({ toast }: { toast: Toast }) {
  const Icon = { copy: ClipboardCopy, trash: Trash2, sparkles: Sparkles, warning: AlertTriangle }[toast.icon] ?? Check;
  return (
    <div className="capsule-card" role="status">
      <Icon size={18} style={{ marginTop: 1, flexShrink: 0, color: toast.icon === "warning" ? "#facc15" : "#fff" }} />
      <div>
        <div style={{ fontWeight: 600 }}>{toast.title}</div>
        <div className="muted" style={{ fontSize: 13 }}>
          {toast.message}
        </div>
      </div>
    </div>
  );
}

export function Capsule() {
  const [session, setSession] = useState<Session | null>(null);
  const [meetings, setMeetings] = useState<MeetingsView | null>(null);
  const [level, setLevel] = useState(0);
  const [duration, setDuration] = useState(0);
  const [toast, setToast] = useState<Toast | null>(null);
  const toastTimer = useRef<number>(undefined);

  useEffect(() => {
    const unlisten = [
      on<Session>("session", setSession),
      on<MeetingsView>("meetings", setMeetings),
      on<number>("level", setLevel),
      on<number>("recording-duration", setDuration),
      on<Toast>("toast", (t) => {
        setToast(t);
        window.clearTimeout(toastTimer.current);
        toastTimer.current = window.setTimeout(() => {
          setToast(null);
          api.capsuleIdle();
        }, 3200);
      }),
      on("idle", () => {
        if (!toastTimer.current) api.capsuleIdle();
      }),
    ];
    api.getSession().then(setSession);
    api.getMeetings().then(setMeetings);
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, []);

  useEffect(() => {
    if (!toast) toastTimer.current = undefined;
  }, [toast]);

  if (!session) return null;
  const phase = session.phase;
  const busy = phase.kind === "preparing" || phase.kind === "recording" || phase.kind === "transcribing";
  const meetingRecording = meetings?.activity.kind === "recording";
  const notetaking = !busy && (meetingRecording || meetings?.activity.kind === "starting");

  let card = null;
  if (phase.kind === "failed") {
    card = (
      <div className="capsule-card" role="alert">
        <AlertTriangle size={18} color="#facc15" style={{ marginTop: 1, flexShrink: 0 }} />
        <div style={{ flex: 1 }}>
          <div style={{ fontWeight: 600 }}>{phase.title}</div>
          <div className="muted" style={{ fontSize: 13 }}>
            {phase.message}
          </div>
        </div>
        <button className="btn ghost icon small" title="Dismiss" onClick={() => api.dismissCard()} style={{ color: "#fff" }}>
          <X size={14} />
        </button>
      </div>
    );
  } else if (phase.kind === "unpasted") {
    card = (
      <div className="capsule-card" role="status">
        <ClipboardCopy size={18} style={{ marginTop: 1, flexShrink: 0, opacity: 0.8 }} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontWeight: 600 }}>Copied, not pasted. Press Ctrl+V.</div>
          <div className="muted clamp-2" style={{ fontSize: 13 }}>
            {phase.text}
          </div>
        </div>
        <button
          className="btn ghost icon small"
          title="Copy text"
          style={{ color: "#fff" }}
          onClick={() => {
            api.copyText(phase.text);
            api.dismissCard();
          }}
        >
          <Copy size={14} />
        </button>
      </div>
    );
  }

  let pill = null;
  if (busy) {
    const recording = phase.kind === "recording";
    pill = (
      <div className="capsule" aria-live="polite">
        {phase.kind === "transcribing" ? (
          <>
            <span className="spinner" aria-hidden="true" />
            <span style={{ fontSize: 13 }}>{session.statusMessage.startsWith("Editing") ? "Editing…" : "Transcribing…"}</span>
          </>
        ) : phase.kind === "preparing" ? (
          <>
            <span className="spinner" aria-hidden="true" />
            <span style={{ fontSize: 13 }}>Starting…</span>
          </>
        ) : (
          <>
            <span className="pulse-dot" aria-hidden="true" />
            <Waveform level={level} />
            <span style={{ fontSize: 12, fontVariantNumeric: "tabular-nums", opacity: 0.8 }}>{durationLabel(duration)}</span>
          </>
        )}
        {recording && !session.held && (
          <button className="btn ghost icon" title="Finish dictation" onClick={() => api.finishRecording()}>
            <Square size={12} fill="currentColor" />
          </button>
        )}
        <button className="btn ghost icon" title="Cancel (Esc)" onClick={() => api.cancelRecording()}>
          <X size={14} />
        </button>
      </div>
    );
  } else if (notetaking && meetings) {
    pill = (
      <div className="capsule" style={{ boxShadow: "0 0 0 2px #10b981, 0 6px 20px rgba(0,0,0,.35)" }}>
        <span className="pulse-dot" style={{ background: "#10b981" }} aria-hidden="true" />
        <span style={{ fontSize: 13 }}>Notetaker {durationLabel(meetings.elapsed)}</span>
        <button className="btn ghost icon" title="Open meeting notes" onClick={() => api.showMain("meetings")}>
          <Sparkles size={14} />
        </button>
        <button className="btn ghost icon" title="Stop notetaker" onClick={() => api.meetingStop()}>
          <Square size={12} fill="currentColor" />
        </button>
      </div>
    );
  }

  return (
    <div className="capsule-root">
      {toast && !card && <ToastCard toast={toast} />}
      {card}
      {!card && pill}
    </div>
  );
}
