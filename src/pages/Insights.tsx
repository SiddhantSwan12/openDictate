// Port of BetterWispr's InsightsView (Features/Insights/InsightsView.swift).
import {
  BarChart3,
  BookText,
  ChevronLeft,
  ChevronRight,
  Copy,
  FileText,
  Infinity as InfinityIcon,
  Info,
  Mail,
  MessageSquare,
  MessagesSquare,
  Sparkles,
  WandSparkles,
  type LucideIcon,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { api, type AppCategory, type UsageInsights } from "../lib/api";
import { useStore } from "../lib/store";
import { Button, Empty, PageHeader } from "../components/ui";
import "./Insights.css";

const CATEGORIES: Record<AppCategory, { label: string; icon: LucideIcon }> = {
  aiPrompts: { label: "AI prompts", icon: Sparkles },
  work: { label: "work messages", icon: MessageSquare },
  personal: { label: "personal messages", icon: MessagesSquare },
  documents: { label: "documents", icon: FileText },
  email: { label: "emails", icon: Mail },
  other: { label: "other tasks", icon: InfinityIcon },
};

/** Mean of 168,000 typists in Dhakal et al., "Observations on Typing from 136 Million Keystrokes", CHI 2018. */
const TYPING_WPM = 52;

const plural = (n: number, word: string) => `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;

export function Insights() {
  const { workspace, run } = useStore();
  const { history, insights, settings } = workspace;

  if (history.length === 0) {
    return (
      <>
        <PageHeader title="Insights" />
        <Empty icon={BarChart3} title="No insights yet">
          {settings.saveHistory
            ? "Dictate a few times and your pace, streak and favorite apps appear here."
            : "Insights are built from saved dictations. Turn on Save history in Settings to see them."}
        </Empty>
      </>
    );
  }

  const share = `I've dictated ${insights.totalWords.toLocaleString()} words with OpenDictate at ${insights.wordsPerMinute} words per minute, on a ${insights.currentStreak} day streak.`;

  return (
    <div className="insights">
      <PageHeader
        title="Insights"
        subtitle="Built from your saved dictations, on this PC."
        actions={
          <Button icon={Copy} title="Copy a summary of your insights" onClick={() => run(() => api.copyText(share))}>
            Copy summary
          </Button>
        }
      />
      {!settings.saveHistory && (
        <div className="hstack muted">
          <Info size={16} />
          History is off, so new dictations are not counted.
        </div>
      )}
      <div className="insights-row insights-row-3">
        <InsightCard>
          <Pace wordsPerMinute={insights.wordsPerMinute} />
        </InsightCard>
        <InsightCard>
          <Fixes insights={insights} />
        </InsightCard>
        <InsightCard>
          <TotalWords insights={insights} dictations={history.length} />
        </InsightCard>
      </div>
      <div className="insights-row insights-row-2">
        <InsightCard>
          <AppUsage insights={insights} dictations={history.length} />
        </InsightCard>
        <InsightCard>
          <StreakCalendar insights={insights} />
        </InsightCard>
      </div>
    </div>
  );
}

function InsightCard({ children }: { children: ReactNode }) {
  return <div className="card insight-card">{children}</div>;
}

function StatHeader({ value, title, help }: { value: number; title: string; help?: string }) {
  return (
    <div title={help}>
      <div className="insight-value">{value.toLocaleString()}</div>
      <div className="insight-caption">{title}</div>
    </div>
  );
}

function Pace({ wordsPerMinute }: { wordsPerMinute: number }) {
  const ratio = wordsPerMinute / TYPING_WPM;
  const scale = Math.max(wordsPerMinute, TYPING_WPM, 1);
  return (
    <>
      <StatHeader value={wordsPerMinute} title="Words per minute" help="Words divided by recording time, pauses included." />
      <hr />
      <div>
        {ratio >= 1.1 ? `${ratio.toFixed(1)}× faster than typing` : "Pauses count toward your pace, so long silences slow it down."}
      </div>
      <PaceBar label="You, speaking" wpm={wordsPerMinute} fraction={wordsPerMinute / scale} you />
      <PaceBar
        label="Average typist"
        wpm={TYPING_WPM}
        fraction={TYPING_WPM / scale}
        help="Mean typing speed of 168,000 people measured by Dhakal et al., CHI 2018."
      />
    </>
  );
}

