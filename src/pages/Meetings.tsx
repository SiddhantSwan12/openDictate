// Notetaker: port of BetterWispr's MeetingsView (list of notes, current note, settings) on Windows.
import { AlertTriangle, CircleDot, Copy, Download, FileText, MoreHorizontal, Search, Settings2, Sparkles, Square, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, durationLabel, errorText, type AudioInputDevice, type Meeting, type NotesCli, type NotesSelection } from "../lib/api";
import { useStore } from "../lib/store";
import { Button, Modal } from "../components/ui";
import { MeetingDetail } from "./meetings/MeetingDetail";
import {
  CLI_NAMES,
  Menu,
  Popover,
  capturingId,
  cliModel,
  displayTitle,
  groupByDay,
  hasContent,
  isActive,
  matches,
  noteDay,
  snippet,
  timeOfDay,
  useMeetingActions,
  useMenu,
  useNotesAvailability,
} from "./meetings/shared";
import "./meetings/Meetings.css";

export function Meetings() {
  const { meetings, openMeetingId, run } = useStore();
  const [toDelete, setToDelete] = useState<string | null>(null);
  const actions = useMeetingActions();
  const open = openMeetingId ? meetings.meetings.find((m) => m.id === openMeetingId) : undefined;

  return (
    <div className="mt-page">
      {meetings.message && (
        <div className="mt-message" role="status">
          <AlertTriangle size={16} className="mt-message-icon" aria-hidden="true" />
          <span className="selectable" style={{ flex: 1 }}>
            {meetings.message}
          </span>
          <Button variant="ghost" size="icon" icon={X} title="Dismiss meeting message" onClick={() => run(() => api.meetingDismissMessage())} />
        </div>
      )}
      {open ? (
        <MeetingDetail key={open.id} meeting={open} onDelete={setToDelete} />
      ) : (
        <div className="mt-home">
          <NotesHome onDelete={setToDelete} />
          <CurrentNote />
        </div>
      )}
      {toDelete && (
        <Modal
          title="Delete this meeting?"
          onClose={() => setToDelete(null)}
          footer={
            <>
              <Button onClick={() => setToDelete(null)}>Cancel</Button>
              <Button
                variant="primary"
                onClick={() => {
                  const id = toDelete;
                  setToDelete(null);
                  actions.remove(id);
                }}
              >
                Delete
              </Button>
            </>
          }
        >
          <p className="muted" style={{ margin: 0 }}>
            This removes the meeting, its notes and transcript from this PC. It can't be undone.
          </p>
        </Modal>
      )}
    </div>
  );
}

/** Starts a meeting and opens its note; sends you to Models if the speech model isn't downloaded. */
export function useStartMeeting() {
  const { setError, setPage, setOpenMeetingId } = useStore();
  return async () => {
    try {
      const id = await api.meetingStart();
      setError(null);
      setPage("meetings");
      setOpenMeetingId(id);
    } catch (e) {
      const text = errorText(e);
      setError(text);
      if (text.includes("in Models")) setPage("models");
    }
  };
}

function NotesHome({ onDelete }: { onDelete: (id: string) => void }) {
  const { meetings, session, run } = useStore();
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const start = useStartMeeting();
  const filtered = useMemo(
    () => (query ? meetings.meetings.filter((m) => matches(m, query)) : meetings.meetings),
    [meetings.meetings, query],
  );
  const availability = useNotesAvailability();
  const capturing = capturingId(meetings.activity) !== null;
  const dictating = ["preparing", "recording", "transcribing"].includes(session.phase.kind);
  const sections = groupByDay(filtered);

  return (
    <div className="mt-notes">
      <div className="mt-notes-header">
        <h1>Notetaker</h1>
        <div className="spacer" />
        <Button
          variant="ghost"
          size="icon"
          icon={Search}
          title="Search notes"
          onClick={() => {
            setSearching(!searching);
            setQuery("");
          }}
        />
        <NotetakerSettings />
        <Button
          size="large"
          icon={capturing ? Square : CircleDot}
          disabled={!capturing && (meetings.activity.kind !== "idle" || dictating)}
          onClick={() => (capturing ? run(() => api.meetingStop()) : start())}
        >
          {capturing ? "Stop Notetaker" : "Start Notetaker"}
        </Button>
      </div>
      {searching && (
        <input
          className="input"
          autoFocus
          placeholder="Search notes"
          aria-label="Search meeting notes"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setSearching(false);
              setQuery("");
            }
          }}
        />
      )}
      <NotesModelNotice availability={availability} />
      {filtered.length === 0 ? (
        <div className="mt-notes-empty">
          <div className="mt-serif" style={{ fontSize: 26, lineHeight: "34px" }}>
            {query ? "No matching notes" : "Be in the conversation."}
          </div>
          <div className="muted">{query ? "Try another title or phrase." : "Keep your thoughts, follow the transcript, and leave with a summary."}</div>
          {!query && (
            <div className="faint" style={{ marginTop: 8 }}>
              Your microphone is “Me”. Everything other apps play, like the call in Teams, Zoom or your browser, is “Them”.
            </div>
          )}
        </div>
      ) : (
        sections.map((section) => (
          <section key={section.title} className="mt-day" aria-label={section.title}>
            <h2 className="mt-day-title">{section.title}</h2>
            {section.meetings.map((meeting) => (
              <NoteRow key={meeting.id} meeting={meeting} availability={availability} onDelete={onDelete} />
            ))}
          </section>
        ))
      )}
    </div>
  );
}

