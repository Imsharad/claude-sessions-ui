/**
 * App shell. Owns the global state: index status, sessions list, selected
 * session, current view. On first run (no sessions indexed), runs a full scan
 * behind a loading screen. Subsequent launches do an incremental scan in the
 * background.
 *
 * View switching: Launcher (P2, this file), Analytics (P5, stub), Digest (P4).
 */
import { useEffect, useState, useCallback } from "react";
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
  const [bootstrapping, setBootstrapping] = useState(true);
  const [bootstrapMsg, setBootstrapMsg] = useState("Starting up…");
  const [reindexing, setReindexing] = useState(false);

  const [view, setView] = useState<View>("launcher");
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  // Refresh sessions + status + stats from the DB.
  const refresh = useCallback(async () => {
    const [st, sess, gs] = await Promise.all([
      indexStatus(),
      listSessions(),
      getStats(),
    ]);
    setStatus(st);
    setSessions(sess);
    setStats(gs);
  }, []);

  // First-run bootstrap: if no sessions indexed, do a full scan; else incremental.
  useEffect(() => {
    (async () => {
      try {
        const st = await indexStatus();
        if (st.sessionCount === 0) {
          setBootstrapMsg("Reading your Claude sessions for the first time…");
          await reindex(true);
        } else {
          // Background incremental — keep the UI snappy.
          setBootstrapMsg("Checking for new sessions…");
          await reindex(false);
        }
        await refresh();
      } catch (e) {
        console.error("bootstrap failed", e);
      } finally {
        setBootstrapping(false);
      }
    })();
  }, [refresh]);

  const handleReindex = useCallback(async () => {
    setReindexing(true);
    try {
      await reindex(false);
      await refresh();
    } catch (e) {
      console.error("reindex failed", e);
    } finally {
      setReindexing(false);
    }
  }, [refresh]);

  // Filter sessions client-side by project + query (server already filters, but
  // this keeps the sidebar's live counts coherent).
  const visibleSessions =
    selectedProject || query
      ? sessions.filter(
          (s) =>
            (!selectedProject || s.projectDir === selectedProject) &&
            (!query ||
              s.title.toLowerCase().includes(query.toLowerCase()) ||
              (s.recap && s.recap.toLowerCase().includes(query.toLowerCase()))),
        )
      : sessions;

  if (bootstrapping) {
    return <FirstRun message={bootstrapMsg} />;
  }

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
