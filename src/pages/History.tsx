// Port of BetterWispr's HistoryView: search, fix and reuse saved dictations.
import { save } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, Clock, Copy, Download, Pencil, RotateCcw, Search, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { api, durationLabel, type Transcript } from "../lib/api";
import { useStore } from "../lib/store";
import { Banner, Button, Empty, Modal, PageHeader } from "../components/ui";
import "./History.css";

/** Friendly names for common Windows executables; anything else is title-cased. */
const APP_NAMES: Record<string, string> = {
  "ms-teams": "Teams",
  teams: "Teams",
  winword: "Word",
  excel: "Excel",
  powerpnt: "PowerPoint",
  onenote: "OneNote",
  olk: "Outlook",
  outlook: "Outlook",
  hxoutlook: "Mail",
  msedge: "Edge",
  chrome: "Chrome",
  firefox: "Firefox",
  brave: "Brave",
  opera: "Opera",
  "whatsapp.root": "WhatsApp",
  whatsapp: "WhatsApp",
  chatgpt: "ChatGPT",
  code: "VS Code",
  windowsterminal: "Terminal",
  explorer: "File Explorer",
  notepad: "Notepad",
  wordpad: "WordPad",
};

/** "slack.exe" → "Slack". */
export function appLabel(exe: string): string {
  const base = exe.replace(/\.exe$/i, "");
  const known = APP_NAMES[base.toLowerCase()];
  if (known) return known;
  return base.charAt(0).toUpperCase() + base.slice(1);
}

function fold(text: string): string {
  return text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
}

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

function dayTitle(date: Date): string {
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (dayKey(date) === dayKey(today)) return "Today";
  if (dayKey(date) === dayKey(yesterday)) return "Yesterday";
  return date.toLocaleDateString(undefined, {
    weekday: "long",
    month: "long",
    day: "numeric",
    year: date.getFullYear() === today.getFullYear() ? undefined : "numeric",
  });
}

function plural(count: number, word: string): string {
  return `${count.toLocaleString()} ${word}${count === 1 ? "" : "s"}`;
}

export function History() {
  const { workspace, run, setPage } = useStore();
  const { history, settings, shortcutName } = workspace;
  const [query, setQuery] = useState("");
  const [toDelete, setToDelete] = useState<Transcript | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = fold(query.trim());
    return q ? history.filter((t) => fold(t.text).includes(q)) : history;
  }, [history, query]);

  const groups = useMemo(() => {
    const result: { key: string; title: string; items: Transcript[] }[] = [];
    for (const transcript of filtered) {
      const date = new Date(transcript.createdAt);
      const key = dayKey(date);
      const last = result[result.length - 1];
      if (last && last.key === key) last.items.push(transcript);
      else result.push({ key, title: dayTitle(date), items: [transcript] });
    }
    return result;
  }, [filtered]);

  async function exportHistory() {
    const path = await save({
      defaultPath: "OpenDictate-history.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return;
    if (await run(() => api.exportHistory(path))) setNotice(`History exported to ${path}.`);
  }

  async function clearAll() {
    setConfirmClear(false);
    await run(() => api.clearHistory());
  }

  async function deleteTranscript() {
    const target = toDelete;
    setToDelete(null);
    if (target) await run(() => api.deleteTranscript(target.id));
  }

  return (
    <>
      <PageHeader
        title="History"
        subtitle={history.length ? plural(history.length, "dictation") + " saved on this PC" : "Your dictations, saved on this PC"}
        actions={
          <>
            <Button icon={Download} onClick={exportHistory} disabled={!history.length}>
              Export
            </Button>
            <Button icon={Trash2} variant="danger" onClick={() => setConfirmClear(true)} disabled={!history.length}>
              Clear all
            </Button>
          </>
        }
      />

      {!settings.saveHistory && (
        <Banner tone="info">
          <div className="hstack" style={{ justifyContent: "space-between" }}>
            <span>Saving history is off, so new dictations aren't kept. Turn on “Save dictation history” in Settings.</span>
            <Button size="small" onClick={() => setPage("settings")}>
              Open Settings
            </Button>
          </div>
        </Banner>
      )}

      {notice && (
        <Banner tone="info" onClose={() => setNotice(null)}>
          {notice}
        </Banner>
      )}

      {history.length > 0 && (
        <div className="history-search">
          <Search size={16} aria-hidden="true" />
          <input
            className="input"
            type="search"
            placeholder="Search dictations"
            aria-label="Search dictations"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setQuery("")}
          />
        </div>
      )}

      {filtered.length === 0 ? (
        query.trim() ? (
          <Empty icon={Search} title={`No results for “${query.trim()}”`}>
            Check the spelling or try a new search.
          </Empty>
        ) : (
          <Empty icon={Clock} title="No dictations">
            Your saved dictations will appear here. Start speaking with {shortcutName}.
          </Empty>
        )
      ) : (
        <>
          {query.trim() && <div className="faint">{plural(filtered.length, "dictation")} found</div>}
          {groups.map((group) => (
            <section className="section" key={group.key}>
              <div className="section-title">
                <h2>{group.title}</h2>
                <span className="faint">{plural(group.items.length, "dictation")}</span>
              </div>
              <div className="card history-list">
                {group.items.map((t) => (
                  <TranscriptRow key={t.id} transcript={t} onDelete={() => setToDelete(t)} />
                ))}
              </div>
            </section>
          ))}
          {settings.learnCorrections && (
            <p className="faint" style={{ margin: 0 }}>
              Fix a misheard word with the pencil and OpenDictate adds it to Vocabulary, so it is spelled right next time.
            </p>
          )}
        </>
      )}

      {toDelete && (
        <Modal
          title="Delete this dictation?"
          onClose={() => setToDelete(null)}
          footer={
            <>
              <Button onClick={() => setToDelete(null)}>Cancel</Button>
              <Button variant="primary" onClick={deleteTranscript}>
                Delete
              </Button>
            </>
          }
        >
          <p className="muted" style={{ margin: 0 }}>
            This removes the saved text from this PC. It cannot be undone.
          </p>
        </Modal>
      )}

      {confirmClear && (
        <Modal
          title="Clear all history?"
          onClose={() => setConfirmClear(false)}
          footer={
            <>
              <Button onClick={() => setConfirmClear(false)}>Cancel</Button>
              <Button variant="primary" onClick={clearAll}>
                Clear history
              </Button>
            </>
          }
        >
          <p className="muted" style={{ margin: 0 }}>
            This removes all {plural(history.length, "saved dictation")} from this PC. It cannot be undone. Export your history
            first if you want to keep a copy.
          </p>
        </Modal>
      )}
    </>
  );
}