/** Points to Models when no notes model is ready. Meetings are still recorded and transcribed. */
function NotesModelNotice({ availability }: { availability: string | null }) {
  const { setPage } = useStore();
  if (!availability) return null;
  return (
    <div className="mt-notice">
      <Sparkles size={16} aria-hidden="true" style={{ flexShrink: 0, marginTop: 2 }} />
      <div style={{ flex: 1 }}>
        <div>Summaries need a notes model.</div>
        <div className="faint">{availability}</div>
      </div>
      <Button size="small" onClick={() => setPage("models")}>
        Open Models
      </Button>
    </div>
  );
}

function NoteRow({ meeting, availability, onDelete }: { meeting: Meeting; availability: string | null; onDelete: (id: string) => void }) {
  const { meetings, setOpenMeetingId } = useStore();
  const actions = useMeetingActions();
  const menu = useMenu();
  const { activity } = meetings;
  const mine = activity.kind !== "idle" && activity.id === meeting.id;
  const status = !mine
    ? null
    : { starting: "Starting", recording: "Recording", finishing: "Transcribing", generating: "Writing summary" }[activity.kind];
  const canSummarize = !meeting.summary && hasContent(meeting) && activity.kind === "idle" && !availability;
  const openNote = () => setOpenMeetingId(meeting.id);

  return (
    <div
      className="mt-row"
      role="button"
      tabIndex={0}
      aria-label={`${displayTitle(meeting)}, ${timeOfDay(meeting.createdAt)}${status ? `, ${status}` : ""}`}
      onClick={openNote}
      onKeyDown={(e) => {
        if (e.target !== e.currentTarget) return;
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          openNote();
        } else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
          e.preventDefault();
          const r = e.currentTarget.getBoundingClientRect();
          menu.openAt(r.left + 48, r.bottom);
        }
      }}
      onContextMenu={menu.fromContext}
    >
      <span className="mt-row-icon" aria-hidden="true">
        <FileText size={16} strokeWidth={1.75} />
      </span>
      <div className="mt-row-text">
        <div className="mt-row-title">{displayTitle(meeting)}</div>
        <div className="mt-row-detail muted">
          <span>{timeOfDay(meeting.createdAt)}</span>
          {status ? (
            <>
              <span aria-hidden="true">•</span>
              <span className={capturingId(activity) === meeting.id ? "mt-live" : undefined}>{status}</span>
            </>
          ) : (
            canSummarize && (
              <>
                <span aria-hidden="true">•</span>
                <button
                  type="button"
                  className="mt-link"
                  onClick={(e) => {
                    e.stopPropagation();
                    actions.generate(meeting.id);
                  }}
                >
                  Generate summary
                </button>
              </>
            )
          )}
        </div>
      </div>
      <button
        type="button"
        className="btn ghost icon mt-row-more"
        title="More meeting actions"
        aria-label="More meeting actions"
        aria-haspopup="menu"
        onClick={(e) => {
          e.stopPropagation();
          menu.fromButton(e);
        }}
      >
        <MoreHorizontal size={16} strokeWidth={1.75} />
      </button>
      {menu.at && (
        <span onClick={(e) => e.stopPropagation()}>
          <Menu
            x={menu.at.x}
            y={menu.at.y}
            label="Meeting actions"
            onClose={menu.close}
            items={[
              { label: "Open", icon: FileText, onSelect: openNote },
              { label: "Copy notes and transcript", icon: Copy, onSelect: () => actions.copy(meeting.id) },
              { label: "Export as Markdown…", icon: Download, onSelect: () => actions.exportMarkdown(meeting) },
              "divider",
              { label: "Delete meeting…", icon: Trash2, danger: true, disabled: isActive(meetings, meeting.id), onSelect: () => onDelete(meeting.id) },
            ]}
          />
        </span>
      )}
    </div>
  );
}

