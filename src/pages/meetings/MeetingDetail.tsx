// One meeting, full page: port of BetterWispr's MeetingDetailView and the note bar from MeetingsView.
import {
  AudioLines,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Circle,
  Clock,
  Copy,
  Download,
  HardDrive,
  Mic,
  MicOff,
  MoreHorizontal,
  Search,
  Sparkles,
  Trash2,
  VolumeX,
  X,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, durationLabel, type ActionItem, type Meeting, type MeetingSegment } from "../../lib/api";
import { useMeetingLevels, useStore } from "../../lib/store";
import { Button, Toggle } from "../../components/ui";
import {
  Menu,
  Working,
  capturingId,
  contains,
  hasContent,
  isActive as isActiveIn,
  longDate,
  noteDay,
  notesModelName,
  removingEchoes,
  speakerLabel,
  timestamp,
  useMeetingActions,
  useMenu,
  useNotesAvailability,
} from "./shared";

type Tab = "thoughts" | "transcript" | "summary";

const CALL_AUDIO_HINT =
  "OpenDictate hasn't heard the other side of the call yet. Make sure the call plays through your default speakers or headset in Windows Sound settings.";
const NOTHING_TO_SUMMARIZE = "There's nothing to summarize yet. Speak or type a few notes first.";

/**
 * Keeps a local draft while typing and saves it after a pause (600 ms), ignoring older values the
 * backend echoes back until it has caught up with what was sent.
 */
function useDraft(value: string, save: (value: string) => void): [string, (value: string) => void] {
  const [draft, setDraft] = useState(value);
  const timer = useRef<number | null>(null);
  const sent = useRef<{ value: string; at: number } | null>(null);
  const latest = useRef(value);
  const saveRef = useRef(save);
  saveRef.current = save;

  useEffect(() => {
    if (timer.current !== null) return;
    if (sent.current) {
      if (value !== sent.current.value && Date.now() - sent.current.at < 3000) return;
      sent.current = null;
    }
    latest.current = value;
    setDraft(value);
  }, [value]);

  useEffect(
    () => () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        saveRef.current(latest.current);
      }
    },
    [],
  );

  const change = (next: string) => {
    setDraft(next);
    latest.current = next;
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      sent.current = { value: next, at: Date.now() };
      saveRef.current(next);
    }, 600);
  };

  return [draft, change];
}