function TranscriptRow({ transcript, onDelete }: { transcript: Transcript; onDelete: () => void }) {
  const { run } = useStore();
  const [draft, setDraft] = useState<string | null>(null);
  const [showOriginal, setShowOriginal] = useState(false);
  const time = new Date(transcript.createdAt).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  const edited = transcript.text !== transcript.rawText;
  const canRestore = transcript.rawText.trim() !== "" && transcript.text !== transcript.rawText.trim();

  async function saveDraft() {
    if (draft === null || !draft.trim()) return;
    const text = draft;
    if (await run(() => api.updateTranscript(transcript.id, text))) setDraft(null);
  }

  const meta = [
    time,
    durationLabel(transcript.duration),
    transcript.modelName,
    transcript.language === "auto" ? "Auto language" : transcript.language.toUpperCase(),
    transcript.app ? appLabel(transcript.app) : null,
  ].filter(Boolean);

  return (
    <article className="history-item">
      <div className="history-meta">
        <span className="faint">
          {meta.map((part, i) => (
            <span key={i}>
              {i > 0 && <span aria-hidden="true"> · </span>}
              {part}
            </span>
          ))}
        </span>
        <span className="spacer" />
        <Button variant="ghost" size="icon" icon={Copy} title="Copy dictation" onClick={() => run(() => api.copyText(transcript.text))} />
        {draft === null && (
          <Button variant="ghost" size="icon" icon={Pencil} title="Fix dictation" onClick={() => setDraft(transcript.text)} />
        )}
        <Button variant="ghost" size="icon" icon={Trash2} title="Delete dictation" onClick={onDelete} />
      </div>

      {draft !== null ? (
        <div className="stack" style={{ gap: 8 }}>
          <textarea
            className="textarea"
            rows={Math.min(10, Math.max(3, Math.ceil(draft.length / 90)))}
            value={draft}
            autoFocus
            aria-label="Dictation text"
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                setDraft(null);
              } else if (e.key === "Enter" && e.ctrlKey) {
                e.preventDefault();
                saveDraft();
              }
            }}
          />
          <div className="hstack" style={{ justifyContent: "flex-end" }}>
            <span className="faint">Ctrl + Enter saves, Esc cancels</span>
            <Button size="small" onClick={() => setDraft(null)}>
              Cancel
            </Button>
            <Button size="small" variant="primary" onClick={saveDraft} disabled={!draft.trim()}>
              Save
            </Button>
          </div>
        </div>
      ) : (
        <p className="history-text selectable">{transcript.text}</p>
      )}

      {edited && (
        <div className="history-original">
          <button
            type="button"
            className="history-disclosure"
            aria-expanded={showOriginal}
            onClick={() => setShowOriginal(!showOriginal)}
          >
            {showOriginal ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            Original transcription
          </button>
          {showOriginal && (
            <div className="history-original-body">
              <p className="selectable">{transcript.rawText}</p>
              {canRestore && (
                <div>
                  <Button
                    size="small"
                    icon={RotateCcw}
                    onClick={() => run(() => api.restoreOriginal(transcript.id))}
                  >
                    Use original
                  </Button>
                  <span className="faint" style={{ marginLeft: 8 }}>
                    Replaces the cleaned-up text with exactly what was transcribed.
                  </span>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </article>
  );
}
