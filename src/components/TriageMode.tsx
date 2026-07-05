/**
 * Keyboard triage — the primary, zero-latency tagging path. Walks the untagged
 * sessions one at a time; every field is a keystroke, nothing hits the network
 * except the synchronous update_session_tags write on save.
 *
 * Keys: b/r/c/o/p → area · 0-9 → completion (digit×10) · f → 100 · +/- → ±5 ·
 *       y → toggle goal done · Enter or → → save + next · s → skip · ← → prev ·
 *       Esc → exit.
 *
 * The queue is frozen at mount so saves don't reshuffle it under the cursor; the
 * parent is refreshed once on exit (not per keystroke).
 */
import { useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { Check, SkipForward, CornerDownLeft } from "lucide-react";
import { AREAS_OF_LIFE, updateSessionTags, type SessionCard } from "../lib/ipc";

// First letters of the vocabulary are unique → one-key area selection.
const AREA_KEYS: Record<string, string> = {
  b: "Building",
  r: "Research",
  c: "Content",
  o: "Ops",
  p: "Personal",
};

/** Same "has any tag" predicate the cards use. Untagged = none of these set. */
function isUntagged(s: SessionCard): boolean {
  return !(
    s.taggedAt ||
    s.areaOfLife ||
    s.projectShortName ||
    s.tagRationale ||
    s.completionPct != null ||
    s.goalCompleted != null
  );
}

interface Pending {
  area: string | null;
  pct: number;
  goal: boolean;
}

export function TriageMode({
  sessions,
  onExit,
  onTagged,
}: {
  sessions: SessionCard[];
  onExit: () => void;
  onTagged: () => void;
}) {
  // ponytail: snapshot the queue at mount, not a live filter — a save marks a row
  // tagged, which would otherwise drop it and jump the cursor.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const queue = useMemo(() => sessions.filter(isUntagged), []);
  const [i, setI] = useState(0);
  const [edits, setEdits] = useState<Record<string, Pending>>({});
  const [saved, setSaved] = useState<Set<string>>(new Set());
  const [err, setErr] = useState<string | null>(null);

  const current: SessionCard | undefined = queue[i];
  const pending: Pending = current
    ? edits[current.id] ?? {
        area: current.areaOfLife ?? null,
        pct: current.completionPct ?? 0,
        goal: current.goalCompleted ?? false,
      }
    : { area: null, pct: 0, goal: false };

  const setPending = (patch: Partial<Pending>) => {
    if (!current) return;
    setEdits((e) => ({ ...e, [current.id]: { ...pending, ...patch } }));
  };

  const exit = () => {
    if (saved.size > 0) onTagged();
    onExit();
  };
  const next = () => setI((n) => Math.min(n + 1, queue.length));
  const prev = () => setI((n) => Math.max(n - 1, 0));

  const save = async () => {
    if (!current) return;
    if (!pending.area) {
      setErr("Pick an area first (b/r/c/o/p)");
      return;
    }
    try {
      await updateSessionTags(current.id, {
        areaOfLife: pending.area,
        completionPct: pending.pct,
        goalCompleted: pending.goal,
      });
      setSaved((s) => new Set(s).add(current.id));
      setErr(null);
      next();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "Save failed");
    }
  };

  // One window-level handler, re-registered each render so it closes over fresh
  // pending/current/save. ponytail: cheaper than threading a ref for a modal view.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
      if (e.key === "Escape") return exit();
      if (!current) return; // done screen: Esc only
      const k = e.key.toLowerCase();
      if (AREA_KEYS[k]) {
        e.preventDefault();
        setPending({ area: AREA_KEYS[k] });
      } else if (k === "y") {
        e.preventDefault();
        setPending({ goal: !pending.goal });
      } else if (k === "f") {
        e.preventDefault();
        setPending({ pct: 100 });
      } else if (/^[0-9]$/.test(k)) {
        e.preventDefault();
        setPending({ pct: Number(k) * 10 });
      } else if (e.key === "+" || e.key === "=") {
        e.preventDefault();
        setPending({ pct: Math.min(100, pending.pct + 5) });
      } else if (e.key === "-") {
        e.preventDefault();
        setPending({ pct: Math.max(0, pending.pct - 5) });
      } else if (e.key === "Enter" || e.key === "ArrowRight") {
        e.preventDefault();
        void save();
      } else if (k === "s") {
        e.preventDefault();
        next();
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        prev();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (queue.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 bg-canvas">
        <Check size={28} className="text-accent" />
        <p className="text-[14px] font-medium text-ink-2">Everything in view is tagged.</p>
        <button onClick={exit} className="text-[12px] font-medium text-accent hover:text-accent-strong">
          Back to list
        </button>
      </div>
    );
  }

  if (!current) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 bg-canvas">
        <Check size={28} className="text-accent" />
        <p className="text-[14px] font-medium text-ink-2">
          Tagged {saved.size} of {queue.length}.
        </p>
        <button onClick={exit} className="text-[12px] font-medium text-accent hover:text-accent-strong">
          Done — back to list
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col bg-canvas">
      {/* Progress */}
      <div className="flex items-center justify-between border-b border-border px-6 py-2.5 text-[11px] text-ink-3">
        <span className="tabular-nums">
          {i + 1} / {queue.length} · <span className="text-accent">{saved.size} tagged</span>
        </span>
        <button onClick={exit} className="text-ink-3 transition hover:text-ink-2">
          Esc to exit
        </button>
      </div>

      {/* Card */}
      <div className="mx-auto flex w-full max-w-2xl flex-1 flex-col justify-center gap-6 px-6">
        <motion.div
          key={current.id}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ type: "spring", stiffness: 400, damping: 32 }}
          className="space-y-5"
        >
          <div className="space-y-1.5">
            <p className="text-[15px] font-medium leading-snug text-ink">
              {current.recap || current.title || "(untitled)"}
            </p>
            <p className="text-[11.5px] text-ink-4">
              {current.projectShortName ?? current.displayProject}
              {" · "}
              {current.messageCount} msgs
            </p>
          </div>

          {/* Area */}
          <div className="space-y-1.5">
            <FieldRow label="Area" />
            <div className="flex flex-wrap gap-1.5">
              {AREAS_OF_LIFE.map((a) => {
                const key = a[0].toLowerCase();
                const on = pending.area === a;
                return (
                  <button
                    key={a}
                    onClick={() => setPending({ area: a })}
                    className={`rounded-md border px-2.5 py-1.5 text-[12px] font-medium transition ${
                      on
                        ? "border-accent/50 bg-accent-soft text-accent-strong"
                        : "border-border bg-surface text-ink-3 hover:border-border-strong"
                    }`}
                  >
                    <span className="mr-1 inline-block w-3 text-center font-mono text-[10px] uppercase text-ink-4">
                      {key}
                    </span>
                    {a}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Completion + goal */}
          <div className="flex flex-wrap items-end gap-8">
            <div className="space-y-1.5">
              <FieldRow label="Completion" hint="0-9 = ×10 · f = 100 · +/-" />
              <div className="flex items-baseline gap-1">
                <span className="text-[24px] font-semibold tabular-nums text-ink">{pending.pct}</span>
                <span className="text-[13px] text-ink-4">%</span>
              </div>
            </div>
            <div className="space-y-1.5">
              <FieldRow label="Goal done" hint="y" />
              <button
                onClick={() => setPending({ goal: !pending.goal })}
                className={`rounded-md border px-3 py-1.5 text-[12px] font-medium transition ${
                  pending.goal
                    ? "border-accent/50 bg-accent-soft text-accent-strong"
                    : "border-border bg-surface text-ink-3 hover:border-border-strong"
                }`}
              >
                {pending.goal ? "Yes" : "No"}
              </button>
            </div>
          </div>

          {err && <p className="text-[11.5px] text-danger">{err}</p>}
        </motion.div>
      </div>

      {/* Footer legend */}
      <div className="flex items-center justify-center gap-5 border-t border-border px-6 py-2.5 text-[11px] text-ink-4">
        <Legend icon={<CornerDownLeft size={11} />} label="save + next" />
        <Legend icon={<SkipForward size={11} />} label="s · skip" />
        <span className="tabular-nums">← prev</span>
      </div>
    </div>
  );
}

function FieldRow({ label, hint }: { label: string; hint?: string }) {
  return (
    <div className="flex items-center gap-2">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-3">{label}</span>
      {hint && <span className="font-mono text-[10px] text-ink-4">{hint}</span>}
    </div>
  );
}

function Legend({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      {icon}
      {label}
    </span>
  );
}