export function MeetingDetail({ meeting, onDelete }: { meeting: Meeting; onDelete: (id: string) => void }) {
  const { meetings, workspace, run, setOpenMeetingId, setPage } = useStore();
  const availability = useNotesAvailability();
  const actions = useMeetingActions();
  const menu = useMenu();
  const id = meeting.id;
  const { activity } = meetings;
  const isRecording = capturingId(activity) === id;
  const isActive = isActiveIn(meetings, id);
  const needsUpdate = meetings.summariesNeedingUpdate.includes(id);
  const notesModel = notesModelName(workspace.settings);
  const canGenerate = activity.kind === "idle" && !availability;

  const [tab, setTab] = useState<Tab>(() => (isActive || !meeting.summary ? "transcript" : "summary"));
  const [title, setTitle] = useDraft(meeting.title, (value) => run(() => api.meetingUpdate(id, value, null)));
  const [notes, setNotes] = useDraft(meeting.notes, (value) => run(() => api.meetingUpdate(id, null, value)));

  // A new summary switches to the Summary tab.
  const generatedAt = meeting.summary?.generatedAt;
  const firstGeneratedAt = useRef(generatedAt);
  useEffect(() => {
    if (generatedAt && generatedAt !== firstGeneratedAt.current) setTab("summary");
    firstGeneratedAt.current = generatedAt;
  }, [generatedAt]);

  const index = meetings.meetings.findIndex((m) => m.id === id);
  const newer = index > 0 ? meetings.meetings[index - 1] : undefined;
  const older = index >= 0 && index + 1 < meetings.meetings.length ? meetings.meetings[index + 1] : undefined;

  return (
    <div className="mt-detail">
      <div className="mt-notebar">
        <Button size="icon" icon={ChevronLeft} title="All notes" onClick={() => setOpenMeetingId(null)} />
        <div className="spacer" />
        <button
          type="button"
          className="btn ghost icon"
          title="More meeting actions"
          aria-label="More meeting actions"
          aria-haspopup="menu"
          onClick={menu.fromButton}
        >
          <MoreHorizontal size={16} strokeWidth={1.75} />
        </button>
        <button type="button" className="btn ghost" title="Copy notes and transcript" onClick={() => actions.copy(id)}>
          <Copy size={16} strokeWidth={1.75} />
          Copy
        </button>
        <Button variant="ghost" size="icon" icon={ChevronLeft} title="Newer note" disabled={!newer} onClick={() => newer && setOpenMeetingId(newer.id)} />
        <Button variant="ghost" size="icon" icon={ChevronRight} title="Older note" disabled={!older} onClick={() => older && setOpenMeetingId(older.id)} />
        {menu.at && (
          <Menu
            x={menu.at.x}
            y={menu.at.y}
            label="More meeting actions"
            onClose={menu.close}
            items={[
              {
                label: meeting.summary ? "Regenerate summary" : "Generate summary",
                icon: Sparkles,
                disabled: !hasContent(meeting) || !canGenerate,
                onSelect: () => actions.generate(id),
              },
              { label: "Copy transcript", icon: AudioLines, disabled: meeting.segments.length === 0, onSelect: () => actions.copy(id, true) },
              { label: "Export as Markdown…", icon: Download, onSelect: () => actions.exportMarkdown(meeting) },
              "divider",
              { label: "Delete meeting…", icon: Trash2, danger: true, disabled: isActive, onSelect: () => onDelete(id) },
            ]}
          />
        )}
      </div>

      <header className="mt-detail-header">
        <TitleField value={title} onChange={setTitle} />
        <div className="mt-meta">
          <span className="mt-chip" title={longDate(meeting.createdAt)}>
            {noteDay(meeting.createdAt)}
          </span>
          <span className="hstack muted" style={{ gap: 6 }}>
            <AudioLines size={14} aria-hidden="true" />
            {meeting.modelName}
          </span>
        </div>
      </header>

      <div className="tabs mt-tabs" role="tablist" aria-label="Meeting">
        {(
          [
            ["thoughts", "My thoughts", null],
            ["transcript", "Transcript", AudioLines],
            ["summary", "Summary", Sparkles],
          ] as const
        ).map(([value, label, Icon]) => (
          <button
            key={value}
            type="button"
            role="tab"
            id={`mt-tab-${value}`}
            aria-selected={tab === value}
            aria-controls="mt-tabpanel"
            className={tab === value ? "on" : ""}
            onClick={() => setTab(value)}
          >
            {Icon && <Icon size={15} aria-hidden="true" className={value === "transcript" && isRecording ? "mt-live" : "mt-tab-icon"} />}
            {label}
          </button>
        ))}
      </div>

      <div className="mt-tabpanel" id="mt-tabpanel" role="tabpanel" aria-labelledby={`mt-tab-${tab}`}>
        {tab === "thoughts" && (
          <div className="stack" style={{ gap: 18 }}>
            {meeting.summary ? (
              <div className="hstack faint" style={{ fontSize: 13, flexWrap: "wrap" }}>
                <span>{needsUpdate ? "Include your latest thoughts in the summary." : "Your summary includes these thoughts."}</span>
                <Button size="small" disabled={!canGenerate} onClick={() => actions.generate(id)}>
                  Update summary
                </Button>
              </div>
            ) : (
              <div className="muted" style={{ fontSize: 13 }}>
                Your thoughts, in your words.
              </div>
            )}
            <textarea
              className="mt-thoughts"
              aria-label="My thoughts"
              placeholder="Jot down questions, ideas, or anything you want to remember. Your summary will take these into account."
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
          </div>
        )}
        {tab === "transcript" && <Transcript meeting={meeting} />}
        {tab === "summary" && (
          <SummaryTab
            meeting={meeting}
            needsUpdate={needsUpdate}
            canGenerate={canGenerate}
            availability={availability}
            notesModel={notesModel}
            onGenerate={() => actions.generate(id)}
            onModels={() => setPage("models")}
          />
        )}
      </div>

      <Footer meeting={meeting} notesModel={notesModel} />
    </div>
  );
}

