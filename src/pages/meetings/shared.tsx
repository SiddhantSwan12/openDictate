// Meeting helpers ported from BetterWisprCore's Meeting.swift, plus small menus used by the Notetaker page.
import { save } from "@tauri-apps/plugin-dialog";
import type { LucideIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { api, errorText, type AppSettings, type Meeting, type MeetingActivity, type MeetingSegment, type MeetingsView, type NotesCli } from "../../lib/api";
import { useStore } from "../../lib/store";

export const CLI_NAMES: Record<NotesCli, string> = { claudeCode: "Claude Code", codex: "Codex" };

export function displayTitle(meeting: Meeting): string {
  return meeting.title.trim() || "New Meeting";
}

export function hasContent(meeting: Meeting): boolean {
  return meeting.segments.some((s) => s.text !== "") || meeting.notes.trim() !== "";
}

/** First non-empty of the summary overview, your thoughts, and the first transcript line, on one line. */
export function snippet(meeting: Meeting): string {
  for (const text of [meeting.summary?.overview, meeting.notes, meeting.segments[0]?.text]) {
    const line = (text ?? "").split(/\r?\n/).filter(Boolean).join(" ").trim();
    if (line) return line;
  }
  return "";
}

/** "02:15" from the segment start. */
export function timestamp(segment: MeetingSegment): string {
  const s = Math.max(0, Math.floor(segment.start));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

export function speakerLabel(segment: MeetingSegment): string {
  return segment.speaker === "me" ? "Me" : "Them";
}

function words(text: string): string[] {
  return text
    .toLowerCase()
    .split(/[^\p{L}\p{M}\p{N}]+/u)
    .filter(Boolean);
}

/**
 * Port of MeetingTranscript.removingEchoes: hides "Me" segments that exactly repeat a "Them"
 * segment of 8 or more words within 30 seconds (speaker audio picked up again by the microphone).
 */
export function removingEchoes(segments: MeetingSegment[]): MeetingSegment[] {
  const systemSpeech = new Map<string, number[]>();
  for (const segment of segments) {
    if (segment.speaker !== "them") continue;
    const w = words(segment.text);
    if (w.length >= 8) {
      const key = w.join(" ");
      systemSpeech.set(key, [...(systemSpeech.get(key) ?? []), segment.start]);
    }
  }
  return segments.filter((segment) => {
    if (segment.speaker !== "me") return true;
    const starts = systemSpeech.get(words(segment.text).join(" "));
    return !starts?.some((start) => Math.abs(start - segment.start) <= 30);
  });
}

/** Case- and accent-insensitive "contains", like localizedStandardContains. */
export function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

export function contains(text: string, query: string): boolean {
  return fold(text).includes(fold(query));
}

export function matches(meeting: Meeting, query: string): boolean {
  const s = meeting.summary;
  const summaryText = s ? [s.overview, ...s.keyPoints, ...s.decisions, ...s.actionItems.map((a) => a.text)] : [];
  return [meeting.title, meeting.notes, ...summaryText, ...meeting.segments.map((x) => x.text)].some((t) => contains(t, query));
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

function dayFormat(date: Date, now: Date, weekday: boolean): string {
  return date.toLocaleDateString(undefined, {
    weekday: weekday ? "short" : undefined,
    month: "short",
    day: "numeric",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}

/** "Today" for today's notes, otherwise a short day such as "Oct 6". */
export function noteDay(iso: string, now = new Date()): string {
  const date = new Date(iso);
  return startOfDay(date) === startOfDay(now) ? "Today" : dayFormat(date, now, false);
}

export function timeOfDay(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

export function longDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, { dateStyle: "long", timeStyle: "short" });
}

export interface MeetingSection {
  title: string;
  meetings: Meeting[];
}

/** Groups newest-first meetings by day, titled like "Today, Oct 8", "Yesterday, Oct 7" or "Tue, Oct 6". */
export function groupByDay(meetings: Meeting[], now = new Date()): MeetingSection[] {
  const today = startOfDay(now);
  const sections: MeetingSection[] = [];
  for (const meeting of meetings) {
    const date = new Date(meeting.createdAt);
    const days = Math.round((today - startOfDay(date)) / 86_400_000);
    const title =
      days <= 0 ? `Today, ${dayFormat(date, now, false)}` : days === 1 ? `Yesterday, ${dayFormat(date, now, false)}` : dayFormat(date, now, true);
    const last = sections[sections.length - 1];
    if (last?.title === title) last.meetings.push(meeting);
    else sections.push({ title, meetings: [meeting] });
  }
  return sections;
}

export function cliModel(settings: AppSettings, cli: NotesCli): string {
  return cli === "claudeCode" ? settings.claudeNotesModel : settings.codexNotesModel;
}

/** Mirrors AppSettings::notes_model_name in the backend. */
export function notesModelName(settings: AppSettings): string {
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
    case "cli":
      return `${CLI_NAMES[selection.value]} · ${cliModel(settings, selection.value) || "Choose a model"}`;
  }
}

export function activityId(activity: MeetingActivity): string | null {
  return activity.kind === "idle" ? null : activity.id;
}

/** The meeting being recorded (or starting), if any. */
export function capturingId(activity: MeetingActivity): string | null {
  return activity.kind === "starting" || activity.kind === "recording" ? activity.id : null;
}

/**
 * Why notes can't be written right now, or null when ready. The meetings view only refreshes on
 * meeting events, so this also asks the backend again whenever the notes settings change.
 */
export function useNotesAvailability(): string | null {
  const { meetings, workspace } = useStore();
  const [availability, setAvailability] = useState(meetings.notesAvailability);
  const s = workspace.settings;
  const key = JSON.stringify([s.notesSelection, s.claudeNotesModel, s.codexNotesModel, s.notesConnections.map((c) => c.id)]);
  useEffect(() => setAvailability(meetings.notesAvailability), [meetings.notesAvailability]);
  useEffect(() => {
    let live = true;
    api.notesAvailability().then(
      (value) => live && setAvailability(value),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [key]);
  return availability;
}

/** Actions shared by the list and the open note. */
export function useMeetingActions() {
  const { run, setError, openMeetingId, setOpenMeetingId } = useStore();
  return {
    copy: (id: string, transcriptOnly = false) => run(() => api.meetingCopy(id, transcriptOnly)),
    generate: (id: string) => run(() => api.meetingGenerateNotes(id)),
    async exportMarkdown(meeting: Meeting) {
      try {
        const name = displayTitle(meeting).replace(/[<>:"/\\|?*\u0000-\u001f]/g, " ").trim() || "Meeting";
        const path = await save({ defaultPath: `${name}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
        if (path) await api.meetingExport(meeting.id, path);
      } catch (e) {
        setError(errorText(e));
      }
    },
    async remove(id: string) {
      if ((await run(() => api.meetingDelete(id))) && openMeetingId === id) setOpenMeetingId(null);
    },
  };
}

export function isActive(view: MeetingsView, id: string): boolean {
  return activityId(view.activity) === id;
}

// MARK: Menus

export type MenuItem = { label: string; icon?: LucideIcon; onSelect: () => void; disabled?: boolean; danger?: boolean } | "divider";

/** Closes on Escape or a click outside `ref`. */
function useDismiss(ref: React.RefObject<HTMLElement | null>, onClose: () => void, open = true) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close.current();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close.current();
      }
    };
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", close.current);
    window.addEventListener("resize", close.current);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", close.current);
      window.removeEventListener("resize", close.current);
    };
  }, [ref, open]);
}

/** A Fluent-style context menu at a point on screen. */
export function Menu({ x, y, items, label, onClose }: { x: number; y: number; items: MenuItem[]; label: string; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });
  useDismiss(ref, onClose);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPosition({
      left: Math.max(8, Math.min(x, window.innerWidth - width - 8)),
      top: y + height > window.innerHeight - 8 ? Math.max(8, y - height) : y,
    });
    el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const buttons = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []);
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === "ArrowDown" ? index + 1 : index - 1;
    buttons[(next + buttons.length) % buttons.length]?.focus();
  };

  return (
    <div ref={ref} className="mt-menu" role="menu" aria-label={label} style={position} onKeyDown={onKeyDown}>
      {items.map((item, i) =>
        item === "divider" ? (
          <div key={i} className="mt-menu-divider" role="separator" />
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            className={item.danger ? "danger" : undefined}
            disabled={item.disabled}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            {item.icon ? <item.icon size={16} strokeWidth={1.75} /> : <span style={{ width: 16 }} />}
            {item.label}
          </button>
        ),
      )}
    </div>
  );
}

/** State for a menu opened from a button or a right-click. */
export function useMenu() {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return {
    at,
    close: () => setAt(null),
    openAt: (x: number, y: number) => setAt({ x, y }),
    fromButton: (e: React.MouseEvent<HTMLElement>) => {
      const r = e.currentTarget.getBoundingClientRect();
      setAt((open) => (open ? null : { x: r.right - 220, y: r.bottom + 4 }));
    },
    fromContext: (e: React.MouseEvent) => {
      e.preventDefault();
      setAt({ x: e.clientX, y: e.clientY });
    },
  };
}

/** A panel anchored under its trigger button. */
export function Popover({ open, onClose, trigger, children, label }: { open: boolean; onClose: () => void; trigger: ReactNode; children: ReactNode; label: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, onClose, open);
  return (
    <div ref={ref} className="mt-popover-anchor">
      {trigger}
      {open && (
        <div className="mt-popover" role="dialog" aria-label={label}>
          {children}
        </div>
      )}
    </div>
  );
}

/** Small spinner with a label, like a ProgressView next to text. */
export function Working({ children }: { children: ReactNode }) {
  return (
    <span className="mt-working">
      <span className="spinner" aria-hidden="true" />
      <span className="muted">{children}</span>
    </span>
  );
}
