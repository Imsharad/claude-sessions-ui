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
  // Recap-led: the recap (or title fallback) is the headline — the hero.
  // Title becomes a quieter subtitle only when a recap leads.
  const hasRecap = Boolean(s.recap);
  const headline = hasRecap ? truncate(s.recap!, 180) : truncate(s.title || "(untitled)", 120);
  return (
    <motion.div
      layout
      onClick={onClick}
      whileHover={{ y: -1 }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className={`group mb-2 cursor-pointer rounded-lg border p-3.5 transition ${
        selected
          ? "border-accent/40 bg-accent-soft/70 shadow-sm"
          : "border-border bg-surface hover:border-border-strong hover:shadow-sm"
      }`}
    >
      {/* Hero: the recap (or title fallback). Sharper weight when recap-led. */}
      <div className="flex items-start gap-2">
        {hasRecap && (
          <Sparkles
            size={13}
            className="mt-[3px] shrink-0 text-accent"
            aria-label="has recap"
          />
        )}
        <p
          className={`flex-1 leading-snug ${
            hasRecap
              ? "text-[13.5px] font-medium text-ink"           // recap = hero, darker + slightly larger
              : "text-[13px] text-ink-2"                        // title fallback = quieter
          }`}
        >
          {headline}
        </p>
        <ChevronRight
          size={15}
          className={`mt-[3px] shrink-0 transition ${
            selected ? "text-accent" : "text-ink-4 group-hover:text-ink-3"
          }`}
        />
      </div>

      {/* Subtitle: the session title — only when a recap leads, and quieter. */}
      {hasRecap && s.title && s.title !== "(untitled session)" && (
        <div className="mt-0.5 truncate pl-[21px] text-[11.5px] text-ink-3">
          {s.title}
        </div>
      )}

      {/* Meta row — grouped: (project · branch) | (time · msgs · dur) | (cost · tokens)
          Separated by middle-dots so the eye groups, not runs-on. */}
      <div className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 pl-[21px] text-[11px] text-ink-3">
        <Group>
          <span className="font-medium text-ink-2">{s.displayProject}</span>
          {s.gitBranch && (
            <span className="inline-flex items-center gap-0.5 font-mono text-ink-3">
              <GitBranch size={10} />
              {s.gitBranch}
            </span>
          )}
        </Group>
        <Sep />
        <Group>
          <span className="inline-flex items-center gap-0.5">
            <Clock size={10} />
            {relativeTime(s.lastTs)}
          </span>
          <span className="inline-flex items-center gap-0.5 tabular-nums">
            <MessageSquare size={10} />
            {s.messageCount}
          </span>
          {s.durationMs > 0 && <span className="tabular-nums">{formatDuration(s.durationMs)}</span>}
        </Group>
        {(s.costUsd > 0.05 || s.inputToks > 0) && <Sep />}
        <Group className="ml-auto">
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
        </Group>
      </div>

      {/* Plan-mode badge */}
      {s.planMode && (
        <div className="mt-1.5 pl-[21px]">
          <span className="rounded-full bg-accent-soft px-1.5 py-0.5 text-[10px] font-medium text-accent-strong">
            plan mode
          </span>
        </div>
      )}
    </motion.div>
  );
}

/** Visually group a set of meta items (no separator between them). */
function Group({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <span className={`inline-flex items-center gap-1.5 ${className}`}>{children}</span>;
}

/** Middle-dot separator between groups.
 *  The horizontal padding is the load-bearing part: it widens the inter-group
 *  gap (~3× the intra-group gap) so the eye reads distinct clusters, not a
 *  run-on line. The dot itself is just a faint marker. */
function Sep() {
  return <span className="px-1.5 text-ink-4">·</span>;
}