function TitleField({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [value]);
  return (
    <textarea
      ref={ref}
      rows={1}
      className="mt-title mt-serif"
      aria-label="Meeting title"
      placeholder="New note"
      value={value}
      onChange={(e) => onChange(e.target.value.replace(/\r?\n/g, " "))}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          e.currentTarget.blur();
        }
      }}
    />
  );
}

// MARK: Transcript

function Transcript({ meeting }: { meeting: Meeting }) {
  const { meetings, run } = useStore();
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [showsModelNote, setShowsModelNote] = useState(true);
  const [showsEchoes, setShowsEchoes] = useState(false);
  const listening = useRef<HTMLDivElement>(null);
  const id = meeting.id;
  const isRecording = capturingId(meetings.activity) === id;
  const isActive = isActiveIn(meetings, id);

  // Follow new words while recording, unless you're searching.
  useEffect(() => {
    if (!isRecording || query) return;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    listening.current?.scrollIntoView({ block: "end", behavior: reduce ? "auto" : "smooth" });
  }, [meeting.segments.length, isRecording, query]);

  const cleaned = removingEchoes(meeting.segments);
  const segments = (showsEchoes ? meeting.segments : cleaned).filter((s) => !query || contains(s.text, query));

  return (
    <div className="stack" style={{ gap: 18 }}>
      <div className="mt-transcript-bar">
        <div className="hstack muted" style={{ padding: "8px 8px 8px 14px" }}>
          <Clock size={15} aria-hidden="true" />
          <span className="mt-digits" aria-label="Duration">
            {durationLabel(isRecording ? meetings.elapsed : meeting.duration)}
          </span>
          <div className="spacer" />
          <Button
            variant="ghost"
            size="icon"
            icon={Search}
            title="Find in transcript"
            onClick={() => {
              setSearching(!searching);
              setQuery("");
            }}
          />
          <Button
            variant="ghost"
            size="icon"
            icon={Copy}
            title="Copy transcript"
            disabled={meeting.segments.length === 0}
            onClick={() => run(() => api.meetingCopy(id, true))}
          />
        </div>
        {searching && (
          <div style={{ padding: "0 14px 14px" }}>
            <input
              className="input"
              style={{ width: "100%" }}
              autoFocus
              placeholder="Find in transcript"
              aria-label="Find in transcript"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setSearching(false);
                  setQuery("");
                }
              }}
            />
          </div>
        )}
        {isRecording && showsModelNote && (
          <div className="mt-model-note muted">
            <span style={{ flex: 1 }}>Transcribing with {meeting.modelName}.</span>
            <Button variant="ghost" size="icon" icon={X} title="Dismiss" onClick={() => setShowsModelNote(false)} />
          </div>
        )}
      </div>

      {meeting.segments.length === 0 && !isRecording ? (
        <div className="mt-transcript-empty muted">
          <div className="mt-serif" style={{ fontSize: 21, fontStyle: "italic" }}>
            No transcript yet
          </div>
          <div style={{ fontSize: 13 }}>Your own thoughts are saved in My thoughts.</div>
        </div>
      ) : (
        <>
          {cleaned.length < meeting.segments.length && (
            <label className="hstack faint" style={{ gap: 10 }}>
              <Toggle on={showsEchoes} onChange={setShowsEchoes} label="Show repeated microphone audio" />
              Show repeated microphone audio
            </label>
          )}
          {segments.length === 0 && meeting.segments.length > 0 && <div className="muted">No matching words</div>}
          <div className="mt-segments">
            {segments.map((segment) => (
              <SegmentRow key={segment.id} segment={segment} />
            ))}
          </div>
          {isRecording && (
            <div ref={listening} className="stack" style={{ gap: 8 }}>
              <div className="hstack" style={{ gap: 6 }}>
                <strong>Listening…</strong>
                {meetings.pendingChunks > 0 && <span className="spinner mt-spinner-small" role="img" aria-label="Transcribing" />}
              </div>
              {meeting.segments.length === 0 && (
                <div className="mt-bubble faint" style={{ fontSize: 15 }}>
                  Words appear here a few seconds after they're spoken.
                </div>
              )}
            </div>
          )}
        </>
      )}
      {isActive && !isRecording && meetings.pendingChunks > 0 && (
        <Working>
          Transcribing… {meetings.pendingChunks} {meetings.pendingChunks === 1 ? "part" : "parts"} left
        </Working>
      )}
    </div>
  );
}

