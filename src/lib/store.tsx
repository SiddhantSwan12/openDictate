// App-wide state kept in sync with backend events.
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { api, errorText, on, type AppSettings, type MeetingsView, type Page, type Session, type Workspace } from "./api";

interface Store {
  session: Session;
  workspace: Workspace;
  meetings: MeetingsView;
  page: Page;
  setPage: (page: Page) => void;
  /** Meeting open in the Notetaker page; null shows the list. */
  openMeetingId: string | null;
  setOpenMeetingId: (id: string | null) => void;
  /** Saves a settings change; shows the error inline if the backend rejects it. */
  saveSettings: (change: (settings: AppSettings) => AppSettings) => Promise<void>;
  /** Runs an action and reports failures in the in-app banner. */
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  error: string | null;
  setError: (error: string | null) => void;
}

const Context = createContext<Store | null>(null);

export function useStore(): Store {
  const store = useContext(Context);
  if (!store) throw new Error("useStore outside StoreProvider");
  return store;
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<Session | null>(null);
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [meetings, setMeetings] = useState<MeetingsView | null>(null);
  const [page, setPage] = useState<Page>("overview");
  const [openMeetingId, setOpenMeetingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const workspaceRef = useRef<Workspace | null>(null);
  workspaceRef.current = workspace;

  useEffect(() => {
    const unlisten = [
      on<Session>("session", setSession),
      on<Workspace>("workspace", setWorkspace),
      on<MeetingsView>("meetings", setMeetings),
      on<Page>("navigate", setPage),
      on<string>("open-meeting", (id) => {
        setPage("meetings");
        setOpenMeetingId(id);
      }),
    ];
    api.getSession().then(setSession);
    api.getWorkspace().then(setWorkspace);
    api.getMeetings().then(setMeetings);
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, []);

  const run = useCallback(async (action: () => Promise<unknown>) => {
    try {
      await action();
      setError(null);
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    }
  }, []);

  const saveSettings = useCallback(
    async (change: (settings: AppSettings) => AppSettings) => {
      const current = workspaceRef.current;
      if (!current) return;
      const next = change(structuredClone(current.settings));
      // Optimistic update keeps controls responsive; the backend echoes the saved workspace.
      setWorkspace({ ...current, settings: next });
      const ok = await run(() => api.updateSettings(next));
      if (!ok) setWorkspace(current);
    },
    [run],
  );

  if (!session || !workspace || !meetings) {
    return <div className="boot" />;
  }

  return (
    <Context.Provider
      value={{
        session,
        workspace,
        meetings,
        page,
        setPage,
        openMeetingId,
        setOpenMeetingId,
        saveSettings,
        run,
        error,
        setError,
      }}
    >
      {children}
    </Context.Provider>
  );
}

/** Microphone level 0...1 while recording, updated about 20 times per second. */
export function useLevel(): number {
  const [level, setLevel] = useState(0);
  useEffect(() => {
    const unlisten = on<number>("level", setLevel);
    return () => {
      unlisten.then((f) => f());
    };
  }, []);
  return level;
}

/** Meeting levels [me, them], 0...1. */
export function useMeetingLevels(): [number, number] {
  const [levels, setLevels] = useState<[number, number]>([0, 0]);
  useEffect(() => {
    const unlisten = on<[number, number]>("meeting-levels", setLevels);
    return () => {
      unlisten.then((f) => f());
    };
  }, []);
  return levels;
}

/** Live recording time in seconds. */
export function useRecordingDuration(initial: number): number {
  const [duration, setDuration] = useState(initial);
  useEffect(() => {
    const unlisten = on<number>("recording-duration", setDuration);
    return () => {
      unlisten.then((f) => f());
    };
  }, []);
  return duration;
}
