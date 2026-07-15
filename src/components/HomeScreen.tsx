/**
 * The home screen — the app's front door (first-screen-spec.md §1, §3, §4).
 * Answers one question: "what was I working on recently, and where do I pick it
 * back up?" Top to bottom, space and detail decay with rank:
 *   Slot 0  — orientation line (one quiet line, no chrome)
 *   Slot 1  — HERO thread (~45% viewport): recap, left-off todos, why, Resume
 *   Slots 2–3 — secondary rows (one recap line, why, compact Resume)
 *   Slots 4–5 — tertiary lines (name, time, recap fragment)
 *   Slot 6  — the exit row (Browse all + search), visually subordinate
 * A dumb component: `data` is fetched by App (on launch / after reindex only —
 * ranking stability between opens is part of the spec, so no focus refetch).
 * The why-sentence and open todos arrive pre-templated from the backend and are
 * rendered verbatim — no free text assembled here.
 */
import { useState, useEffect, useRef } from "react";
import { motion } from "framer-motion";
import { GitBranch, Clock, Square, Play, Search } from "lucide-react";
import type { HomeData, HomeThread } from "../lib/ipc";
import { resumeSession } from "../lib/ipc";
import { relativeTime, truncate } from "../lib/format";
import { format } from "date-fns";

interface Props {
  data: HomeData;
  onBrowse: (query?: string, projectHint?: string) => void;
}

/** Tiny area-of-life chip — same vocabulary as SessionList cards. Absent renders nothing. */
function AreaChip({ area }: { area: string | null }) {
  if (!area) return null;
  return (
    <span className="rounded-full bg-surface-3 px-1.5 py-0.5 text-[10px] text-ink-2">
      {area}
    </span>
  );
}

/** Quiet git-branch tag — GitBranch icon + mono name, matching the card meta line. */
function BranchTag({ branch }: { branch: string | null }) {
  if (!branch) return null;
  return (
    <span className="inline-flex items-center gap-0.5 font-mono text-[11px] text-ink-3">
      <GitBranch size={11} />
      {branch}
    </span>
  );
}

/** Resume — the one promoted action from the detail pane. Owns its own pending
 *  state so a slow resume can't be double-fired. `large` is the hero's primary
 *  target (the largest interactive element on screen); the compact variant sits
 *  on secondary rows. Fires resume_session and stays put — no navigation. */