function SegmentRow({ segment }: { segment: MeetingSegment }) {
  return (
    <div className="mt-segment">
      <div className="mt-segment-head">
        <span className={`mt-dot ${segment.speaker}`} aria-hidden="true" />
        <strong>{speakerLabel(segment)}</strong>
        <span className="muted mt-digits">{timestamp(segment)}</span>
      </div>
      <div className="mt-bubble selectable">{segment.text}</div>
    </div>
  );
}

// MARK: Summary

function SummaryTab({
  meeting,
  needsUpdate,
  canGenerate,
  availability,
  notesModel,
  onGenerate,
  onModels,
}: {
  meeting: Meeting;
  needsUpdate: boolean;
  canGenerate: boolean;
  availability: string | null;
  notesModel: string;
  onGenerate: () => void;
  onModels: () => void;
}) {
  const { meetings, run } = useStore();
  const summary = meeting.summary;
  const isActive = isActiveIn(meetings, meeting.id);
  const modelsLink = availability && (
    <div className="hstack" style={{ flexWrap: "wrap" }}>
      <span className="muted" style={{ fontSize: 13 }}>
        {availability}
      </span>
      <Button size="small" onClick={onModels}>
        Open Models
      </Button>
    </div>
  );

  if (summary) {
    return (
      <div className="stack" style={{ gap: 26 }}>
        <div className="hstack" style={{ alignItems: "baseline" }}>
          {needsUpdate && (
            <span className="muted" style={{ fontSize: 13 }}>
              Update to include your latest thoughts and transcript.
            </span>
          )}
          <div className="spacer" />
          <Button
            icon={Sparkles}
            disabled={!canGenerate}
            title={`Combine the full transcript and your current thoughts using ${notesModel}`}
            onClick={onGenerate}
          >
            Update summary
          </Button>
        </div>
        {summary.overview && <p className="mt-overview selectable">{summary.overview}</p>}
        {summary.keyPoints.length > 0 && <Bullets title="Key points" items={summary.keyPoints} />}
        {summary.decisions.length > 0 && <Bullets title="Decisions" items={summary.decisions} />}
        {summary.actionItems.length > 0 && (
          <section className="mt-summary-section">
            <h2>Action items</h2>
            {summary.actionItems.map((item) => (
              <ActionRow key={item.id} item={item} onToggle={() => run(() => api.meetingToggleAction(meeting.id, item.id))} />
            ))}
          </section>
        )}
        <div className="faint">{summary.modelName ? `Generated with ${summary.modelName}` : "Generated summary"}</div>
        {modelsLink}
      </div>
    );
  }

  if (isActive) {
    return (
      <div className="stack" style={{ gap: 10 }}>
        <h2 className="hstack">
          <Sparkles size={16} aria-hidden="true" />A summary when you're done
        </h2>
        <div className="muted">Finish recording to turn your transcript and thoughts into key points, decisions, and action items.</div>
        {modelsLink}
      </div>
    );
  }

  const unavailable = availability ?? (hasContent(meeting) ? null : NOTHING_TO_SUMMARIZE);
  return (
    <div className="mt-write-card">
      <div className="mt-serif" style={{ fontSize: 24, lineHeight: "32px" }}>
        Bring it all together
      </div>
      <div className="muted">
        {availability ? "A notes model" : notesModel} turns your transcript and thoughts into a summary, key points, decisions, and action items.
      </div>
      <div>
        <Button variant="primary" icon={Sparkles} disabled={unavailable !== null || meetings.activity.kind !== "idle"} onClick={onGenerate}>
          Generate summary
        </Button>
      </div>
      {availability ? modelsLink : unavailable && <div className="muted" style={{ fontSize: 13 }}>{unavailable}</div>}
    </div>
  );
}

function Bullets({ title, items }: { title: string; items: string[] }) {
  return (
    <section className="mt-summary-section">
      <h2>{title}</h2>
      <ul className="mt-bullets">
        {items.map((item, i) => (
          <li key={i} className="selectable">
            {item}
          </li>
        ))}
      </ul>
    </section>
  );
}