function PaceBar({ label, wpm, fraction, you, help }: { label: string; wpm: number; fraction: number; you?: boolean; help?: string }) {
  return (
    <div className="pace-bar" title={help ?? `${label}: ${wpm} words per minute`} role="img" aria-label={`${label}, ${wpm} words per minute`}>
      <div className="insight-caption hstack">
        <span className="spacer">{label}</span>
        <span className="tabular">{wpm} wpm</span>
      </div>
      <div className="pace-track">
        <div className={you ? "pace-fill you" : "pace-fill"} style={{ width: `max(10px, ${fraction * 100}%)` }} />
      </div>
    </div>
  );
}

function Fixes({ insights }: { insights: UsageInsights }) {
  return (
    <>
      <StatHeader value={insights.wordsCleaned + insights.dictionaryFixes} title="Fixes made by OpenDictate" />
      <hr />
      <div className="hstack" title="Filler words, repeats and edits, counted against each original transcription.">
        <WandSparkles size={16} className="muted" />
        {insights.wordsCleaned.toLocaleString()} words cleaned up
      </div>
      <div className="hstack" title="Words respelled from your Vocabulary.">
        <BookText size={16} className="muted" />
        {insights.dictionaryFixes.toLocaleString()} vocabulary fixes
      </div>
    </>
  );
}

function TotalWords({ insights, dictations }: { insights: UsageInsights; dictations: number }) {
  const novels = Math.floor(insights.totalWords / 50_000);
  const pages = Math.max(1, Math.floor(insights.totalWords / 250));
  return (
    <>
      <StatHeader value={insights.totalWords} title="Total words dictated" />
      <hr />
      <div>
        {novels > 0 ? `That's ${plural(novels, "novel")}, at 50,000 words each.` : `That's about ${plural(pages, "page")}, at 250 words a page.`}
      </div>
      <div className="muted">{plural(dictations, "dictation")} saved</div>
    </>
  );
}

function AppUsage({ insights, dictations }: { insights: UsageInsights; dictations: number }) {
  const entries = Object.entries(insights.dictationsByCategory) as [AppCategory, number][];
  const tracked = entries.reduce((sum, [, n]) => sum + n, 0);
  const rows = entries
    .filter(([, n]) => n > 0)
    .sort(([a, x], [b, y]) => (x === y ? CATEGORIES[a].label.localeCompare(CATEGORIES[b].label) : y - x));
  return (
    <>
      <div className="insight-title-row">
        <h2 className="insight-title">App usage</h2>
        <span className="insight-caption">Apps used | {insights.appsUsed}</span>
      </div>
      {rows.length === 0 && <div className="muted">New dictations record the app they were sent to, so this fills in as you dictate.</div>}
      {rows.map(([category, count]) => (
        <UsageBar key={category} category={category} count={count} fraction={count / Math.max(tracked, 1)} />
      ))}
      {tracked < dictations && rows.length > 0 && <div className="faint">Dictations saved without an app recorded are not counted.</div>}
    </>
  );
}

function UsageBar({ category, count, fraction }: { category: AppCategory; count: number; fraction: number }) {
  const { label, icon: Icon } = CATEGORIES[category];
  const percent = `${Math.round(fraction * 100)}%`;
  return (
    <div className="usage-bar" role="img" aria-label={`${count} ${label}, ${percent}`} title={`${count.toLocaleString()} ${label}, ${percent}`}>
      <Icon size={16} className="muted" />
      <div className="usage-track">
        <div
          className="usage-fill"
          style={{
            width: `max(44px, ${fraction * 60}%)`,
            background: `color-mix(in srgb, var(--insights) ${Math.round((0.25 + 0.35 * fraction) * 100)}%, transparent)`,
          }}
        >
          {percent}
        </div>
        <span className="insight-caption usage-label">
          {count.toLocaleString()} {label}
        </span>
      </div>
    </div>
  );
}

// MARK: Streak calendar

const WEEKS = 18;
const DAY_MS = 86_400_000;

function dayKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
}