/** The meeting in progress, or the latest one, shown beside the list of notes. */
function CurrentNote() {
  const { meetings, setOpenMeetingId } = useStore();
  const id = (meetings.activity.kind !== "idle" ? meetings.activity.id : null) ?? meetings.meetings[0]?.id;
  const meeting = id ? meetings.meetings.find((m) => m.id === id) : undefined;

  return (
    <aside className="mt-current" aria-label="Current note">
      {meeting ? (
        <>
          <button type="button" className="mt-current-title" title="Open note" onClick={() => setOpenMeetingId(meeting.id)}>
            {displayTitle(meeting)}
          </button>
          <div className="muted">
            {noteDay(meeting.createdAt)} • {timeOfDay(meeting.createdAt)}
          </div>
          {capturingId(meetings.activity) === meeting.id ? (
            <>
              <div className="hstack" style={{ marginTop: 6 }}>
                <span className="mt-dot me" aria-hidden="true" />
                <span className="mt-digits">
                  {meetings.activity.kind === "starting" ? "Starting" : "Recording"} · {durationLabel(meetings.elapsed)}
                </span>
              </div>
              <a
                role="button"
                tabIndex={0}
                onClick={() => setOpenMeetingId(meeting.id)}
                onKeyDown={(e) => e.key === "Enter" && setOpenMeetingId(meeting.id)}
              >
                Open the live transcript
              </a>
              <div className="faint">The notetaker pill above the taskbar shows it's listening. Stop it there or here.</div>
            </>
          ) : (
            snippet(meeting) && <p className="mt-current-snippet muted">{snippet(meeting)}</p>
          )}
        </>
      ) : (
        <div className="muted">Your current note appears here.</div>
      )}
    </aside>
  );
}

// MARK: Settings

function encodeNotes(selection: NotesSelection): string {
  return selection.kind === "none" ? "none" : `${selection.kind}:${selection.value}`;
}

function decodeNotes(value: string): NotesSelection {
  const i = value.indexOf(":");
  const kind = i < 0 ? value : value.slice(0, i);
  const rest = value.slice(i + 1);
  if (kind === "ollama") return { kind, value: rest };
  if (kind === "connection") return { kind, value: rest };
  if (kind === "cli") return { kind, value: rest as NotesCli };
  return { kind: "none" };
}

