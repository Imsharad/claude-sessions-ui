/**
 * App shell. Owns the global state: index status, sessions list, selected
 * session, current view. On first run (no sessions indexed), runs a full scan
 * behind a loading screen. Subsequent launches do an incremental scan in the
 * background.
 *
 * View switching: Launcher (P2, this file), Analytics (P5, stub), Digest (P4).
 */
import { useEffect, useState } from "react";
import { TopBar, type View } from "./components/TopBar";
import { Sidebar } from "./components/Sidebar";
import { SessionList } from "./components/SessionList";
import { SessionDetail } from "./components/SessionDetail";
import { FirstRun } from "./components/FirstRun";
import { ErrorBoundary } from "./components/ErrorBoundary";
import {
  indexStatus,
  listSessions,
  reindex,
  getStats,
  type SessionCard,
  type IndexStatus,
  type GlobalStats,
} from "./lib/ipc";
import "./index.css";

export default function App() {
  const [status, setStatus] = useState<IndexStatus | null>(null);
  const [sessions, setSessions] = useState<SessionCard[]>([]);
  const [stats, setStats] = useState<GlobalStats | null>(null);
  const [bootstrapping, setBootstrapping] = useState<string | false>("Starting up…");
  const [reindexing, setReindexing] = useState(false);

  const [view, setView] = useState<View>("launcher");
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  // ponytail: Drop redundant useCallback, it's fine to recreate on re-render for App shell
  const refresh = async () => {
    const [st, sess, gs] = await Promise.all([indexStatus(), listSessions(), getStats()]);
    setStatus(st); setSessions(sess); setStats(gs);
  };

  useEffect(() => {
    indexStatus().then(async (st) => {
      setBootstrapping(st.sessionCount === 0 ? "Reading your Claude sessions for the first time…" : "Checking for new sessions…");
      await reindex(st.sessionCount === 0);
      await refresh();
      setBootstrapping(false);
    }).catch(e => console.error(e));
  }, []);

  const handleReindex = async () => {
    setReindexing(true);
    try { await reindex(false); await refresh(); }
    catch (e) { console.error(e); }
    finally { setReindexing(false); }
  };

  const visibleSessions = sessions.filter(s =>
    (!selectedProject || s.projectDir === selectedProject) &&
    (!query || (s.title + (s.recap || "")).toLowerCase().includes(query.toLowerCase()))
  );

  if (bootstrapping !== false) return <FirstRun message={bootstrapping} />;

  return (
    <ErrorBoundary>
      <div className="flex h-full flex-col">
        <TopBar
          view={view}
          onViewChange={setView}
          status={status}
          stats={stats}
          reindexing={reindexing}
          onReindex={handleReindex}
        />

        {view === "launcher" && (
          <div className="flex min-h-0 flex-1">
            <Sidebar
              sessions={sessions}
              selectedProject={selectedProject}
              onSelectProject={(dir) => {
                setSelectedProject(dir);
                setSelectedSession(null);
              }}
              query={query}
              onQueryChange={setQuery}
              onPinnedChange={refresh}
            />
            <SessionList
              sessions={visibleSessions}
              selectedId={selectedSession}
              onSelect={setSelectedSession}
            />
            <div className="flex w-[420px] shrink-0 flex-col border-l border-border">
              <SessionDetail sessionId={selectedSession} />
            </div>
          </div>
        )}

        {view === "analytics" && <ComingSoon title="Analytics" />}
        {view === "digest" && <ComingSoon title="Digest" />}
      </div>
    </ErrorBoundary>
  );
}

function ComingSoon({ title }: { title: string }) {
  return (
    <div className="flex flex-1 items-center justify-center bg-canvas">
      <div className="text-center">
        <h2 className="text-[15px] font-semibold text-ink-2">{title}</h2>
        <p className="mt-1 text-[13px] text-ink-3">Coming in the next phase.</p>
      </div>
    </div>
  );
}
