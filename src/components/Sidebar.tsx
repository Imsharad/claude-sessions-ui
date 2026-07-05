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
import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Search, Star, Folder, Hash, EyeOff, X, Plus } from "lucide-react";
import type { SessionCard, BlacklistEntry, BlacklistPreview } from "../lib/ipc";
import {
  togglePin,
  listBlacklist,
  addBlacklistPattern,
  removeBlacklistPattern,
  previewBlacklistPattern,
} from "../lib/ipc";
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
  // Refresh sessions after a hide/unhide. rescan=true reindexes first —
  // required on unhide, since scan-skipped sessions were never indexed.
  onBlacklistChange: (rescan?: boolean) => void | Promise<void>;
}

export function Sidebar({
  sessions,
  selectedProject,
  onSelectProject,
  query,
  onQueryChange,
  onPinnedChange,
  onBlacklistChange,
}: SidebarProps) {
  const [showAll, setShowAll] = useState(false);

  // Hidden-projects (blacklist) manage panel. Loaded once so the count reads
  // from launch; mutators return the refreshed list in one round-trip.
  const [showHidden, setShowHidden] = useState(false);
  const [blacklist, setBlacklist] = useState<BlacklistEntry[]>([]);
  const [newPattern, setNewPattern] = useState("");
  const [hiddenError, setHiddenError] = useState<string | null>(null);
  const [preview, setPreview] = useState<BlacklistPreview | null>(null);

  useEffect(() => {
    listBlacklist().then(setBlacklist).catch((e) => console.error("blacklist load failed", e));
  }, []);

  // Live preview of what the typed pattern would hide, debounced so we don't
  // round-trip on every keystroke.
  useEffect(() => {
    const p = newPattern.trim();
    if (!p) {
      setPreview(null);
      return;
    }
    const t = setTimeout(() => {
      previewBlacklistPattern(p).then(setPreview).catch(() => setPreview(null));
    }, 250);
    return () => clearTimeout(t);
  }, [newPattern]);

  const handleAddHidden = async () => {
    try {
      setBlacklist(await addBlacklistPattern(newPattern));
      setNewPattern("");
      setHiddenError(null);
      onBlacklistChange();
    } catch (e) {
      setHiddenError(String(e));
    }
  };

  // Native folder picker — the zero-syntax way to hide whole trees. macOS
  // allows multi-select, so several parent dirs land in one gesture; each
  // picked dir becomes its own pattern (individually removable later).
  const handlePickFolders = async () => {
    try {
      const picked = await openDialog({
        directory: true,
        multiple: true,
        title: "Hide sessions under these folders",
      });
      if (!picked) return; // cancelled
      const dirs = Array.isArray(picked) ? picked : [picked];
      let list = blacklist;
      for (const d of dirs) list = await addBlacklistPattern(d);
      setBlacklist(list);
      setHiddenError(null);
      // Deselect if the active project filter just vanished under a picked dir.
      const selCwd = sessions.find((s) => s.projectDir === selectedProject)?.cwd;
      if (selCwd && dirs.some((d) => `${selCwd}/`.startsWith(`${d.replace(/\/+$/, "")}/`))) {
        onSelectProject(null);
      }
      onBlacklistChange();
    } catch (e) {
      setHiddenError(String(e));
    }
  };

  // One-click hide from a project row: the row's own cwd is the pattern, so
  // no glob syntax is involved. Deselect first if the hidden project is the
  // active filter — otherwise the list would sit on an invisible project.
  const handleHideProject = async (g: ProjectGroup) => {
    try {
      setBlacklist(await addBlacklistPattern(g.cwd));
      setHiddenError(null);
      if (selectedProject === g.projectDir) onSelectProject(null);
      onBlacklistChange();
    } catch (e) {
      setHiddenError(String(e));
      setShowHidden(true); // the error renders inside the panel
    }
  };

  const handleRemoveHidden = async (pattern: string) => {
    try {
      setBlacklist(await removeBlacklistPattern(pattern));
      setHiddenError(null);
      // Rescan: sessions created while the tree was hidden were never indexed.
      await onBlacklistChange(true);
      // Counts of remaining patterns can shift once the rescan lands.
      setBlacklist(await listBlacklist());
    } catch (e) {
      setHiddenError(String(e));
    }
  };

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
          <ProjectItem key={g.projectDir} active={selectedProject === g.projectDir} onClick={() => onSelectProject(g.projectDir)} icon={<Star size={14} className="fill-warn text-warn" />} label={g.display} sublabel={shortCwd(g.cwd)} count={g.count} onPin={() => handlePin(g.projectDir)} pinned onHide={() => handleHideProject(g)} />
        ))}

        <div className="mt-4 px-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-4">Projects {others.length > 12 && !showAll && `(${others.length})`}</div>
        {visibleOthers.map((g) => (
          <ProjectItem key={g.projectDir} active={selectedProject === g.projectDir} onClick={() => onSelectProject(g.projectDir)} icon={<Folder size={14} className="text-ink-3" />} label={g.display} sublabel={shortCwd(g.cwd)} count={g.count} onPin={() => handlePin(g.projectDir)} onHide={() => handleHideProject(g)} />
        ))}
        {others.length > 12 && (
          <button onClick={() => setShowAll((v) => !v)} className="mt-1 w-full rounded-sm px-3 py-1.5 text-left text-[12px] text-ink-3 transition hover:bg-surface-3 hover:text-ink-2">
            {showAll ? "Show less" : `Show ${others.length - 12} more`}
          </button>
        )}
        {others.length === 0 && pinned.length === 0 && <p className="px-3 py-2 text-[12px] text-ink-4">No projects indexed.</p>}
      </nav>

      <div className="border-t border-border px-2 pt-2 pb-3">
        <button
          onClick={() => setShowHidden((v) => !v)}
          title="Hidden projects"
          className={`flex w-full items-center gap-2 rounded-sm px-3 py-1.5 text-[13px] transition hover:bg-surface-3 ${
            showHidden ? "text-ink-2" : "text-ink-3 hover:text-ink-2"
          }`}
        >
          <EyeOff size={14} className="text-ink-3" />
          <span className="flex-1 text-left font-medium">Hidden projects</span>
          {blacklist.length > 0 && (
            <span className="text-[11px] tabular-nums text-ink-4">{blacklist.length}</span>
          )}
        </button>

        <AnimatePresence initial={false}>
          {showHidden && (
            <motion.div
              key="hidden-panel"
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ type: "spring", stiffness: 400, damping: 30 }}
              className="overflow-hidden"
            >
              <div className="pt-1">
                {blacklist.length === 0 ? (
                  <p className="px-3 py-1.5 text-[11.5px] leading-snug text-ink-4">
                    Declutter: choose folders to hide, or hover a project and
                    click the eye.
                  </p>
                ) : (
                  blacklist.map((b) => (
                    <div
                      key={b.pattern}
                      className="group flex items-center gap-2 rounded-sm px-3 py-1 text-ink-2 transition hover:bg-surface-3"
                    >
                      <span className="min-w-0 flex-1 truncate font-mono text-[11px]" title={b.pattern}>
                        {b.pattern}
                      </span>
                      <span className="text-[11px] tabular-nums text-ink-4" title="Sessions hidden">
                        {b.matchCount}
                      </span>
                      <button
                        onClick={() => handleRemoveHidden(b.pattern)}
                        className="shrink-0 transition"
                        title="Stop hiding this project"
                      >
                        <X size={12} className="text-ink-4 transition-colors hover:text-ink-2" />
                      </button>
                    </div>
                  ))
                )}

                <button
                  onClick={handlePickFolders}
                  className="mt-1.5 flex w-[calc(100%-16px)] items-center gap-2 rounded border border-dashed border-border px-3 py-1.5 mx-2 text-[12px] font-medium text-ink-3 transition hover:bg-surface-3 hover:text-ink-2"
                  title="Pick one or more folders; sessions under them are hidden"
                >
                  <Folder size={13} />
                  Choose folders to hide…
                </button>

                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    handleAddHidden();
                  }}
                  className="mt-1.5 flex items-center gap-1.5 px-2"
                >
                  <input
                    value={newPattern}
                    onChange={(e) => setNewPattern(e.target.value)}
                    placeholder="e.g. project-name/**"
                    className="min-w-0 flex-1 rounded border border-border bg-surface px-2 py-1 text-[11.5px] text-ink placeholder:text-ink-4 shadow-xs transition focus:border-accent focus:outline-none"
                  />
                  <button
                    type="submit"
                    className="shrink-0 rounded-sm p-1 text-ink-3 transition hover:bg-surface-3 hover:text-ink-2"
                    title="Hide project"
                  >
                    <Plus size={14} />
                  </button>
                </form>

                {newPattern.trim() !== "" && preview && (
                  <p className="mt-1 px-3 text-[11px] text-ink-4">
                    {preview.sessionCount > 0
                      ? `Will hide ${preview.projectCount} project${preview.projectCount === 1 ? "" : "s"} · ${preview.sessionCount} session${preview.sessionCount === 1 ? "" : "s"}`
                      : "Matches nothing currently indexed"}
                  </p>
                )}

                {hiddenError && (
                  <p className="mt-1 px-3 text-[11px] text-danger">{hiddenError}</p>
                )}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
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
  onHide?: () => void;
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
  onHide,
}: ProjectItemProps) {
  const hasActions = Boolean(onPin || onHide);
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
            (countStyle === "muted"
              ? "text-[11px] tabular-nums text-ink-4"                    // muted: bare number, no pill
              : "rounded-full px-1.5 py-0.5 text-[10px] font-medium tabular-nums " +
                (active
                  ? "bg-accent/15 text-accent-strong"
                  : "bg-surface-3 text-ink-3")) +
            // The hover actions land where the count sits; fade it out so the
            // two never overlap.
            (hasActions ? " transition group-hover:opacity-0" : "")
          }
        >
          {count}
        </span>
      )}
      {onHide && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onHide();
          }}
          className="absolute right-6 opacity-0 transition group-hover:opacity-100"
          title="Hide this project"
        >
          <EyeOff size={12} className="text-ink-4 hover:text-ink-2" />
        </button>
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