/** Transcription model, microphone and notes model, like the gear menu in BetterWispr. */
function NotetakerSettings() {
  const { workspace, meetings, session, saveSettings, setPage, run } = useStore();
  const [open, setOpen] = useState(false);
  const [ollama, setOllama] = useState<string[]>([]);
  const [catalog, setCatalog] = useState<{ id: string; name: string }[]>([]);
  const settings = workspace.settings;
  const idle = meetings.activity.kind === "idle";
  const dictating = ["preparing", "recording", "transcribing"].includes(session.phase.kind);
  const selection = settings.notesSelection;
  const cli = selection.kind === "cli" ? selection.value : null;

  useEffect(() => {
    if (!open) return;
    api.ollamaModels().then(setOllama, () => setOllama([]));
  }, [open]);

  useEffect(() => {
    if (!open || !cli) return setCatalog([]);
    api.cliCatalog(cli).then(
      (c) => setCatalog(c.models),
      () => setCatalog([]),
    );
  }, [open, cli]);

  const speechModels = workspace.models;
  const selectedSpeech = speechModels.find((m) => m.id === settings.selectedModelID);

  const microphoneChoices: AudioInputDevice[] = [...workspace.microphones];
  if (settings.microphone && !microphoneChoices.some((m) => m.id === settings.microphone!.id)) microphoneChoices.push(settings.microphone);

  const ollamaChoices = selection.kind === "ollama" && !ollama.includes(selection.value) ? [...ollama, selection.value] : ollama;
  const missingConnection = selection.kind === "connection" && !settings.notesConnections.some((c) => c.id === selection.value);

  const chooseNotes = async (value: string) => {
    const next = decodeNotes(value);
    let model: string | null = null;
    if (next.kind === "cli" && !cliModel(settings, next.value)) {
      model = await api.cliCatalog(next.value).then((c) => c.defaultId, () => null);
    }
    saveSettings((s) => ({
      ...s,
      notesSelection: next,
      ...(model && next.kind === "cli" ? (next.value === "claudeCode" ? { claudeNotesModel: model } : { codexNotesModel: model }) : {}),
    }));
  };

  const goToModels = () => {
    setOpen(false);
    setPage("models");
  };

  return (
    <Popover
      open={open}
      onClose={() => setOpen(false)}
      label="Notetaker settings"
      trigger={<Button variant="ghost" size="icon" icon={Settings2} title="Notetaker settings" onClick={() => setOpen(!open)} />}
    >
      <div className="mt-settings">
        <label className="field">
          <span>Transcription model</span>
          <select
            className="select"
            value={settings.selectedModelID}
            disabled={!idle || dictating}
            onChange={(e) => run(() => api.selectModel(e.target.value))}
          >
            {speechModels.map((m) => (
              <option key={m.id} value={m.id} disabled={!m.installed}>
                {m.name + (m.installed ? "" : " · Download in Models")}
              </option>
            ))}
          </select>
          <span className="faint">{selectedSpeech?.engine === "api" ? "Audio is sent to your selected endpoint." : "Transcribed on this PC."}</span>
        </label>
        <a role="button" tabIndex={0} onClick={goToModels} onKeyDown={(e) => e.key === "Enter" && goToModels()}>
          Manage models…
        </a>
        <div className="mt-settings-divider" />
        <label className="field">
          <span>Microphone</span>
          <select
            className="select"
            value={settings.microphone?.id ?? ""}
            onChange={(e) => {
              const device = microphoneChoices.find((m) => m.id === e.target.value) ?? null;
              saveSettings((s) => ({ ...s, microphone: device }));
            }}
          >
            <option value="">{workspace.defaultMicrophone ? `Automatic (${workspace.defaultMicrophone.name})` : "Automatic"}</option>
            {microphoneChoices.map((m) => (
              <option key={m.id} value={m.id}>
                {workspace.microphones.some((d) => d.id === m.id) ? m.name : `${m.name} (not connected)`}
              </option>
            ))}
          </select>
          <span className="faint">“Them” is whatever plays through your default speakers or headset.</span>
        </label>
        <div className="mt-settings-divider" />
        <label className="field">
          <span>Notes model</span>
          <select className="select" value={encodeNotes(selection)} disabled={!idle} onChange={(e) => chooseNotes(e.target.value)}>
            {selection.kind === "none" && (
              <option value="none" disabled>
                Choose a notes model
              </option>
            )}
            {ollamaChoices.map((name) => (
              <option key={name} value={`ollama:${name}`}>
                Ollama · {name} · Free on this PC
              </option>
            ))}
            {(Object.keys(CLI_NAMES) as NotesCli[]).map((c) => (
              <option key={c} value={`cli:${c}`}>
                {CLI_NAMES[c]} · Subscription
              </option>
            ))}
            {settings.notesConnections.map((c) => (
              <option key={c.id} value={`connection:${c.id}`}>
                {c.name} · {c.modelID}
              </option>
            ))}
            {missingConnection && <option value={encodeNotes(selection)}>Missing notes connection</option>}
          </select>
          {ollama.length === 0 && <span className="faint">Install Ollama and pull a model to write notes free on this PC.</span>}
        </label>
        {cli && (
          <label className="field">
            <span>{CLI_NAMES[cli]} model</span>
            <select
              className="select"
              value={cliModel(settings, cli)}
              disabled={!idle}
              onChange={(e) => {
                const model = e.target.value;
                saveSettings((s) => (cli === "claudeCode" ? { ...s, claudeNotesModel: model } : { ...s, codexNotesModel: model }));
              }}
            >
              <option value="" disabled>
                Choose a model
              </option>
              {catalog.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name} · {m.id}
                </option>
              ))}
              {cliModel(settings, cli) && !catalog.some((m) => m.id === cliModel(settings, cli)) && (
                <option value={cliModel(settings, cli)}>{cliModel(settings, cli)}</option>
              )}
            </select>
          </label>
        )}
        <a role="button" tabIndex={0} onClick={goToModels} onKeyDown={(e) => e.key === "Enter" && goToModels()}>
          Configure and test notes models…
        </a>
      </div>
    </Popover>
  );
}
