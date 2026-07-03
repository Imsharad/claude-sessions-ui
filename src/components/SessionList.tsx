/**
 * Center pane: the session list.
 * Cards are RECAP-LED — the recap (or first-prompt fallback) is the primary
 * text, with title/project/time as secondary metadata. Virtualized via
 * @tanstack/react-virtual so 688 items scroll at 60fps.
 */
import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { motion } from "framer-motion";
import { GitBranch, Clock, MessageSquare, Sparkles, ChevronRight } from "lucide-react";
import type { SessionCard } from "../lib/ipc";
import {
  relativeTime,
  formatTokens,
  formatCost,
  formatDuration,
  truncate,
} from "../lib/format";

const ROW_HEIGHT = 116; // px — tuned for recap + 2 meta lines
const GAP = 8;

interface SessionListProps {
  sessions: SessionCard[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function SessionList({ sessions, selectedId, onSelect }: SessionListProps) {
  const parentRef = useRef<HTMLDivElement>(null);

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
                  onClick={() => onSelect(s.id)}
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
  onClick: () => void;
}

function SessionCardView({ session: s, selected, onClick }: CardProps) {
  // Recap-led: the recap (or first prompt) is the headline.
  const headline = s.recap ? truncate(s.recap, 180) : truncate(s.title || "(untitled)", 120);
  return (
    <motion.div
      layout
      onClick={onClick}
      whileHover={{ y: -1 }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className={`group mb-2 cursor-pointer rounded-xl border p-3.5 transition ${
        selected
          ? "border-accent/40 bg-accent-soft/60 shadow-sm"
          : "border-border bg-surface hover:border-border-strong hover:shadow-sm"
      }`}
    >
      {/* Headline: recap or title */}
      <div className="flex items-start gap-2">
        {s.recap && (
          <Sparkles
            size={13}
            className="mt-0.5 shrink-0 text-accent"
            aria-label="has recap"
          />
        )}
        <p
          className={`flex-1 text-[13px] leading-snug ${
            s.recap ? "text-ink" : "text-ink-2 italic"
          }`}
        >
          {headline}
        </p>
        <ChevronRight
          size={15}
          className={`mt-0.5 shrink-0 transition ${
            selected ? "text-accent" : "text-ink-4 group-hover:text-ink-3"
          }`}
        />
      </div>

      {/* Title (secondary, only if recap is the headline) */}
      {s.recap && s.title && (
        <div className="mt-1 truncate pl-[21px] text-[12px] font-medium text-ink-2">
          {s.title}
        </div>
      )}

      {/* Meta row */}
      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 pl-[21px] text-[11px] text-ink-3">
        <span className="font-medium text-ink-2">{s.displayProject}</span>
        {s.gitBranch && (
          <span className="inline-flex items-center gap-1 font-mono">
            <GitBranch size={10} />
            {s.gitBranch}
          </span>
        )}
        <span className="inline-flex items-center gap-1">
          <Clock size={10} />
          {relativeTime(s.lastTs)}
        </span>
        <span className="inline-flex items-center gap-1 tabular-nums">
          <MessageSquare size={10} />
          {s.messageCount}
        </span>
        {s.durationMs > 0 && (
          <span className="tabular-nums">{formatDuration(s.durationMs)}</span>
        )}
        {s.costUsd > 0.05 && (
          <span className="ml-auto rounded-full bg-surface-3 px-1.5 py-0.5 font-mono text-[10px] tabular-nums text-ink-2">
            {formatCost(s.costUsd)}
          </span>
        )}
        {s.inputToks > 0 && (
          <span className="font-mono text-[10px] tabular-nums text-ink-4">
            {formatTokens(s.inputToks + s.outputToks)} tok
          </span>
        )}
      </div>

      {/* Plan-mode + error badges */}
      {(s.planMode || false) && (
        <div className="mt-1.5 pl-[21px]">
          <span className="rounded-full bg-accent-soft px-1.5 py-0.5 text-[10px] font-medium text-accent-strong">
            plan mode
          </span>
        </div>
      )}
    </motion.div>
  );
}
