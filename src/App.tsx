/**
 * App shell. Owns the global state: index status, sessions list, selected
 * session, current view. On first run (no sessions indexed), runs a full scan
 * behind a loading screen. Subsequent launches do an incremental scan in the
 * background.
 *
 * View switching: Launcher (P2, this file), Analytics (P5, stub), Review (P4).
 */
import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { TopBar, type View, type LauncherMode } from "./components/TopBar";
import { Sidebar } from "./components/Sidebar";
import { SessionList } from "./components/SessionList";
import { KanbanBoard, columnOf } from "./components/KanbanBoard";
import { SessionDetail } from "./components/SessionDetail";
import { TriageMode } from "./components/TriageMode";
import { TimelineView } from "./components/TimelineView";
import { ReviewView } from "./components/ReviewView";
import { HomeScreen } from "./components/HomeScreen";
import { FirstRun } from "./components/FirstRun";
import { ErrorBoundary } from "./components/ErrorBoundary";
import {
  indexStatus,
  listSessions,
  listThreads,
  reindex,
  getStats,
  type SessionCard,
  type IndexStatus,
  type GlobalStats,
  type HomeData,
} from "./lib/ipc";
import "./index.css";

export default function App() {
  const [status, setStatus] = useState<IndexStatus | null>(null);
  const [sessions, setSessions] = useState<SessionCard[]>([]);
  const [stats, setStats] = useState<GlobalStats | null>(null);
  const [home, setHome] = useState<HomeData | null>(null);
  const [bootstrapping, setBootstrapping] = useState<string | false>("Starting up…");
  const [reindexing, setReindexing] = useState(false);

  const [view, setView] = useState<View>("home");
  const [launcherMode, setLauncherMode] = useState<LauncherMode>("list");
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  // ponytail: Drop redundant useCallback, it's fine to recreate on re-render for App shell
  // Home ranking is recomputed here — on launch and after reindex only (this is
  // the one refresh path). Deliberately NOT re-fetched on focus/view switches:
  // stability of the home ranking between opens is part of the first-screen spec.
  const refresh = async () => {
    const [st, sess, gs, hd] = await Promise.all([indexStatus(), listSessions(), getStats(), listThreads(5)]);
    setStatus(st); setSessions(sess); setStats(gs); setHome(hd);
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

  // Un-hiding needs a rescan: sessions created while a tree was hidden were
  // skipped at scan time and never entered the index, so a plain refresh
  // would resurface only the rows that predate the pattern.
  const handleBlacklistChange = async (rescan = false) => {
    if (rescan) await handleReindex();
    else await refresh();
  };

  // Home → browse. Switch to the launcher, carry an optional query, and try to
  // preselect the sidebar project by matching the thread's display name against
  // a session's displayProject/projectShortName. No match → leave selection alone.
  const handleBrowse = (query?: string, projectHint?: string) => {
    setView("launcher");
    if (query != null) setQuery(query);
    if (projectHint) {
      const hint = projectHint.toLowerCase();
      const match = sessions.find(
        (s) =>
          s.displayProject?.toLowerCase() === hint ||
          s.projectShortName?.toLowerCase() === hint,
      );
      if (match) setSelectedProject(match.projectDir);
    }
  };

  const visibleSessions = sessions.filter(s =>
    (!selectedProject || s.projectDir === selectedProject) &&
    (!query || (s.title + (s.recap || "")).toLowerCase().includes(query.toLowerCase()))
  );

  // Switching to the board must not leak a list-mode selection that has no
  // board presence (untagged → no derivable column). Mirrors the project-change
  // clearing above. List mode keeps whatever is selected.
  const handleLauncherMode = (mode: LauncherMode) => {
    if (
      mode === "board" &&
      selectedSession &&
      !visibleSessions.some((s) => s.id === selectedSession && columnOf(s) !== null)
    ) {
      setSelectedSession(null);
    }
    setLauncherMode(mode);
  };

  // Escape dismisses the board's detail overlay (board scroll/column state
  // survive — the board stays mounted beneath the overlay).
  useEffect(() => {
    if (launcherMode !== "board" || !selectedSession) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSelectedSession(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [launcherMode, selectedSession]);

  // Same overlay-dismiss for the timeline's detail (it stays mounted beneath).
  useEffect(() => {
    if (view !== "timeline" || !selectedSession) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSelectedSession(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, selectedSession]);

  // Same overlay-dismiss for the Review tab's evidence detail (Tier 2).
  useEffect(() => {
    if (view !== "review" || !selectedSession) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSelectedSession(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, selectedSession]);

  if (bootstrapping !== false) return <FirstRun message={bootstrapping} />;

  return (
    <ErrorBoundary>
      <div className="flex h-full flex-col">
        <TopBar
          view={view}
          onViewChange={setView}
          launcherMode={launcherMode}
          onLauncherModeChange={handleLauncherMode}
          status={status}
          stats={stats}
          reindexing={reindexing}
          onReindex={handleReindex}
        />

        {view === "home" && home && <HomeScreen data={home} onBrowse={handleBrowse} />}

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
              onBlacklistChange={handleBlacklistChange}
            />
            {launcherMode === "triage" ? (
              // Keyboard triage: zero-latency manual tagging over the untagged queue.
              // Full-width beside the sidebar; refreshes the list once on exit.
              <TriageMode
                sessions={visibleSessions}
                onExit={() => setLauncherMode("list")}
                onTagged={refresh}
              />
            ) : launcherMode === "board" ? (
              // Board keeps all three columns co-visible: the detail mounts as a
              // right-anchored overlay (soft depth, no scrim). The board reserves
              // the pane's width and compresses columns instead of clipping them.
              <div className="relative flex min-h-0 flex-1">
                <KanbanBoard
                  sessions={visibleSessions}
                  selectedId={selectedSession}
                  onSelect={setSelectedSession}
                  onSessionsChanged={refresh}
                  detailOpen={selectedSession != null}
                />
                <AnimatePresence>
                  {selectedSession && (
                    <motion.aside
                      key="board-detail"
                      initial={{ x: "100%" }}
                      animate={{ x: 0 }}
                      exit={{ x: "100%" }}
                      transition={{ type: "spring", stiffness: 400, damping: 30 }}
                      className="absolute inset-y-0 right-0 z-20 flex w-[420px] flex-col overflow-hidden rounded-l-lg border-l border-border bg-surface shadow-lg"
                    >
                      <SessionDetail
                        sessionId={selectedSession}
                        onClose={() => setSelectedSession(null)}
                      />
                    </motion.aside>
                  )}
                </AnimatePresence>
              </div>
            ) : (
              <>
                <SessionList
                  sessions={visibleSessions}
                  selectedId={selectedSession}
                  onSelect={setSelectedSession}
                  onSessionsChanged={refresh}
                />
                <div className="flex w-[420px] shrink-0 flex-col border-l border-border">
                  <SessionDetail sessionId={selectedSession} />
                </div>
              </>
            )}
          </div>
        )}

        {view === "timeline" && (
          // Timeline is a full reading surface; selecting a session opens the
          // existing SessionDetail as a right-anchored overlay (same idiom as
          // the board — the timeline stays mounted, scroll survives).
          <div className="relative flex min-h-0 flex-1">
            <TimelineView
              sessions={sessions}
              selectedId={selectedSession}
              onSelect={setSelectedSession}
            />
            <AnimatePresence>
              {selectedSession && (
                <motion.aside
                  key="timeline-detail"
                  initial={{ x: "100%" }}
                  animate={{ x: 0 }}
                  exit={{ x: "100%" }}
                  transition={{ type: "spring", stiffness: 400, damping: 30 }}
                  className="absolute inset-y-0 right-0 z-20 flex w-[420px] flex-col overflow-hidden rounded-l-lg border-l border-border bg-surface shadow-lg"
                >
                  <SessionDetail
                    sessionId={selectedSession}
                    onClose={() => setSelectedSession(null)}
                  />
                </motion.aside>
              )}
            </AnimatePresence>
          </div>
        )}

        {view === "analytics" && <ComingSoon title="Analytics" />}

        {view === "review" && (
          // Review is a full reading surface; clicking an evidence chip (Tier 2)
          // opens the existing SessionDetail as a right-anchored overlay (same
          // idiom as the timeline — the review stays mounted, scroll survives).
          <div className="relative flex min-h-0 flex-1">
            <ReviewView
              sessions={sessions}
              selectedId={selectedSession}
              onSelect={setSelectedSession}
            />
            <AnimatePresence>
              {selectedSession && (
                <motion.aside
                  key="review-detail"
                  initial={{ x: "100%" }}
                  animate={{ x: 0 }}
                  exit={{ x: "100%" }}
                  transition={{ type: "spring", stiffness: 400, damping: 30 }}
                  className="absolute inset-y-0 right-0 z-20 flex w-[420px] flex-col overflow-hidden rounded-l-lg border-l border-border bg-surface shadow-lg"
                >
                  <SessionDetail
                    sessionId={selectedSession}
                    onClose={() => setSelectedSession(null)}
                  />
                </motion.aside>
              )}
            </AnimatePresence>
          </div>
        )}
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
