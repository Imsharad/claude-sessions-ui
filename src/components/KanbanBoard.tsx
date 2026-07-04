/**
 * Kanban board — the triage surface for TAGGED work.
 * Three columns (Planned / In Progress / Completed) reusing the exact Tier-1
 * card from the list (SessionCardView), so the board is the same design family,
 * not a second language. Membership + column come from completion, with an
 * explicit drag (kanban_status) overriding the %-derived column (override-wins).
 *
 * Column rule (mirrors derive_kanban_column in lib.rs):
 *   kanbanStatus (if set) wins; else pct 0 → Planned, 1..99 → In Progress,
 *   100 || goalCompleted → Completed; untagged (no pct, no goal) stays OFF.
 *
 * Drag: native HTML5 DnD (no new dep). Dropping computes a fractional order
 * (midpoint of neighbours, seeding 1000-gaps when they lack orders), persists
 * via setKanban, updates local state optimistically, then refreshes from App.
 */
import { Fragment, useEffect, useState } from "react";
import { Wand2 } from "lucide-react";
import { SessionCardView } from "./SessionList";
import { setKanban, type SessionCard, type KanbanStatus } from "../lib/ipc";

interface KanbanBoardProps {
  sessions: SessionCard[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  /** Refresh sessions from App after a drag persists (onSessionsChanged). */
  onSessionsChanged?: () => void;
}

const COLUMNS: { id: KanbanStatus; label: string; empty: string }[] = [
  { id: "planned", label: "Planned", empty: "Tagged sessions at 0% land here." },
  { id: "in_progress", label: "In Progress", empty: "Drag a card here, or tag a session that is underway." },
  { id: "completed", label: "Completed", empty: "Finished work collects here." },
];

/** The %-derived column, IGNORING any explicit override. Null = untagged. */
function pctColumn(s: SessionCard): KanbanStatus | null {
  if (s.completionPct == null && s.goalCompleted == null) return null;
  if (s.goalCompleted === true || (s.completionPct ?? 0) >= 100) return "completed";
  if ((s.completionPct ?? 0) === 0) return "planned";
  return "in_progress";
}

/** The column a card actually sits in: override-wins over the %-derived one. */
function columnOf(s: SessionCard): KanbanStatus | null {
  if (s.kanbanStatus === "planned" || s.kanbanStatus === "in_progress" || s.kanbanStatus === "completed") {
    return s.kanbanStatus;
  }
  return pctColumn(s);
}

/** Sort a column: explicit kanbanOrder first (nulls last), then lastTs desc. */
function sortColumn(cards: SessionCard[]): SessionCard[] {
  return [...cards].sort((a, b) => {
    const ao = a.kanbanOrder;
    const bo = b.kanbanOrder;
    if (ao != null && bo != null) return ao - bo;
    if (ao != null) return -1;
    if (bo != null) return 1;
    return (b.lastTs ?? "").localeCompare(a.lastTs ?? "");
  });
}

/** Fractional order for an insertion at `idx` within `cards` (dragged card
 *  already removed). Midpoint of neighbours; 1000-gaps seed missing orders. */
function orderForInsertion(cards: SessionCard[], idx: number): number {
  const eff = (i: number): number | null => {
    if (i < 0 || i >= cards.length) return null;
    const o = cards[i].kanbanOrder;
    return o == null ? (i + 1) * 1000 : o;
  };
  const prev = eff(idx - 1);
  const next = eff(idx);
  if (prev == null && next == null) return 1000;
  if (prev == null) return next! / 2;
  if (next == null) return prev + 1000;
  return (prev + next) / 2;
}

interface DropTarget {
  col: KanbanStatus;
  idx: number;
}

export function KanbanBoard({ sessions, selectedId, onSelect, onSessionsChanged }: KanbanBoardProps) {
  // Local mirror of props so a drop reflects instantly (optimistic); reconciled
  // when App refreshes sessions and new props flow in.
  const [items, setItems] = useState(sessions);
  useEffect(() => setItems(sessions), [sessions]);

  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);

  const onBoard = items.filter((s) => columnOf(s) !== null);

  const handleDrop = (col: KanbanStatus) => {
    if (!draggingId || !dropTarget) return;
    const id = draggingId;
    const idx = dropTarget.idx;
    // Sorted column with the dragged card removed — the target neighbourhood.
    const colCards = sortColumn(onBoard.filter((s) => columnOf(s) === col && s.id !== id));
    const order = orderForInsertion(colCards, idx);

    setItems((prev) =>
      prev.map((s) => (s.id === id ? { ...s, kanbanStatus: col, kanbanOrder: order } : s)),
    );
    setDraggingId(null);
    setDropTarget(null);
    setKanban(id, col, order)
      .then(() => onSessionsChanged?.())
      .catch((e) => console.error("setKanban failed", e));
  };

