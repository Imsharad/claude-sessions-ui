/**
 * Center pane: the session list.
 * Cards are RECAP-LED and progressive-disclosure:
 *   Tier 1 (always) — recap headline, short project, area chip, completion badge, time.
 *   Tier 2 (expand in place) — fuller recap + tag fields, via a hover chevron.
 *   Tier 3 (click) — SessionDetail (unchanged).
 * Virtualized via @tanstack/react-virtual with measureElement, so expanded rows
 * grow without fixed heights and 688 items still scroll at 60fps. Expansion
 * state lives HERE (a Set of ids), outside the virtualized row, so it survives
 * the row unmount/remount that windowing does.
 */
import { useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { motion } from "framer-motion";
import {
  GitBranch,
  Clock,
  MessageSquare,
  Sparkles,
  ChevronRight,
  ChevronDown,
} from "lucide-react";
import type { SessionCard } from "../lib/ipc";
import {
  relativeTime,
  formatTokens,
  formatCost,
  formatDuration,
  truncate,
} from "../lib/format";

const ROW_HEIGHT = 116; // px — tuned for recap + 2 meta lines (collapsed estimate)
const GAP = 8;

interface SessionListProps {
  sessions: SessionCard[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function SessionList({ sessions, selectedId, onSelect }: SessionListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  // Expansion lives outside the virtualized row so it survives windowing
  // unmount/remount. A Set keeps toggles O(1) and lets many rows stay open.
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const toggleExpanded = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const virtualizer = useVirtualizer({
    count: sessions.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT + GAP,
    overscan: 6,
  });

  if (sessions.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-8 text-center">
        <div>
          <p className="text-ink-3">No sessions match.</p>
          <p className="mt-1 text-[12px] text-ink-4">
            Try clearing the search or selecting a different project.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-1 flex-col bg-canvas">
      {/* Header strip — count + scroll hint */}
      <div className="flex items-center justify-between border-b border-border px-5 py-3">
        <h2 className="text-[13px] font-semibold text-ink-2">
          {sessions.length} session{sessions.length === 1 ? "" : "s"}
        </h2>
      </div>

      {/* Virtualized list */}
      <div ref={parentRef} className="flex-1 overflow-y-auto px-4 py-3">
        <div
          style={{
            height: virtualizer.getTotalSize(),
            position: "relative",
            width: "100%",
          }}
        >
          {virtualizer.getVirtualItems().map((vItem) => {
            const s = sessions[vItem.index];
            if (!s) return null;
            const isSelected = s.id === selectedId;
            return (
              <div
                key={s.id}
                data-index={vItem.index}
                ref={virtualizer.measureElement}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${vItem.start}px)`,
                }}
              >
                <SessionCardView
                  session={s}
                  selected={isSelected}
                  expanded={expanded.has(s.id)}
                  onClick={() => onSelect(s.id)}
                  onToggleExpand={() => toggleExpanded(s.id)}
                />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

interface CardProps {
  session: SessionCard;
  selected: boolean;
  expanded: boolean;
  onClick: () => void;
  onToggleExpand: () => void;
}

/** Small quiet completion badge — garnish, never louder than the recap.
 *  Absent completion renders nothing (cross-feature data rule). */
function CompletionBadge({ session: s }: { session: SessionCard }) {
  const done = s.completionPct != null ? s.completionPct >= 100 : s.goalCompleted === true;
  const show = s.completionPct != null || s.goalCompleted === true;
  if (!show) return null;
  const label = s.completionPct != null ? `${s.completionPct}%` : "done";
  return (
    <span
      className={`rounded-full px-1.5 py-0.5 text-[10px] tabular-nums ${
        done ? "bg-positive-soft text-positive" : "bg-surface-3 text-ink-2"
      }`}
    >
      {label}
    </span>
  );
}

/** Tiny area-of-life chip. Absent renders nothing. */
function AreaChip({ area }: { area: string | null }) {
  if (!area) return null;
  return (
    <span className="rounded-full bg-surface-3 px-1.5 py-0.5 text-[10px] text-ink-2">
      {area}
    </span>
  );
}

function SessionCardView({
  session: s,
  selected,
  expanded,
  onClick,
  onToggleExpand,
}: CardProps) {
  // Recap-led: the recap (or title fallback) is the headline — the hero.
  const hasRecap = Boolean(s.recap);
  const headline = hasRecap
    ? expanded
      ? s.recap!
      : truncate(s.recap!, 180)
    : truncate(s.title || "(untitled)", 120);
  const shortProject = s.projectShortName ?? s.displayProject;

  return (
    <motion.div
      layout
      onClick={onClick}
      whileHover={{ y: -1 }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className={`group mb-2 cursor-pointer rounded-lg border p-3.5 transition ${selected ? "border-accent/40 bg-accent-soft/70 shadow-sm" : "border-border bg-surface hover:border-border-strong hover:shadow-sm"}`}
    >
      <div className="flex items-start gap-2">
        {hasRecap && <Sparkles size={13} className="mt-[3px] shrink-0 text-accent" />}
        <p
          className={`flex-1 leading-snug ${hasRecap ? "text-[13.5px] font-medium text-ink" : "text-[13px] text-ink-2"} ${expanded && hasRecap ? "line-clamp-6" : ""}`}
        >
          {headline}
        </p>
        {/* Expand affordance — quiet, appears on hover like the pin pattern.
            stopPropagation so it toggles the tier without selecting the row. */}
        <button
          type="button"
          title={expanded ? "Collapse" : "Expand"}
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand();
          }}
          className={`mt-[1px] shrink-0 rounded p-0.5 text-ink-4 transition hover:text-ink-2 group-hover:opacity-100 ${expanded ? "opacity-100" : "opacity-0"}`}
        >
          <ChevronDown
            size={14}
            className={`transition-transform ${expanded ? "rotate-180" : ""}`}
          />
        </button>
        <ChevronRight
          size={15}
          className={`mt-[3px] shrink-0 transition ${selected ? "text-accent" : "text-ink-4 group-hover:text-ink-3"}`}
        />
      </div>

      {hasRecap && s.title && s.title !== "(untitled session)" && (
        <div className="mt-0.5 truncate pl-[21px] text-[11.5px] text-ink-3">{s.title}</div>
      )}

      <div className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 pl-[21px] text-[11px] text-ink-3">
        <span className="inline-flex items-center gap-1.5">
          <span className="font-medium text-ink-2">{shortProject}</span>
          {s.gitBranch && (
            <span className="inline-flex items-center gap-0.5 font-mono text-ink-3">
              <GitBranch size={10} />
              {s.gitBranch}
            </span>
          )}
          <AreaChip area={s.areaOfLife} />
          <CompletionBadge session={s} />
        </span>
        <span className="px-1.5 text-ink-4">·</span>
        <span className="inline-flex items-center gap-1.5">
          <span className="inline-flex items-center gap-0.5">
            <Clock size={10} />
            {relativeTime(s.lastTs)}
          </span>
          <span className="inline-flex items-center gap-0.5 tabular-nums">
            <MessageSquare size={10} />
            {s.messageCount}
          </span>
          {s.durationMs > 0 && (
            <span className="tabular-nums">{formatDuration(s.durationMs)}</span>
          )}
        </span>
        {(s.costUsd > 0.05 || s.inputToks > 0) && <span className="px-1.5 text-ink-4">·</span>}
        <span className="ml-auto inline-flex items-center gap-1.5">
          {s.costUsd > 0.05 && (
            <span className="rounded-full bg-surface-3 px-1.5 py-0.5 font-mono text-[10px] tabular-nums text-ink-2">
              {formatCost(s.costUsd)}
            </span>
          )}
          {s.inputToks > 0 && (
            <span className="font-mono text-[10px] tabular-nums text-ink-4">
              {formatTokens(s.inputToks + s.outputToks)}
            </span>
          )}
        </span>
      </div>

      {s.planMode && (
        <div className="mt-1.5 pl-[21px]">
          <span className="rounded-full bg-accent-soft px-1.5 py-0.5 text-[10px] font-medium text-accent-strong">
            plan mode
          </span>
        </div>
      )}

      {expanded && (
        <div
          data-card-detail
          className="mt-2.5 space-y-2 border-t border-border pl-[21px] pt-2.5 text-[11.5px] text-ink-2"
        >
          {/* Duration + message count, spelled out (Tier 2 detail). */}
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-ink-3">
            <span className="inline-flex items-center gap-1 tabular-nums">
              <MessageSquare size={11} />
              {s.messageCount} message{s.messageCount === 1 ? "" : "s"}
            </span>
            {s.durationMs > 0 && (
              <span className="inline-flex items-center gap-1 tabular-nums">
                <Clock size={11} />
                {formatDuration(s.durationMs)}
              </span>
            )}
          </div>

          {/* Tag fields present — each renders only when set. */}
          {(s.projectShortName || s.areaOfLife || s.completionPct != null || s.goalCompleted === true) && (
            <div className="flex flex-wrap items-center gap-1.5">
              {s.projectShortName && (
                <span className="font-medium text-ink">{s.projectShortName}</span>
              )}
              <AreaChip area={s.areaOfLife} />
              <CompletionBadge session={s} />
            </div>
          )}

          {s.tagRationale && (
            <p className="leading-snug text-ink-2">{s.tagRationale}</p>
          )}
        </div>
      )}
    </motion.div>
  );
}