function ActionRow({ item, onToggle }: { item: ActionItem; onToggle: () => void }) {
  return (
    <button type="button" role="checkbox" aria-checked={item.isDone} className={`mt-action ${item.isDone ? "done" : ""}`} onClick={onToggle}>
      {item.isDone ? <CheckCircle2 size={18} className="mt-action-check" aria-hidden="true" /> : <Circle size={18} className="muted" aria-hidden="true" />}
      <span>{item.text}</span>
    </button>
  );
}

// MARK: Footer

function Footer({ meeting, notesModel }: { meeting: Meeting; notesModel: string }) {
  const { meetings, run } = useStore();
  const { activity } = meetings;
  const id = meeting.id;
  const mine = activity.kind !== "idle" && activity.id === id;
  const issue = meetings.systemAudioIssue ?? (meetings.showsCallAudioHint ? CALL_AUDIO_HINT : null);
  const stop = () => run(() => api.meetingStop());

  let body;
  if (mine && activity.kind === "starting") {
    body = (
      <div className="hstack">
        <Working>Preparing {meeting.modelName}…</Working>
        <div className="spacer" />
        <Button onClick={stop}>Cancel</Button>
      </div>
    );
  } else if (mine && activity.kind === "recording") {
    body = <Recording onStop={stop} />;
  } else if (mine && activity.kind === "finishing") {
    body = <Working>Transcribing the last few seconds…</Working>;
  } else if (mine && activity.kind === "generating") {
    const step = meetings.generationStep;
    body = (
      <div className="hstack">
        <Working>{step && step[1] > 1 ? `Writing summary… part ${step[0]} of ${step[1]}` : `Writing summary with ${notesModel}…`}</Working>
        <div className="spacer" />
        <Button onClick={() => run(() => api.meetingCancelNotes())}>Cancel</Button>
      </div>
    );
  } else {
    body = (
      <div className="hstack muted" style={{ fontSize: 13 }}>
        <HardDrive size={15} aria-hidden="true" />
        Notes stored on this PC
        <div className="spacer" />
        <span className="mt-digits">{durationLabel(meeting.duration)}</span>
      </div>
    );
  }

  return (
    <footer className="mt-footer">
      {mine && issue && (
        <div className="hstack muted" style={{ fontSize: 13, alignItems: "flex-start" }}>
          <VolumeX size={15} aria-hidden="true" style={{ marginTop: 3, flexShrink: 0 }} />
          <span style={{ flex: 1 }}>{issue}</span>
          <Button size="small" onClick={() => run(() => api.openWindowsSettings("sound"))}>
            Open Sound settings
          </Button>
        </div>
      )}
      {body}
    </footer>
  );
}

function Recording({ onStop }: { onStop: () => void }) {
  const { meetings } = useStore();
  const [me, them] = useMeetingLevels();
  return (
    <div className="stack" style={{ gap: 10 }}>
      <div className="faint" style={{ textAlign: "center" }}>
        Always get consent when transcribing others.
      </div>
      <div className="hstack" style={{ gap: 10 }}>
        <button type="button" className="mt-stop" aria-label="Stop meeting" onClick={onStop}>
          <span className="mt-stop-square" aria-hidden="true" />
          Stop
        </button>
        <div className="mt-levels">
          <span className="hstack muted" style={{ gap: 6, minWidth: 0 }} title="Microphone recording “Me”">
            {meetings.microphone ? <Mic size={15} aria-hidden="true" /> : <MicOff size={15} aria-hidden="true" />}
            <span className="mt-ellipsis">{meetings.microphone ?? "No microphone"}</span>
          </span>
          <div className="spacer" />
          <LevelMeter label="Me" level={me} />
          <LevelMeter label="Them" level={them} />
          <span className="mt-digits muted">{durationLabel(meetings.elapsed)}</span>
        </div>
      </div>
    </div>
  );
}

function LevelMeter({ label, level }: { label: "Me" | "Them"; level: number }) {
  const value = Math.min(1, Math.max(0, level));
  return (
    <span
      className="hstack"
      style={{ gap: 5 }}
      role="meter"
      aria-label={`${label} level`}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value * 100)}
      aria-valuetext={`${Math.round(value * 100)} percent`}
    >
      <span className="faint">{label}</span>
      <span className="mt-meter">
        <span className={label === "Me" ? "me" : "them"} style={{ width: `${value * 100}%` }} />
      </span>
    </span>
  );
}
