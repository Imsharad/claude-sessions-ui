/**
 * Left sidebar: project navigation.
 * - "All sessions" entry at top
 * - Pinned projects (star icon) — a cross-status flag, rendered first
 * - Projects grouped by lifecycle status (Active / Labs / Inbox / Archived)
 * - Search input that filters the main list
 * - Hidden-projects (blacklist) panel at the bottom
 *
 * The project list comes from `list_projects` (the FULL list, including
 * archived — which list_sessions filters out of SessionCard), so the sidebar
 * can surface and un-archive them. Right-click a project to set its status.
 * Selection drives the SessionList filter. Pinning persists via toggle_pin.
 */
import { useEffect, useState, useMemo } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Search, Star, Folder, Hash, EyeOff, X, Plus, ChevronRight } from "lucide-react";
import type { SessionCard, BlacklistEntry, BlacklistPreview, ProjectEntry } from "../lib/ipc";
import {
  togglePin,
  listBlacklist,
  listProjects,
  addBlacklistPattern,
  removeBlacklistPattern,
  previewBlacklistPattern,
  setProjectStatus,
  PROJECT_STATUS_LABELS,
  PROJECT_STATUS_ORDER,
  type ProjectStatus,
} from "../lib/ipc";
import { shortCwd } from "../lib/format";

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
  // Full project list (including archived, which SessionCard doesn't carry).
  // Reload on the same triggers the session list does — pin/hide/status writes
  // all route through refreshProjects so sections re-group in one round-trip.
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set(["archived"]));
  const [menu, setMenu] = useState<{ dir: string; x: number; y: number } | null>(null);

  const refreshProjects = () =>
    listProjects()
      .then(setProjects)
      .catch((e) => console.error("list_projects failed", e));

  useEffect(() => {
    refreshProjects();
  }, []);

  // Group by derived status. Pinned renders first as its own section (a
  // cross-status flag), then the four status sections in canonical order.
  const { pinned, sections } = useMemo(() => {
    const pinnedList = projects.filter((p) => p.pinned);
    const byStatus: Record<string, ProjectEntry[]> = {};
    for (const s of PROJECT_STATUS_ORDER) byStatus[s] = [];
    for (const p of projects) {
      const key = p.status ?? "active"; // null status → active section
      (byStatus[key] ?? byStatus.active).push(p);
    }
    return { pinned: pinnedList, sections: byStatus };
  }, [projects]);

  // Close the context menu on any click outside it, or on Escape.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  const handleSetStatus = async (dir: string, status: ProjectStatus | null) => {
    setMenu(null);
    try {
      await setProjectStatus(dir, status);
      await Promise.all([refreshProjects(), onBlacklistChange()]);
    } catch (e) {
      console.error("set_project_status failed", e);
    }
  };

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
  const handleHideProject = async (p: ProjectEntry) => {
    try {
      setBlacklist(await addBlacklistPattern(p.cwd));
      setHiddenError(null);
      if (selectedProject === p.encodedDir) onSelectProject(null);
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

  // Pin toggle also refreshes the project list so it re-sections (a pinned
  // archived project jumps to the Pinned section, etc.).
  const handlePin = async (dir: string) => {
    try {
      await togglePin(dir);
      await Promise.all([refreshProjects(), onPinnedChange()]);
    } catch (e) {
      console.error("pin failed", e);
    }
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

        {pinned.length > 0 && (
          <div className="mt-4 px-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-4">Pinned</div>
        )}
        {pinned.map((p) => (
          <ProjectItem
            key={p.encodedDir}
            active={selectedProject === p.encodedDir}
            onClick={() => onSelectProject(p.encodedDir)}
            onContextMenu={(e) => { e.preventDefault(); setMenu({ dir: p.encodedDir, x: e.clientX, y: e.clientY }); }}
            icon={<Star size={14} className="fill-warn text-warn" />}
            label={p.displayName}
            sublabel={shortCwd(p.cwd)}
            count={p.sessionCount}
            onPin={() => handlePin(p.encodedDir)}
            pinned
            onHide={() => handleHideProject(p)}
          />
        ))}

        {PROJECT_STATUS_ORDER.map((statusKey) => {
          const list = sections[statusKey] ?? [];
          if (list.length === 0) return null;
          const isCollapsed = collapsed.has(statusKey);
          const toggle = () =>
            setCollapsed((prev) => {
              const next = new Set(prev);
              if (next.has(statusKey)) next.delete(statusKey);
              else next.add(statusKey);
              return next;
            });
          return (
            <div key={statusKey}>
              <button
                onClick={toggle}
                className="mt-4 flex w-full items-center gap-1 px-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-4 hover:text-ink-3"
              >
                <ChevronRight size={10} className={isCollapsed ? "" : "rotate-90 transition-transform"} />
                {PROJECT_STATUS_LABELS[statusKey]}
                <span className="ml-auto font-normal text-ink-4 tabular-nums">{list.length}</span>
              </button>
              {!isCollapsed && list.map((p) => (
                <ProjectItem
                  key={p.encodedDir}
                  active={selectedProject === p.encodedDir}
                  onClick={() => onSelectProject(p.encodedDir)}
                  onContextMenu={(e) => { e.preventDefault(); setMenu({ dir: p.encodedDir, x: e.clientX, y: e.clientY }); }}
                  icon={<Folder size={14} className={statusKey === "archived" ? "text-ink-4" : "text-ink-3"} />}
                  label={p.displayName}
                  sublabel={shortCwd(p.cwd)}
                  count={p.sessionCount}
                  dimmed={statusKey === "archived"}
                  onPin={() => handlePin(p.encodedDir)}
                  onHide={() => handleHideProject(p)}
                />
              ))}
            </div>
          );
        })}

        {projects.length === 0 && <p className="px-3 py-2 text-[12px] text-ink-4">No projects indexed.</p>}
      </nav>

      {/* Right-click context menu: set lifecycle status. Closes on any outside
          click or Escape (listeners in the effect above). */}
      {menu && (
        <div
          className="fixed z-50 min-w-[160px] rounded-lg border border-border bg-surface py-1 shadow-lg"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <div className="px-3 py-1 text-[10px] font-semibold uppercase tracking-wider text-ink-4">
            Status
          </div>
          {PROJECT_STATUS_ORDER.map((s) => {
            const current = projects.find((p) => p.encodedDir === menu.dir)?.status;
            return (
              <button
                key={s}
                onClick={() => handleSetStatus(menu.dir, s)}
                className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] transition hover:bg-surface-2 ${
                  current === s ? "text-accent-strong" : "text-ink-2"
                }`}
              >
                <span className="w-3 text-center">{current === s ? "✓" : ""}</span>
                {PROJECT_STATUS_LABELS[s]}
              </button>
            );
          })}
          <div className="my-1 border-t border-border" />
          <button
            onClick={() => handleSetStatus(menu.dir, null)}
            className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] text-ink-3 transition hover:bg-surface-2"
          >
            <span className="w-3" />
            Clear (auto)
          </button>
        </div>
      )}

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
  onContextMenu?: (e: React.MouseEvent) => void;
  icon: React.ReactNode;
  label: string;
  sublabel?: string;
  count?: number;
  countStyle?: "default" | "muted";
  pinned?: boolean;
  dimmed?: boolean;
  onPin?: () => void;
  onHide?: () => void;
}

function ProjectItem({
  active,
  onClick,
  onContextMenu,
  icon,
  label,
  sublabel,
  count,
  countStyle = "default",
  pinned,
  dimmed,
  onPin,
  onHide,
}: ProjectItemProps) {
  const hasActions = Boolean(onPin || onHide);
  return (
    <div
      onClick={onClick}
      onContextMenu={onContextMenu}
      className={`group relative flex cursor-pointer items-center gap-2 rounded-sm px-3 py-1.5 text-[13px] transition ${
        active
          ? "bg-accent-soft text-accent-strong"
          : dimmed
            ? "text-ink-4 hover:bg-surface-3 hover:text-ink-3"
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
