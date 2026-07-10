/**
 * Top bar: app title, view switcher (Launcher / Analytics / Review), global
 * stats summary, and reindex action. Drag region for the window (Tauri native
 * feel — the bar moves the window on macOS).
 */
import { RefreshCw, Loader2, Layers, BarChart3, BookOpen, List, LayoutGrid, Keyboard, Calendar, Home } from "lucide-react";
import type { IndexStatus, GlobalStats } from "../lib/ipc";
import { formatTokens, formatCost } from "../lib/format";

export type View = "home" | "launcher" | "timeline" | "analytics" | "review";
export type LauncherMode = "list" | "board" | "triage";

interface Props {
  view: View;
  onViewChange: (v: View) => void;
  launcherMode: LauncherMode;
  onLauncherModeChange: (m: LauncherMode) => void;
  status: IndexStatus | null;
  stats: GlobalStats | null;
  reindexing: boolean;
  onReindex: () => void;
}

export function TopBar({ view, onViewChange, launcherMode, onLauncherModeChange, status, stats, reindexing, onReindex }: Props) {
  const views: { id: View; label: string; icon: React.ReactNode }[] = [
    { id: "home", label: "Home", icon: <Home size={12} /> },
    { id: "launcher", label: "Launcher", icon: <Layers size={12} /> },
    { id: "timeline", label: "Timeline", icon: <Calendar size={12} /> },
    { id: "analytics", label: "Analytics", icon: <BarChart3 size={12} /> },
    { id: "review", label: "Review", icon: <BookOpen size={12} /> },
  ];
  const modes: { id: LauncherMode; label: string; icon: React.ReactNode }[] = [
    { id: "list", label: "List view", icon: <List size={12} /> },
    { id: "board", label: "Board view", icon: <LayoutGrid size={12} /> },
    { id: "triage", label: "Triage (keyboard tagging)", icon: <Keyboard size={12} /> },
  ];

  return (
    <header data-tauri-drag-region className="flex items-center justify-between border-b border-border bg-surface/80 px-4 py-2 backdrop-blur">
      <div className="flex items-center gap-3" data-tauri-drag-region>
        <div className="flex items-center gap-1.5">
          <div className="flex h-5 w-5 items-center justify-center rounded-sm bg-accent text-[11px] font-bold text-white">C</div>
          <span className="text-[13px] font-semibold text-ink">Sessions</span>
        </div>
        {status && status.sessionCount > 0 && (
          <span className="text-[11px] text-ink-4">{status.sessionCount} indexed · {status.recapCount} recaps</span>
        )}
      </div>

      <div className="flex items-center gap-2">
        <div className="flex items-center gap-0.5 rounded bg-surface-2 p-0.5">
          {views.map((v) => (
            <button
              key={v.id}
              onClick={() => onViewChange(v.id)}
              className={`inline-flex items-center gap-1 rounded-sm px-2.5 py-1 text-[12px] font-medium transition ${view === v.id ? "bg-surface text-ink shadow-xs" : "text-ink-3 hover:text-ink-2"}`}
            >
              {v.icon}
              {v.label}
            </button>
          ))}
        </div>

        {/* List | board toggle — a quiet second segmented control, only in the
            launcher. Same pill styling as the views group; icon-only to stay
            subordinate. */}
        {view === "launcher" && (
          <div className="flex items-center gap-0.5 rounded bg-surface-2 p-0.5">
            {modes.map((m) => (
              <button
                key={m.id}
                title={m.label}
                onClick={() => onLauncherModeChange(m.id)}
                className={`inline-flex items-center justify-center rounded-sm px-2 py-1 transition ${launcherMode === m.id ? "bg-surface text-ink shadow-xs" : "text-ink-3 hover:text-ink-2"}`}
              >
                {m.icon}
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center gap-3">
        {stats && (
          <div className="hidden items-center gap-3 text-[11px] text-ink-3 md:flex">
            <span className="tabular-nums"><span className="text-ink-4">cost</span> {formatCost(stats.totalCostUsd)}{stats.estimatedCostUsd > stats.measuredCostUsd && <span className="ml-0.5 text-[9px] text-ink-4">est</span>}</span>
            <span className="tabular-nums"><span className="text-ink-4">tokens</span> {formatTokens(stats.totalInputToks + stats.totalOutputToks)}</span>
          </div>
        )}
        <button onClick={onReindex} disabled={reindexing} title="Re-scan for new sessions" className="inline-flex items-center gap-1 rounded-sm border border-border bg-surface px-2 py-1 text-[11px] text-ink-2 transition hover:bg-surface-3 disabled:opacity-50">
          {reindexing ? <Loader2 size={11} className="animate-spin" /> : <RefreshCw size={11} />}
          {reindexing ? "Scanning…" : "Refresh"}
        </button>
      </div>
    </header>
  );
}
