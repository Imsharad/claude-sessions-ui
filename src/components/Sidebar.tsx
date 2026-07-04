/**
 * Left sidebar: project navigation.
 * - "All sessions" entry at top
 * - Pinned projects (star icon)
 * - All projects grouped, with session counts
 * - Search input that filters the main list
 *
 * Selection drives the SessionList filter. Pinning persists via the
 * toggle_pin command (writes through to SQLite).
 */
import { useState } from "react";
import { Search, Star, Folder, Hash } from "lucide-react";
import type { SessionCard } from "../lib/ipc";
import { togglePin } from "../lib/ipc";
import { shortCwd } from "../lib/format";

interface ProjectGroup {
  projectDir: string;
  cwd: string;
  display: string;
  count: number;
  pinned: boolean;
  lastTs: string | null;
}

interface SidebarProps {
  sessions: SessionCard[];
  selectedProject: string | null; // null = "All"
  onSelectProject: (dir: string | null) => void;
  query: string;
  onQueryChange: (q: string) => void;
  onPinnedChange: () => void; // refresh after pin toggle
}

export function Sidebar({
  sessions,
  selectedProject,
  onSelectProject,
  query,
  onQueryChange,
  onPinnedChange,
}: SidebarProps) {
  const [showAll, setShowAll] = useState(false);

  // ponytail: Drop useMemo, computing groups is fast enough for <1000 items
  const map = new Map<string, ProjectGroup>();
  for (const s of sessions) {
    const existing = map.get(s.projectDir);
    if (existing) {
      existing.count += 1;
      if (s.lastTs && (!existing.lastTs || s.lastTs > existing.lastTs)) existing.lastTs = s.lastTs;
    } else {
      map.set(s.projectDir, {
        projectDir: s.projectDir,
        cwd: s.cwd,
        display: s.displayProject || s.cwd.split("/").pop() || s.cwd,
        count: 1,
        pinned: s.pinned,
        lastTs: s.lastTs,
      });
    }
  }

  const allGroups = [...map.values()].sort((a, b) => a.pinned !== b.pinned ? (a.pinned ? -1 : 1) : (b.lastTs || "").localeCompare(a.lastTs || ""));
  const pinned = allGroups.filter((g) => g.pinned);
  const others = allGroups.filter((g) => !g.pinned);
  const visibleOthers = showAll ? others : others.slice(0, 12);

  const handlePin = async (dir: string) => {
    try { await togglePin(dir); onPinnedChange(); }
    catch (e) { console.error("pin failed", e); }
  };

  return (
    <aside className="flex h-full w-64 flex-col border-r border-border bg-surface-2/50">
      <div className="p-3 pb-2">
        <div className="relative">
          <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-ink-3" />
          <input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder="Search sessions & recaps…"
            className="w-full rounded border border-border bg-surface py-2 pl-8 pr-3 text-[13px] text-ink placeholder:text-ink-4 shadow-xs transition focus:border-accent focus:outline-none"
          />
        </div>
      </div>

      <nav className="flex-1 overflow-y-auto px-2 pb-4">
        <ProjectItem active={selectedProject === null} onClick={() => onSelectProject(null)} icon={<Hash size={14} />} label="All sessions" count={sessions.length} countStyle="muted" />

        {pinned.length > 0 && <div className="mt-4 px-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-4">Pinned</div>}
        {pinned.map((g) => (
          <ProjectItem key={g.projectDir} active={selectedProject === g.projectDir} onClick={() => onSelectProject(g.projectDir)} icon={<Star size={14} className="fill-warn text-warn" />} label={g.display} sublabel={shortCwd(g.cwd)} count={g.count} onPin={() => handlePin(g.projectDir)} pinned />
        ))}

        <div className="mt-4 px-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-4">Projects {others.length > 12 && !showAll && `(${others.length})`}</div>
        {visibleOthers.map((g) => (
          <ProjectItem key={g.projectDir} active={selectedProject === g.projectDir} onClick={() => onSelectProject(g.projectDir)} icon={<Folder size={14} className="text-ink-3" />} label={g.display} sublabel={shortCwd(g.cwd)} count={g.count} onPin={() => handlePin(g.projectDir)} />
        ))}
        {others.length > 12 && (
          <button onClick={() => setShowAll((v) => !v)} className="mt-1 w-full rounded-sm px-3 py-1.5 text-left text-[12px] text-ink-3 transition hover:bg-surface-3 hover:text-ink-2">
            {showAll ? "Show less" : `Show ${others.length - 12} more`}
          </button>
        )}
        {others.length === 0 && pinned.length === 0 && <p className="px-3 py-2 text-[12px] text-ink-4">No projects indexed.</p>}
      </nav>
    </aside>
  );
}

interface ProjectItemProps {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  sublabel?: string;
  count?: number;
  countStyle?: "default" | "muted";
  pinned?: boolean;
  onPin?: () => void;
}

function ProjectItem({
  active,
  onClick,
  icon,
  label,
  sublabel,
  count,
  countStyle = "default",
  pinned,
  onPin,
}: ProjectItemProps) {
  return (
    <div
      onClick={onClick}
      className={`group relative flex cursor-pointer items-center gap-2 rounded-sm px-3 py-1.5 text-[13px] transition ${
        active
          ? "bg-accent-soft text-accent-strong"
          : "text-ink-2 hover:bg-surface-3"
      }`}
    >
      <span className={active ? "text-accent" : "text-ink-3"}>{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium">{label}</div>
        {sublabel && (
          <div className="truncate font-mono text-[10px] text-ink-4">
            {sublabel}
          </div>
        )}
      </div>
      {count !== undefined && (
        <span
          className={
            countStyle === "muted"
              ? "text-[11px] tabular-nums text-ink-4"                    // muted: bare number, no pill
              : "rounded-full px-1.5 py-0.5 text-[10px] font-medium tabular-nums " +
                (active
                  ? "bg-accent/15 text-accent-strong"
                  : "bg-surface-3 text-ink-3")
          }
        >
          {count}
        </span>
      )}
      {onPin && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onPin();
          }}
          className="absolute right-1 opacity-0 transition group-hover:opacity-100"
          title={pinned ? "Unpin" : "Pin to top"}
        >
          <Star
            size={12}
            className={
              pinned
                ? "fill-warn text-warn"
                : "text-ink-4 hover:text-warn"
            }
          />
        </button>
      )}
    </div>
  );
}