  // Nothing tagged at all → one warm, board-level invitation, not three echoes.
  if (onBoard.length === 0) {
    return (
      <div className="flex h-full flex-1 flex-col bg-canvas">
        <div className="flex flex-1 items-center justify-center p-8 text-center">
          <div className="max-w-sm">
            <div className="mx-auto mb-3 flex h-9 w-9 items-center justify-center rounded-full bg-surface-2 text-ink-3">
              <Wand2 size={16} />
            </div>
            <p className="text-[13px] font-medium text-ink-2">The board is for tagged work</p>
            <p className="mt-1 text-[12px] leading-relaxed text-ink-4">
              Tag a session with the wand and it lands in a column by how done it is.
              Drag between columns to triage — a re-tag never moves what you placed.
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-1 gap-3 overflow-x-auto bg-canvas p-4">
      {COLUMNS.map((c) => {
        const colCards = sortColumn(onBoard.filter((s) => columnOf(s) === c.id));
        const isDropCol = dropTarget?.col === c.id;

        const indicator = (i: number) =>
          isDropCol && dropTarget?.idx === i ? (
            <div className="mx-1 my-0.5 h-0.5 rounded-full bg-accent" />
          ) : null;

        return (
          <div
            key={c.id}
            className="flex min-w-[260px] flex-1 flex-col rounded-lg bg-surface-2/50"
            onDragOver={(e) => {
              if (!draggingId) return;
              e.preventDefault();
              const body = e.currentTarget.querySelector("[data-col-body]");
              const idx = body
                ? computeInsertIndex(body as HTMLElement, e.clientY)
                : colCards.length;
              setDropTarget({ col: c.id, idx });
            }}
            onDrop={(e) => {
              e.preventDefault();
              handleDrop(c.id);
            }}
          >
            {/* Calm header — uppercase micro-label + muted count, no colour. */}
            <div className="flex items-center justify-between px-3 py-2.5">
              <span className="text-[11px] font-medium uppercase tracking-wide text-ink-3">
                {c.label}
              </span>
              <span className="text-[11px] tabular-nums text-ink-4">{colCards.length}</span>
            </div>

            <div data-col-body className="flex-1 overflow-y-auto px-2 pb-2">
              {colCards.length === 0 ? (
                <div className="flex h-full min-h-[80px] items-center justify-center px-4 py-8 text-center">
                  <p className="text-[11.5px] leading-relaxed text-ink-4">{c.empty}</p>
                </div>
              ) : (
                <>
                  {colCards.map((s, i) => {
                    // "% changed" hint: an explicitly-placed card whose %-derived
                    // column now disagrees — the auto-tag moved the % but the card
                    // stays put (override-wins). Quiet signal, never a move.
                    const drifted =
                      s.kanbanStatus != null &&
                      pctColumn(s) !== null &&
                      pctColumn(s) !== s.kanbanStatus;
                    const hint =
                      drifted && s.completionPct != null ? (
                        <span
                          className="text-[10px] text-ink-4"
                          title="The auto-tag moved this session's completion %, but the card stays where you placed it."
                        >
                          now {s.completionPct}%
                        </span>
                      ) : null;

                    return (
                      <Fragment key={s.id}>
                        {indicator(i)}
                        <div
                          data-board-card
                          draggable
                          onDragStart={(e) => {
                            e.dataTransfer.effectAllowed = "move";
                            setDraggingId(s.id);
                          }}
                          onDragEnd={() => {
                            setDraggingId(null);
                            setDropTarget(null);
                          }}
                          className={draggingId === s.id ? "opacity-50" : ""}
                        >
                          <SessionCardView
                            session={s}
                            selected={s.id === selectedId}
                            expanded={false}
                            onClick={() => onSelect(s.id)}
                            onToggleExpand={() => {}}
                            onTagged={onSessionsChanged}
                            hideExpand
                            footer={hint}
                          />
                        </div>
                      </Fragment>
                    );
                  })}
                  {indicator(colCards.length)}
                </>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** Insertion index from pointer Y over a column body's cards: the first card
 *  whose vertical midpoint is below the cursor, else the end. */
function computeInsertIndex(body: HTMLElement, y: number): number {
  const cards = Array.from(body.querySelectorAll("[data-board-card]"));
  for (let i = 0; i < cards.length; i++) {
    const r = cards[i].getBoundingClientRect();
    if (y < r.top + r.height / 2) return i;
  }
  return cards.length;
}
