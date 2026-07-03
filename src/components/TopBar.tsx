/**
 * Top bar: app title, view switcher (Launcher / Analytics / Digest), global
 * stats summary, and reindex action. Drag region for the window (Tauri native
 * feel — the bar moves the window on macOS).
 */
import { RefreshCw, Loader2, Layers, BarChart3, BookOpen } from "lucide-react";
import type { IndexStatus, GlobalStats } from "../lib/ipc";
import { formatTokens, formatCost } from "../lib/format";

export type View = "launcher" | "analytics" | "digest";

interface Props {
  view: View;
  onViewChange: (v: View) => void;
  status: IndexStatus | null;
  stats: GlobalStats | null;
  reindexing: boolean;
  onReindex: () => void;
}

export function TopBar({
  view,
  onViewChange,
  status,
  stats,
  reindexing,
  onReindex,
}: Props) {
  return (
    <header
      data-tauri-drag-region
      className="flex items-center justify-between border-b border-border bg-surface/80 px-4 py-2 backdrop-blur"
    >
      {/* Left: title + scan status */}
      <div className="flex items-center gap-3" data-tauri-drag-region>
        <div className="flex items-center gap-1.5">
          <div className="flex h-5 w-5 items-center justify-center rounded-sm bg-accent text-[11px] font-bold text-white">
            C
          </div>
          <span className="text-[13px] font-semibold text-ink">Sessions</span>
        </div>
        {status && status.sessionCount > 0 && (
          <span className="text-[11px] text-ink-4">
            {status.sessionCount} indexed · {status.recapCount} recaps
          </span>
        )}
      </div>

      {/* Center: view switcher — input-scale (10px) so its contained chips (6px) nest within. */}
      <div className="flex items-center gap-0.5 rounded bg-surface-2 p-0.5">
        <ViewButton
          active={view === "launcher"}
          onClick={() => onViewChange("launcher")}
          icon={<Layers size={12} />}
          label="Launcher"
        />
        <ViewButton
          active={view === "analytics"}
          onClick={() => onViewChange("analytics")}
          icon={<BarChart3 size={12} />}
          label="Analytics"
        />
        <ViewButton
          active={view === "digest"}
          onClick={() => onViewChange("digest")}
          icon={<BookOpen size={12} />}
          label="Digest"
        />
      </div>

      {/* Right: stats + reindex */}
      <div className="flex items-center gap-3">
        {stats && (
          <div className="hidden items-center gap-3 text-[11px] text-ink-3 md:flex">
            <span className="tabular-nums">
              <span className="text-ink-4">cost</span>{" "}
              {formatCost(stats.totalCostUsd)}
              {stats.estimatedCostUsd > stats.measuredCostUsd && (
                <span className="ml-0.5 text-[9px] text-ink-4">est</span>
              )}
            </span>
            <span className="tabular-nums">
              <span className="text-ink-4">tokens</span>{" "}
              {formatTokens(
                stats.totalInputToks + stats.totalOutputToks,
              )}
            </span>
          </div>
        )}
        <button
          onClick={onReindex}
          disabled={reindexing}
          title="Re-scan for new sessions"
          className="inline-flex items-center gap-1 rounded-sm border border-border bg-surface px-2 py-1 text-[11px] text-ink-2 transition hover:bg-surface-3 disabled:opacity-50"
        >
          {reindexing ? (
            <Loader2 size={11} className="animate-spin" />
          ) : (
            <RefreshCw size={11} />
          )}
          {reindexing ? "Scanning…" : "Refresh"}
        </button>
      </div>
    </header>
  );
}

function ViewButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`inline-flex items-center gap-1 rounded-sm px-2.5 py-1 text-[12px] font-medium transition ${
        active
          ? "bg-surface text-ink shadow-xs"
          : "text-ink-3 hover:text-ink-2"
      }`}
    >
      {icon}
      {label}
    </button>
  );
}