function parseKey(key: string): Date {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** First day of the week for the user's locale, as a JS weekday (0 = Sunday). */
function firstWeekday(): number {
  try {
    const locale = new Intl.Locale(navigator.language) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    const info = locale.getWeekInfo?.() ?? locale.weekInfo;
    if (info) return info.firstDay % 7;
  } catch {
    // Fall through to Sunday.
  }
  return 0;
}

function levelColor(level: number): string {
  return level === 0
    ? "color-mix(in srgb, var(--text-3) 18%, transparent)"
    : `color-mix(in srgb, var(--insights) ${Math.round((0.25 + (0.75 * level) / 4) * 100)}%, transparent)`;
}

function StreakCalendar({ insights }: { insights: UsageInsights }) {
  const [page, setPage] = useState(0);
  const first = firstWeekday();
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const thisWeek = addDays(today, -((today.getDay() - first + 7) % 7));
  const columns = Array.from({ length: WEEKS }, (_, i) => addDays(thisWeek, 7 * (i - (WEEKS - 1) - page * WEEKS)));

  const values = Object.values(insights.wordsByDay);
  const busiest = Math.max(1, ...values);
  const keys = Object.keys(insights.wordsByDay).sort();
  const oldest = keys.length ? parseKey(keys[0]) : today;

  const streakEnd = insights.wordsByDay[dayKey(today)] ? today : addDays(today, -1);
  const streak = new Set(Array.from({ length: insights.currentStreak }, (_, i) => dayKey(addDays(streakEnd, -i))));

  const weekdayFormat = new Intl.DateTimeFormat(undefined, { weekday: "short" });
  const weekdays = Array.from({ length: 7 }, (_, i) => weekdayFormat.format(addDays(thisWeek, i)));
  const monthFormat = new Intl.DateTimeFormat(undefined, { month: "short" });
  const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

  return (
    <>
      <div className="insight-title-row">
        <h2 className="insight-title">{insights.currentStreak}-day streak</h2>
        <span className="insight-caption">Longest streak | {plural(insights.longestStreak, "day")}</span>
      </div>
      <div className="calendar">
        <div className="calendar-weekdays" aria-hidden="true">
          {weekdays.map((d, i) => (
            <span key={i}>{d}</span>
          ))}
        </div>
        <div className="calendar-body">
          <div className="calendar-head">
            <div className="calendar-months" aria-hidden="true">
              {columns.map((week, i) => {
                const starts = i === 0 || week.getMonth() !== columns[i - 1].getMonth();
                return <span key={i}>{starts ? monthFormat.format(week) : ""}</span>;
              })}
            </div>
            <span className="spacer" />
            <Button
              variant="ghost"
              size="icon"
              icon={ChevronLeft}
              title="Earlier weeks"
              disabled={columns[0].getTime() <= oldest.getTime()}
              onClick={() => setPage(page + 1)}
            />
            <Button variant="ghost" size="icon" icon={ChevronRight} title="Later weeks" disabled={page === 0} onClick={() => setPage(page - 1)} />
          </div>
          <div className="calendar-grid" role="grid" aria-label="Words dictated each day">
            {columns.map((week, i) => (
              <div key={i} className="calendar-week" role="row">
                {Array.from({ length: 7 }, (_, offset) => {
                  const date = addDays(week, offset);
                  const key = dayKey(date);
                  const words = insights.wordsByDay[key] ?? 0;
                  const level = words === 0 ? 0 : Math.max(1, Math.ceil((4 * words) / busiest));
                  const future = date.getTime() > today.getTime() + DAY_MS / 2;
                  const label = `${dateFormat.format(date)}: ${words.toLocaleString()} words`;
                  return (
                    <span
                      key={offset}
                      role="gridcell"
                      className={streak.has(key) ? "calendar-cell streak" : "calendar-cell"}
                      style={{ background: future ? "transparent" : levelColor(level) }}
                      title={future ? undefined : label}
                      aria-label={future ? undefined : label}
                    />
                  );
                })}
              </div>
            ))}
          </div>
        </div>
      </div>
      <div className="calendar-legend faint">
        Less
        {[0, 1, 2, 3, 4].map((level) => (
          <span key={level} className="calendar-cell" style={{ background: levelColor(level) }} />
        ))}
        More
        <span className="spacer" />
        <span className="calendar-cell streak" />
        Current streak
      </div>
    </>
  );
}
