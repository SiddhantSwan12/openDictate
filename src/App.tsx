import { BarChart3, BookText, Clock, Cpu, Info, LayoutGrid, NotebookPen, Settings as SettingsIcon, Type, type LucideIcon } from "lucide-react";
import { useEffect } from "react";
import type { Page } from "./lib/api";
import { StoreProvider, useStore } from "./lib/store";
import { Banner, Tile } from "./components/ui";
import { BrandMark } from "./components/BrandMark";
import { Overview } from "./pages/Overview";
import { Insights } from "./pages/Insights";
import { Meetings } from "./pages/Meetings";
import { History } from "./pages/History";
import { Models } from "./pages/Models";
import { Vocabulary } from "./pages/Vocabulary";
import { Style } from "./pages/Style";
import { Settings } from "./pages/Settings";
import { About } from "./pages/About";
import { Onboarding } from "./pages/Onboarding";

export const PAGES: { id: Page; title: string; icon: LucideIcon; color: string }[] = [
  { id: "overview", title: "Overview", icon: LayoutGrid, color: "#0a84ff" },
  { id: "insights", title: "Insights", icon: BarChart3, color: "#14b8a6" },
  { id: "meetings", title: "Notetaker", icon: NotebookPen, color: "#eab308" },
  { id: "history", title: "History", icon: Clock, color: "#f97316" },
  { id: "models", title: "Models", icon: Cpu, color: "#a855f7" },
  { id: "vocabulary", title: "Vocabulary", icon: BookText, color: "#22c55e" },
  { id: "style", title: "Style", icon: Type, color: "#ec4899" },
  { id: "settings", title: "Settings", icon: SettingsIcon, color: "#71717a" },
  { id: "about", title: "About", icon: Info, color: "#6366f1" },
];

function Shell() {
  const { page, setPage, workspace, session, error, setError } = useStore();

  // Esc cancels a dictation in progress, like BetterWispr's menu command.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const busy = ["preparing", "recording", "transcribing"].includes(session.phase.kind);
      if (e.key === "Escape" && busy) import("./lib/api").then(({ api }) => api.cancelRecording());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [session.phase.kind]);

  if (workspace.needsOnboarding) return <Onboarding />;

  const Current = { overview: Overview, insights: Insights, meetings: Meetings, history: History, models: Models, vocabulary: Vocabulary, style: Style, settings: Settings, about: About }[page];
  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Pages">
        <div className="brand">
          <BrandMark size={26} />
          OpenDictate
        </div>
        {PAGES.map((p) => (
          <button key={p.id} className={`nav-item ${page === p.id ? "active" : ""}`} onClick={() => setPage(p.id)} aria-current={page === p.id}>
            <Tile icon={p.icon} color={p.color} size={22} />
            {p.title}
          </button>
        ))}
        <div className="sidebar-footer">Free and private. Everything runs on this PC.</div>
      </nav>
      <main className="content">
        <div className="page">
          {error && (
            <Banner tone="error" onClose={() => setError(null)}>
              {error}
            </Banner>
          )}
          <Current />
        </div>
      </main>
    </div>
  );
}

export function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}