function ResumeButton({
  sessionId,
  large = false,
}: {
  sessionId: string;
  large?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const go = async (e: React.MouseEvent, fork: boolean) => {
    e.stopPropagation();
    if (busy) return;
    setBusy(true);
    try {
      await resumeSession(sessionId, fork);
    } catch (err) {
      console.error(err);
    } finally {
      setBusy(false);
    }
  };

  if (large) {
    return (
      <div className="flex items-center gap-4">
        <button
          type="button"
          disabled={busy}
          onClick={(e) => go(e, false)}
          className="inline-flex items-center gap-2 rounded-lg bg-accent px-6 py-3 text-[15px] font-semibold text-white shadow-md transition hover:bg-accent-strong disabled:opacity-60"
        >
          <Play size={16} className="fill-white" />
          Resume
          <span className="ml-1 text-[11px] font-medium text-white/70">Ret</span>
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={(e) => go(e, true)}
          className="text-[12px] text-ink-3 transition hover:text-ink-2 disabled:opacity-60"
        >
          fork
        </button>
      </div>
    );
  }

  return (
    <button
      type="button"
      disabled={busy}
      onClick={(e) => go(e, false)}
      className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-border bg-surface px-3 py-1.5 text-[12px] font-medium text-ink-2 shadow-xs transition hover:border-border-strong hover:bg-surface-2 disabled:opacity-60"
    >
      <Play size={12} className="fill-ink-2 text-ink-2" />
      Resume
    </button>
  );
}

/** Slot 1 — the HERO. The single most likely "resume this" answer, shown in full:
 *  name + area + branch, digest-first body (worked_on/outcome), open loops,
 *  mechanical todos, why, and the largest Resume target. In stale mode the todo
 *  block is prefixed "When you left:" when no loops are present. */
function Hero({ thread, stale }: { thread: HomeThread; stale: boolean }) {
  const openLoops = (thread.openLoops ?? []).slice(0, 3);
  const openTodos = thread.openTodos ?? [];

  let body = thread.latestRecap || thread.latestTitle;
  if (thread.workedOn || thread.outcome) {
    const parts: string[] = [];
    if (thread.workedOn) parts.push(thread.workedOn);
    if (thread.outcome) parts.push(thread.outcome);
    body = `Left off: ${parts.join(" ")}`;
  }

  return (
    <motion.section
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className="flex min-h-[45vh] flex-col rounded-xl border border-border bg-surface p-6 shadow-sm"
    >
      {/* Name + chip + branch + relative time */}
      <div className="flex items-start justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-[19px] font-semibold text-ink">{thread.displayName}</h2>
          <AreaChip area={thread.areaOfLife} />
          <BranchTag branch={thread.gitBranch} />
        </div>
        <span className="inline-flex shrink-0 items-center gap-1 text-[12px] text-ink-3">
          <Clock size={12} />
          {relativeTime(thread.lastTs)}
        </span>
      </div>

      {/* Body — digest worked_on/outcome first, else recap, else title */}
      <p className="mt-4 max-w-[68ch] text-[14px] leading-relaxed text-ink-2 line-clamp-4">
        {body}
      </p>

      {/* Open loops — stated unfinished intent from digests (max 3) */}
      {openLoops.length > 0 && (
        <div className="mt-5">
          <div className="text-[11px] font-medium uppercase tracking-wide text-ink-3">
            Open loops
          </div>
          <ul className="mt-2 space-y-1.5">
            {openLoops.map((loop, i) => (
              <li key={i} className="flex items-start gap-2 text-[13px] text-ink-2">
                <span className="mt-[1px] shrink-0 text-ink-3">-</span>
                <span className="leading-snug">{loop}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Mechanical todos — quieter label when open loops already shown */}
      {openTodos.length > 0 && (
        <div className={openLoops.length > 0 ? "mt-4" : "mt-5"}>
          <div
            className={
              openLoops.length > 0
                ? "text-[10px] font-medium uppercase tracking-wide text-ink-3"
                : "text-[11px] font-medium uppercase tracking-wide text-ink-3"
            }
          >
            {openLoops.length > 0
              ? "Todos"
              : stale
                ? "When you left:"
                : "Left off"}
          </div>
          <ul className="mt-2 space-y-1.5">
            {openTodos.map((todo, i) => (
              <li key={i} className="flex items-start gap-2 text-[13px] text-ink-2">
                <Square size={14} className="mt-[3px] shrink-0 text-ink-3" />
                <span className="leading-snug">{todo}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Why — verbatim from the backend, one quiet line */}
      <p className="mt-5 text-[12px] text-ink-3">{thread.whySentence}</p>

      {/* Resume — the largest interactive target on screen */}
      <div className="mt-auto pt-6">
        <ResumeButton sessionId={thread.latestSessionId} large />
      </div>
    </motion.section>
  );
}

/** Slots 2–3 — secondary rows. One recap line (truncated ~120), the why, a
 *  relative time, and a compact Resume. The whole row is a browse target (opens
 *  the thread's project in browse); only the explicit Resume button resumes. */
function SecondaryRow({
  thread,
  onOpen,
}: {
  thread: HomeThread;
  onOpen: () => void;
}) {
  const recap = thread.latestRecap || thread.latestTitle;
  return (
    <div
      onClick={onOpen}
      className="group flex cursor-pointer items-center gap-4 rounded-lg border border-border bg-surface px-4 py-3 transition hover:border-border-strong hover:shadow-sm"
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[13.5px] font-medium text-ink">{thread.displayName}</span>
          <AreaChip area={thread.areaOfLife} />
          <BranchTag branch={thread.gitBranch} />
        </div>
        <p className="mt-1 truncate text-[12.5px] text-ink-2">{truncate(recap, 120)}</p>
        <div className="mt-1 flex items-center gap-2 text-[11px] text-ink-3">
          <span>{thread.whySentence}</span>
          <span className="text-ink-4">·</span>
          <span className="inline-flex items-center gap-1">
            <Clock size={10} />
            {relativeTime(thread.lastTs)}
          </span>
        </div>
      </div>
      <ResumeButton sessionId={thread.latestSessionId} />
    </div>
  );
}

/** Slots 4–5 — tertiary lines. Name, relative time, a 60-char recap fragment.
 *  No why, no button. The whole line is a browse target (resume is one more
 *  click inside the thread). Peripheral-vision recall, ignorable otherwise. */
function TertiaryRow({
  thread,
  onOpen,
}: {
  thread: HomeThread;
  onOpen: () => void;
}) {
  const recap = thread.latestRecap || thread.latestTitle;
  return (
    <div
      onClick={onOpen}
      className="group flex cursor-pointer items-center gap-3 rounded-md px-4 py-2 text-[12px] transition hover:bg-surface-2"
    >
      <span className="shrink-0 font-medium text-ink-2">{thread.displayName}</span>
      <span className="truncate text-ink-3">{truncate(recap, 60)}</span>
      <span className="ml-auto shrink-0 text-ink-4">{relativeTime(thread.lastTs)}</span>
    </div>
  );
}

/** Slot 6 — the exit. "Browse all N sessions" + search. Visible but subordinate
 *  (ink-3, no border weight). Enter or the label switches to the browse view,
 *  carrying the query. */
function ExitRow({
  total,
  onBrowse,
}: {
  total: number;
  onBrowse: (query?: string) => void;
}) {
  const [q, setQ] = useState("");
  return (
    <div className="flex items-center justify-between gap-4 border-t border-border/60 pt-4">
      <button
        type="button"
        onClick={() => onBrowse(q || undefined)}
        className="text-[12px] text-ink-3 transition hover:text-ink-2"
      >
        Browse all {total} sessions
      </button>
      <div className="inline-flex items-center gap-1.5 rounded-md bg-surface-2 px-2.5 py-1.5">
        <Search size={12} className="text-ink-4" />
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onBrowse(q || undefined);
          }}
          placeholder="Search sessions"
          className="w-40 bg-transparent text-[12px] text-ink-2 placeholder:text-ink-4 focus:outline-none"
        />
      </div>
    </div>
  );
}

export function HomeScreen({ data, onBrowse }: Props) {
  const { threads, totalSessions, activeThreadsThisWeek, stale, lastActivityTs } = data;
  const resumingRef = useRef(false);

  // Enter resumes hero; 2/3 resume secondary slots. Ignore typing surfaces.
  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      const active = document.activeElement as HTMLElement | null;
      if (
        active &&
        (active.tagName === "INPUT" ||
          active.tagName === "TEXTAREA" ||
          active.isContentEditable)
      ) {
        return;
      }

      let target: HomeThread | undefined;
      if (e.key === "Enter") target = threads[0];
      else if (e.key === "2") target = threads[1];
      else if (e.key === "3") target = threads[2];
      else return;

      if (!target || resumingRef.current) return;
      e.preventDefault();
      resumingRef.current = true;
      try {
        await resumeSession(target.latestSessionId, false);
      } catch (err) {
        console.error(err);
      } finally {
        resumingRef.current = false;
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [threads]);

  // Zero sessions: a single hero-sized card, no slots, no exit row.
  if (totalSessions === 0 || threads.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center bg-canvas p-8">
        <div className="max-w-md rounded-xl border border-border bg-surface p-8 text-center shadow-sm">
          <h2 className="text-[16px] font-semibold text-ink">No Claude Code sessions found yet.</h2>
          <p className="mt-2 text-[13px] leading-relaxed text-ink-3">
            Sessions appear here after you run <span className="font-mono text-ink-2">claude</span> in any project.
          </p>
        </div>
      </div>
    );
  }

  // Slot 0 — orientation. Stale mode swaps the resume anchor for a memory jog.
  const orientation = stale
    ? `You were last here ${lastActivityTs ? format(new Date(lastActivityTs), "MMMM d") : "recently"}.`
    : `${format(new Date(), "EEEE, MMMM d")} — ${activeThreadsThisWeek} thread${
        activeThreadsThisWeek === 1 ? "" : "s"
      } active this week.`;

  const [hero, ...rest] = threads;
  const secondary = rest.slice(0, 2); // slots 2–3
  const tertiary = rest.slice(2, 4); // slots 4–5

  return (
    <div className="min-h-0 flex-1 overflow-y-auto bg-canvas">
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.2 }}
        className="mx-auto flex max-w-3xl flex-col gap-4 px-6 py-6"
      >
        {/* Slot 0 */}
        <p className="text-[12px] text-ink-3">{orientation}</p>

        {/* Slot 1 */}
        <Hero thread={hero} stale={stale} />

        {/* Slots 2–3 */}
        {secondary.length > 0 && (
          <div className="flex flex-col gap-2">
            {secondary.map((t) => (
              <SecondaryRow key={t.key} thread={t} onOpen={() => onBrowse(undefined, t.displayName)} />
            ))}
          </div>
        )}

        {/* Slots 4–5 */}
        {tertiary.length > 0 && (
          <div className="flex flex-col">
            {tertiary.map((t) => (
              <TertiaryRow key={t.key} thread={t} onOpen={() => onBrowse(undefined, t.displayName)} />
            ))}
          </div>
        )}

        {/* Slot 6 */}
        <ExitRow total={totalSessions} onBrowse={(query) => onBrowse(query)} />
      </motion.div>
    </div>
  );
}
