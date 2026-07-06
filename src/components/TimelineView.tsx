/**
 * Timeline — the weekly reconstruction surface. Answers "what was I working on
 * this week and what's unfinished?" in a single unscrolled viewport.
 *
 * Progressive disclosure, four levels:
 *   L0 — threads as one-line table-of-contents rows (arc + member count + a
 *        dot-strip of member days), then one line per day (weekday + date +
 *        composite "N sessions · M projects · K open loops"). Gap days stay
 *        muted zero-count lines. If the window would overflow one viewport,
 *        the oldest days collapse behind a single "earlier" row — threads never
 *        collapse.
 *   L1 — a day expands (click/Enter) to project clusters, each collapsing past
 *        COLLAPSE_THRESHOLD sessions. A session row: title, project tail, time,
 *        and the digest worked-on line. The per-row Digest action is
 *        hover-revealed (group-hover/group-focus-within); no "no digest" text.
 *   L2 — a session expands to the full digest card (worked-on / outcome /
 *        open loops, provenance markers, inline edit, retry) — the pre-existing
 *        card relocated one level deeper, not rewritten.
 *   L3 — the card's open action hands off to the existing SessionDetail
 *        overlay via onSelect, unchanged.
 *
 * Expansion is additive (never collapses a sibling), never refetches
 * (get_timeline already returned everything), and never shifts content above
 * the expansion point. Expanded state is keyed by date / sessionId so it
 * survives resizes and "Digest week" refreshes. Roving focus: ArrowUp/Down
 * walk visible rows across levels, ArrowRight/Enter expand, ArrowLeft/Escape
 * collapse and return focus to the row that owned the disclosure.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { format } from "date-fns";
import {
  Loader2,
  Sparkles,
  ChevronRight,
  Clock,
  MessageSquare,
} from "lucide-react";
import type {
  SessionCard,
  SessionDigest,
  TimelineResponse,
  Thread,
  DigestBatchReport,
  DigestPatch,
  TagError,
} from "../lib/ipc";
import {
  getTimeline,
  digestSession,
  digestPending,
  linkThreads,
  updateSessionDigest,
  asTagError,
} from "../lib/ipc";
import { relativeTime } from "../lib/format";

// The one spring the app uses everywhere — light, quick, no gratuitous bounce.
const SPRING = { type: "spring", stiffness: 400, damping: 30 } as const;

// L0 budget bookkeeping: estimated row heights (px) used to decide how many
// day rows fit one viewport before the oldest collapse behind an "earlier" row.
const THREAD_ROW_PX = 32;
const DAY_ROW_PX = 40;
const GAP_ROW_PX = 26;
const BODY_CHROME_PX = 56; // body padding + threads/days separation
const MIN_VISIBLE_DAYS = 3;

// Sessions shown in a project cluster's preview. Clusters over this threshold
// render the first N rows + "show N more"; expanding reveals the rest. A
// 25-session / 4-project day reads as four scannable previews, not a flat
// scroll — the same skeleton the L0 collapse gives across days, one level down.
const COLLAPSE_THRESHOLD = 3;

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

type Announce = (message: string, kind?: "polite" | "assertive") => void;

interface TimelineViewProps {
  /** The full sessions list App already holds — joined by id for card metadata. */
  sessions: SessionCard[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function TimelineView({ sessions, selectedId, onSelect }: TimelineViewProps) {
  const [days, setDays] = useState(7);
  const [resp, setResp] = useState<TimelineResponse | null>(null);
  const [digests, setDigests] = useState<Record<string, SessionDigest>>({});
  const [threads, setThreads] = useState<Thread[]>([]);
  const [metaSessionIds, setMetaSessionIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [activeThread, setActiveThread] = useState<string | null>(null);
  const [openLoopsOnly, setOpenLoopsOnly] = useState(false);
  // null = idle, "running" = backfill in flight, report = last finished counts.
  const [weekRun, setWeekRun] = useState<null | "running" | DigestBatchReport>(null);

  // Disclosure state — keyed by date / sessionId / `${date}:${project}` (never
  // array index) so it survives window resizes and "Digest week" refreshes.
  const [expandedDays, setExpandedDays] = useState<Set<string>>(new Set());
  const [expandedSessions, setExpandedSessions] = useState<Set<string>>(new Set());
  // Three-state cluster disclosure, keyed by `${date}:${project}`:
  //   absent / 0 = collapsed (0 rows — header carries the count)
  //   1          = preview (first COLLAPSE_THRESHOLD rows + "show N more")
  //   2          = all rows
  // Each toggle advances 0 → 1 → 2 → 0. Clusters at or under the threshold
  // skip the preview stage — stage 0 already shows every row.
  const [clusterStage, setClusterStage] = useState<Map<string, number>>(new Map());
  // "App activity" group (meta/harness sessions) — collapsed by default; only
  // expands on user action. Separate from day rows/totals (FIX 3).
  const [showAppActivity, setShowAppActivity] = useState(false);
  const [showEarlier, setShowEarlier] = useState(false);

  // Roving focus: one row carries tabIndex 0; arrows move focus among rows.
  const [rovingKey, setRovingKey] = useState<string | null>(null);
  const rowRefs = useRef(new Map<string, HTMLButtonElement>());

  // aria-live announcements for async digest generation (polite) and errors
  // (assertive, once — the region re-announces only when the text changes).
  const [politeMsg, setPoliteMsg] = useState("");
  const [assertiveMsg, setAssertiveMsg] = useState("");
  const announce = useCallback<Announce>((message, kind = "polite") => {
    if (kind === "assertive") setAssertiveMsg(message);
    else setPoliteMsg(message);
  }, []);

  const reduceMotion = useReducedMotion() ?? false;
  const regionTransition = reduceMotion ? ({ duration: 0 } as const) : SPRING;

  const cardById = useMemo(() => {
    const m = new Map<string, SessionCard>();
    for (const s of sessions) m.set(s.id, s);
    return m;
  }, [sessions]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const r = await getTimeline(days);
      setResp(r);
      setDigests(r.digests ?? {});
      setThreads(r.threads ?? []);
      setMetaSessionIds(r.metaSessionIds ?? []);
    } catch (e) {
      setError(asTagError(e).message);
    } finally {
      setLoading(false);
    }
  }, [days]);

  useEffect(() => {
    load();
  }, [load]);

  // Fold a freshly generated / edited digest into the shared map, no reload.
  const applyDigest = useCallback((id: string, d: SessionDigest) => {
    setDigests((prev) => ({ ...prev, [id]: d }));
  }, []);

  // Header-level backfill: generate every pending digest in the window, refresh
  // the (now-populated) timeline, then relink threads. Never blocks the view —
  // no loading flag flips, so the current content stays interactive throughout.
  const digestWeek = async () => {
    setWeekRun("running");
    try {
      const report = await digestPending(days);
      setWeekRun(report);
      const r = await getTimeline(days);
      setResp(r);
      setDigests(r.digests ?? {});
      setMetaSessionIds(r.metaSessionIds ?? []);
      const t = await linkThreads(days);
      setThreads(t);
      announce(`Week digested: ${report.generated} new, ${report.cached} cached`);
    } catch (e) {
      const msg = asTagError(e).message;
      setError(msg);
      setWeekRun(null);
      announce(msg, "assertive");
    }
  };

  const toggleDay = useCallback((date: string) => {
    setExpandedDays((prev) => {
      const next = new Set(prev);
      if (next.has(date)) next.delete(date);
      else next.add(date);
      return next;
    });
  }, []);

  const toggleSession = useCallback((id: string) => {
    setExpandedSessions((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const toggleCluster = useCallback((date: string, project: string, count: number) => {
    const key = `${date}:${project}`;
    // Small clusters skip the preview stage (a preview of 2-of-2 is pointless),
    // so they cycle 0 → 2 → 0. Big clusters cycle 0 → 1 → 2 → 0.
    setClusterStage((prev) => {
      const next = new Map(prev);
      const cur = next.get(key) ?? 0;
      if (count > COLLAPSE_THRESHOLD) {
        next.set(key, (cur + 1) % 3); // 0 → 1 → 2 → 0
      } else {
        next.set(key, cur === 0 ? 2 : 0); // 0 → 2 → 0 (skip preview)
      }
      return next;
    });
  }, []);

  // A filtered thread auto-expands its member days to L1 — additive, so days
  // the user already opened stay open.
  useEffect(() => {
    if (!activeThread || !resp) return;
    const thread = threads.find((t) => t.id === activeThread);
    if (!thread) return;
    const member = new Set(thread.memberSessionIds);
    const dates = resp.days
      .filter((d) => d.sessionIds.some((id) => member.has(id)))
      .map((d) => d.date);
    if (dates.length === 0) return;
    setExpandedDays((prev) => new Set([...prev, ...dates]));
  }, [activeThread, threads, resp]);

  // Measure the reading column so the L0 budget collapse is driven by the real
  // viewport, not a guess. Estimation constants above keep this a plain sum —
  // no layout-feedback loop.
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const [bodyHeight, setBodyHeight] = useState<number | null>(null);
  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    setBodyHeight(el.clientHeight);
    const ro = new ResizeObserver(() => setBodyHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [resp === null]);

  if (loading && !resp) {
    return (
      <div className="flex flex-1 items-center justify-center bg-canvas text-ink-3">
        <Loader2 size={20} className="animate-spin" />
      </div>
    );
  }

  if (error && !resp) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 bg-canvas">
        <p className="text-[13px] text-ink-2">Couldn't load the timeline.</p>
        <p className="text-[11.5px] text-ink-4">{error}</p>
        <button
          onClick={load}
          className="mt-1 text-[12px] font-medium text-accent transition hover:text-accent-strong"
        >
          Retry
        </button>
      </div>
    );
  }

  const orderedDays = resp ? [...resp.days].sort((a, b) => b.date.localeCompare(a.date)) : [];
  const windowDatesAsc = orderedDays.map((d) => d.date).slice().reverse();
  const sessionDate = new Map<string, string>();
  for (const day of orderedDays) for (const id of day.sessionIds) sessionDate.set(id, day.date);

  // Thread filter is client-side: restrict day sections to a thread's members.
  const activeMembers =
    activeThread ? threads.find((t) => t.id === activeThread)?.memberSessionIds ?? [] : null;
  const passes = (id: string) => !activeMembers || activeMembers.includes(id);

  // Open-loops read: flatten every session that has open loops, newest first.
  const openLoopIds: string[] = [];
  let openLoopsTotal = 0;
  for (const day of orderedDays) {
    for (const id of day.sessionIds) {
      const count = digests[id]?.openLoops?.length ?? 0;
      openLoopsTotal += count;
      if (!passes(id)) continue;
      if (count > 0) openLoopIds.push(id);
    }
  }

  // L0 budget: collapse the oldest days behind one "earlier" row when the
  // default render would overflow the viewport. Threads never collapse, and a
  // day the user expanded is never hidden by a resize.
  let visibleDayCount = orderedDays.length;
  if (bodyHeight && !showEarlier && !activeThread && !openLoopsOnly) {
    const budget =
      bodyHeight - BODY_CHROME_PX - threads.length * THREAD_ROW_PX - (threads.length ? 16 : 0);
    let used = 0;
    let n = 0;
    for (const day of orderedDays) {
      const h = day.sessionCount === 0 ? GAP_ROW_PX : DAY_ROW_PX;
      if (used + h > budget && n >= MIN_VISIBLE_DAYS) break;
      used += h;
      n++;
    }
    visibleDayCount = n;
  }
  const collapseActive = visibleDayCount < orderedDays.length;
  const visibleDays = collapseActive
    ? orderedDays.filter((d, i) => i < visibleDayCount || expandedDays.has(d.date))
    : orderedDays;
  const hiddenDays = collapseActive
    ? orderedDays.filter((d, i) => i >= visibleDayCount && !expandedDays.has(d.date))
    : [];
  const hiddenSessionTotal = hiddenDays.reduce((sum, d) => sum + d.sessionCount, 0);

  // The visible-row order, top to bottom — drives roving focus and arrow keys.
  const rowOrder: string[] = threads.map((t) => `thread:${t.id}`);
  if (openLoopsOnly) {
    for (const id of openLoopIds) rowOrder.push(`sess:${id}`);
  } else {
    for (const day of visibleDays) {
      const ids = day.sessionIds.filter(passes);
      if (activeMembers && ids.length === 0) continue;
      if (day.sessionCount === 0) continue; // gap lines are text, not rows
      rowOrder.push(`day:${day.date}`);
      if (expandedDays.has(day.date)) for (const id of ids) rowOrder.push(`sess:${id}`);
    }
    if (hiddenDays.length > 0) rowOrder.push("earlier");
  }
  const tabStopKey = rovingKey && rowOrder.includes(rovingKey) ? rovingKey : rowOrder[0] ?? null;

  const registerRow = (key: string) => (el: HTMLButtonElement | null) => {
    if (el) rowRefs.current.set(key, el);
    else rowRefs.current.delete(key);
  };

  const focusRow = (key: string | null | undefined) => {
    if (!key) return;
    const el = rowRefs.current.get(key);
    if (!el) return;
    el.focus();
    setRovingKey(key);
  };

  const dayIdsOf = (date: string) =>
    orderedDays.find((d) => d.date === date)?.sessionIds.filter(passes) ?? [];

  // Within-day grouping by project — kills the L1 flat-scroll regression. The
  // day header already derives `projects.size` the same way; here we keep
  // first-appearance order (post-FIX-4 = last-activity-descending) so the
  // most-recently-touched project tops the day. Project key mirrors the L1 row.
  const projectKeyOf = (id: string) => {
    const c = cardById.get(id);
    return c ? c.projectShortName ?? c.displayProject : "unknown";
  };
  const clusterByProject = (ids: string[]) => {
    const order: string[] = [];
    const groups = new Map<string, string[]>();
    for (const id of ids) {
      const key = projectKeyOf(id);
      if (!groups.has(key)) {
        groups.set(key, []);
        order.push(key);
      }
      groups.get(key)!.push(id);
    }
    return order.map((key) => ({ key, ids: groups.get(key)! }));
  };

  // One keyboard contract for every level. Text inputs opt out (inline edit
  // owns its own Enter/Escape); Escape from inside an expanded region always
  // returns focus to the row that owned the disclosure.
  const handleKeyDown = (e: React.KeyboardEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName === "TEXTAREA" || target.tagName === "INPUT") return;
    const rowKey = target.closest("[data-trow]")?.getAttribute("data-trow") ?? null;

    // Enter activates the focused row explicitly (preventDefault suppresses the
    // native button activation so the toggle fires exactly once everywhere,
    // including webviews that don't synthesize click from keyboard activation).
    if (e.key === "Enter" && rowKey && target.closest("[data-trow]") === target) {
      e.preventDefault();
      (target as HTMLElement).click();
      return;
    }

    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (!rowKey) return;
      e.preventDefault();
      const i = rowOrder.indexOf(rowKey);
      if (i === -1) return;
      const j = e.key === "ArrowDown" ? Math.min(i + 1, rowOrder.length - 1) : Math.max(i - 1, 0);
      focusRow(rowOrder[j]);
      return;
    }

    if (e.key === "ArrowRight" && rowKey) {
      if (rowKey.startsWith("day:")) {
        e.preventDefault();
        const date = rowKey.slice(4);
        if (!expandedDays.has(date)) toggleDay(date);
        else focusRow(`sess:${dayIdsOf(date)[0]}`);
      } else if (rowKey.startsWith("sess:")) {
        e.preventDefault();
        const id = rowKey.slice(5);
        if (!expandedSessions.has(id)) toggleSession(id);
      }
      return;
    }

    if (e.key === "ArrowLeft" && rowKey) {
      if (rowKey.startsWith("day:")) {
        const date = rowKey.slice(4);
        if (expandedDays.has(date)) {
          e.preventDefault();
          toggleDay(date); // focus stays on the day row — it owns the disclosure
        }
      } else if (rowKey.startsWith("sess:")) {
        e.preventDefault();
        const id = rowKey.slice(5);
        if (expandedSessions.has(id)) toggleSession(id);
        else focusRow(`day:${sessionDate.get(id)}`);
      }
      return;
    }

    if (e.key === "Escape") {
      // Inside an expanded region but not on a row (e.g. the digest card):
      // collapse the owning disclosure and return focus to its row.
      const region = target.closest("[data-owner]");
      if (region && !rowKey) {
        e.preventDefault();
        e.stopPropagation();
        const owner = region.getAttribute("data-owner")!;
        if (owner.startsWith("sess:")) toggleSession(owner.slice(5));
        else if (owner.startsWith("day:")) toggleDay(owner.slice(4));
        focusRow(owner);
        return;
      }
      if (rowKey?.startsWith("sess:")) {
        const id = rowKey.slice(5);
        if (expandedSessions.has(id)) {
          e.preventDefault();
          e.stopPropagation();
          toggleSession(id); // this row owns the disclosure — focus stays here
        } else {
          const date = sessionDate.get(id);
          if (date && expandedDays.has(date)) {
            e.preventDefault();
            e.stopPropagation();
            toggleDay(date);
            focusRow(`day:${date}`);
          }
        }
        return;
      }
      if (rowKey?.startsWith("day:")) {
        const date = rowKey.slice(4);
        if (expandedDays.has(date)) {
          e.preventDefault();
          e.stopPropagation();
          toggleDay(date);
        }
      }
    }
  };

  const revealEarlier = () => {
    const firstHidden = hiddenDays.find((d) => d.sessionCount > 0);
    setShowEarlier(true);
    requestAnimationFrame(() => focusRow(firstHidden ? `day:${firstHidden.date}` : null));
  };

  const renderSessionRow = (id: string, inlineLoops: boolean, showProject = true) => (
    <SessionRow
      key={id}
      sessionId={id}
      card={cardById.get(id)}
      digest={digests[id]}
      expanded={expandedSessions.has(id)}
      onToggle={() => toggleSession(id)}
      tabIndex={tabStopKey === `sess:${id}` ? 0 : -1}
      refCb={registerRow(`sess:${id}`)}
      onFocusRow={() => setRovingKey(`sess:${id}`)}
      selected={selectedId === id}
      onSelect={onSelect}
      onApply={applyDigest}
      announce={announce}
      inlineLoops={inlineLoops}
      transition={regionTransition}
      showProject={showProject}
    />
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-canvas">
      {/* Screen-reader announcements: digest completion (polite), errors (assertive). */}
      <div aria-live="polite" className="sr-only">
        {politeMsg}
      </div>
      <div aria-live="assertive" className="sr-only">
        {assertiveMsg}
      </div>

      {/* Header: window selector · open-loops toggle (with the week's aggregate
          count — the only loops number that renders at L0) · digest-week action */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-5 py-3">
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-0.5 rounded bg-surface-2 p-0.5">
            {[7, 14, 30].map((d) => (
              <button
                key={d}
                onClick={() => setDays(d)}
                className={`rounded-sm px-2.5 py-1 text-[12px] font-medium tabular-nums transition ${
                  days === d ? "bg-surface text-ink shadow-xs" : "text-ink-3 hover:text-ink-2"
                }`}
              >
                {d}d
              </button>
            ))}
          </div>
          <button
            onClick={() => setOpenLoopsOnly((v) => !v)}
            aria-pressed={openLoopsOnly}
            title="Show only sessions with unfinished open loops"
            className={`inline-flex min-h-[32px] items-center gap-1.5 rounded-sm border px-2.5 py-1 text-[12px] font-medium transition ${
              openLoopsOnly
                ? "border-accent/40 bg-accent-soft text-accent-strong"
                : "border-border bg-surface text-ink-2 hover:border-border-strong"
            }`}
          >
            Open loops
            {openLoopsTotal > 0 && (
              <span
                className={`rounded-full px-1.5 py-px text-[10px] font-semibold tabular-nums ${
                  openLoopsOnly ? "bg-accent/10 text-accent-strong" : "bg-surface-3 text-ink-2"
                }`}
              >
                {openLoopsTotal}
              </span>
            )}
          </button>
        </div>

        <div className="flex items-center gap-2">
          {weekRun && weekRun !== "running" && (
            <span className="text-[11px] tabular-nums text-ink-3">
              {weekRun.generated} new · {weekRun.cached} cached
              {weekRun.failed > 0 ? ` · ${weekRun.failed} failed` : ""}
              {weekRun.skippedNoRecap > 0 ? ` · ${weekRun.skippedNoRecap} no recap` : ""}
            </span>
          )}
          <button
            onClick={digestWeek}
            disabled={weekRun === "running"}
            title="Generate digests for every session in the window"
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded-sm border border-border bg-surface px-2.5 py-1 text-[12px] font-medium text-ink-2 shadow-xs transition hover:border-border-strong hover:text-ink disabled:opacity-50"
          >
            {weekRun === "running" ? (
              <Loader2 size={12} className="animate-spin text-accent" />
            ) : (
              <Sparkles size={12} />
            )}
            {weekRun === "running" ? "Digesting week…" : "Digest week"}
          </button>
        </div>
      </div>

      {/* Body — one reading column. Threads first (the week's table of
          contents), then day rows. All disclosure happens below this point. */}
      <div ref={bodyRef} className="flex-1 overflow-y-auto px-5 py-4" onKeyDown={handleKeyDown}>
        {orderedDays.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <p className="text-[13px] text-ink-4">No sessions in this window.</p>
          </div>
        ) : (
          <div className="mx-auto max-w-[720px]">
            {threads.length > 0 && (
              <div role="list" aria-label="Threads" className="mb-4 space-y-px">
                {threads.map((t) => (
                  <ThreadLine
                    key={t.id}
                    thread={t}
                    active={activeThread === t.id}
                    windowDatesAsc={windowDatesAsc}
                    days={orderedDays}
                    onToggle={() => setActiveThread(activeThread === t.id ? null : t.id)}
                    tabIndex={tabStopKey === `thread:${t.id}` ? 0 : -1}
                    refCb={registerRow(`thread:${t.id}`)}
                    onFocusRow={() => setRovingKey(`thread:${t.id}`)}
                  />
                ))}
              </div>
            )}

            {openLoopsOnly ? (
              openLoopIds.length === 0 ? (
                <p className="py-8 text-center text-[13px] text-ink-4">
                  No open loops in this window.
                </p>
              ) : (
                <div className="space-y-0.5">
                  {openLoopIds.map((id) => renderSessionRow(id, true))}
                </div>
              )
            ) : (
              <div className="space-y-0.5">
                {visibleDays.map((day) => {
                  const d = new Date(day.date + "T00:00:00");
                  const weekday = format(d, "EEEE");
                  const dateLabel = format(d, "MMM d");
                  const ids = day.sessionIds.filter(passes);
                  // Under a thread filter, a day with no members is not part of
                  // the story — drop it rather than show an empty header.
                  if (activeMembers && ids.length === 0) return null;

                  if (day.sessionCount === 0) {
                    return (
                      <p key={day.date} className="px-2 py-1 text-[11.5px] text-ink-4">
                        <span className="font-medium text-ink-3">{weekday}</span> {dateLabel} · no
                        sessions
                      </p>
                    );
                  }

                  const projects = new Set(
                    ids.map((id) => {
                      const c = cardById.get(id);
                      return c ? c.projectShortName ?? c.displayProject : "unknown";
                    }),
                  );
                  const loops = ids.reduce(
                    (sum, id) => sum + (digests[id]?.openLoops?.length ?? 0),
                    0,
                  );
                  const composite =
                    `${plural(ids.length, "session")} · ${plural(projects.size, "project")}` +
                    (loops > 0 ? ` · ${plural(loops, "open loop")}` : "");
                  const expanded = expandedDays.has(day.date);
                  const regionId = `tl-day-${day.date}`;

                  return (
                    <div key={day.date}>
                      <button
                        data-trow={`day:${day.date}`}
                        ref={registerRow(`day:${day.date}`)}
                        tabIndex={tabStopKey === `day:${day.date}` ? 0 : -1}
                        onFocus={() => setRovingKey(`day:${day.date}`)}
                        onClick={() => toggleDay(day.date)}
                        aria-expanded={expanded}
                        aria-controls={expanded ? regionId : undefined}
                        className="flex min-h-[36px] w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-surface-2/60"
                      >
                        <ChevronRight
                          size={13}
                          aria-hidden
                          className={`shrink-0 text-ink-4 transition-transform ${
                            expanded ? "rotate-90" : ""
                          }`}
                        />
                        <span className="w-24 shrink-0 text-[13.5px] font-semibold text-ink">
                          {weekday}
                        </span>
                        <span className="shrink-0 text-[11.5px] text-ink-3">{dateLabel}</span>
                        <span className="min-w-0 flex-1 truncate text-right text-[12.5px] font-semibold tabular-nums text-ink-2">
                          {composite}
                        </span>
                      </button>
                      <AnimatePresence initial={false}>
                        {expanded && (
                          <motion.div
                            key="region"
                            id={regionId}
                            role="region"
                            aria-label={`Sessions for ${weekday}, ${dateLabel}`}
                            data-owner={`day:${day.date}`}
                            initial={{ height: 0, opacity: 0 }}
                            animate={{ height: "auto", opacity: 1 }}
                            exit={{ height: 0, opacity: 0 }}
                            transition={regionTransition}
                            className="overflow-hidden"
                          >
                            <div className="ml-[15px] space-y-1 border-l border-border py-1 pl-3">
                              {(() => {
                                const clusters = clusterByProject(ids);
                                // Always render through ProjectCluster so every
                                // group of sessions collapses the same way —
                                // even a day with a single project gets a header
                                // + the 3-state disclosure (consistency beats
                                // saving one header row).
                                return clusters.map(({ key, ids: cIds }) => (
                                  <ProjectCluster
                                    key={key}
                                    projectKey={key}
                                    ids={cIds}
                                    stage={clusterStage.get(`${day.date}:${key}`) ?? 0}
                                    onToggle={() => toggleCluster(day.date, key, cIds.length)}
                                    renderRow={renderSessionRow}
                                  />
                                ));
                              })()}
                            </div>
                          </motion.div>
                        )}
                      </AnimatePresence>
                    </div>
                  );
                })}

                {hiddenDays.length > 0 && (
                  <button
                    data-trow="earlier"
                    ref={registerRow("earlier")}
                    tabIndex={tabStopKey === "earlier" ? 0 : -1}
                    onFocus={() => setRovingKey("earlier")}
                    onClick={revealEarlier}
                    aria-expanded={false}
                    className="flex min-h-[32px] w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11.5px] text-ink-4 transition hover:bg-surface-2/60 hover:text-ink-2"
                  >
                    <ChevronRight size={13} aria-hidden className="shrink-0" />
                    Show {plural(hiddenDays.length, "earlier day")}
                    {hiddenSessionTotal > 0 ? ` · ${plural(hiddenSessionTotal, "session")}` : ""}
                  </button>
                )}

                {/* App activity — harness/self sessions the timeline spawned
                    (triage, digest, grouping calls). Filtered out of every day
                    and total above; surfaced here, collapsed by default, only
                    when present and the view isn't filtered. Never counted in
                    the day summary's sessions/open-loops totals. */}
                {metaSessionIds.length > 0 && (
                  <div className="mt-2 border-t border-border/60 pt-1">
                    <button
                      onClick={() => setShowAppActivity((v) => !v)}
                      aria-expanded={showAppActivity}
                      className="flex min-h-[28px] w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-[11.5px] text-ink-3 transition hover:bg-surface-2/60 hover:text-ink-2"
                    >
                      <ChevronRight
                        size={12}
                        aria-hidden
                        className={`shrink-0 text-ink-4 transition-transform ${showAppActivity ? "rotate-90" : ""}`}
                      />
                      App activity
                      <span className="tabular-nums text-ink-4">
                        · {plural(metaSessionIds.length, "hidden session")}
                      </span>
                    </button>
                    <AnimatePresence initial={false}>
                      {showAppActivity && (
                        <motion.div
                          key="app-activity"
                          initial={{ height: 0, opacity: 0 }}
                          animate={{ height: "auto", opacity: 1 }}
                          exit={{ height: 0, opacity: 0 }}
                          transition={regionTransition}
                          className="overflow-hidden"
                        >
                          <div className="ml-[15px] space-y-0.5 border-l border-border/60 py-1 pl-3">
                            {metaSessionIds.map((id) => renderSessionRow(id, false, true))}
                          </div>
                        </motion.div>
                      )}
                    </AnimatePresence>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** L0 thread line — the week's table of contents. Arc + member count + a
 *  dot-strip of member days (chronological). Click filters the timeline to the
 *  thread and auto-expands its member days. */
function ThreadLine({
  thread,
  active,
  windowDatesAsc,
  days,
  onToggle,
  tabIndex,
  refCb,
  onFocusRow,
}: {
  thread: Thread;
  active: boolean;
  windowDatesAsc: string[];
  days: { date: string; sessionIds: string[] }[];
  onToggle: () => void;
  tabIndex: number;
  refCb: (el: HTMLButtonElement | null) => void;
  onFocusRow: () => void;
}) {
  const member = new Set(thread.memberSessionIds);
  const memberDates = new Set(
    days.filter((d) => d.sessionIds.some((id) => member.has(id))).map((d) => d.date),
  );
  // Text equivalent for the color-only dot strip.
  const activeLabels = windowDatesAsc
    .filter((dt) => memberDates.has(dt))
    .map((dt) => format(new Date(dt + "T00:00:00"), "EEE MMM d"))
    .join(", ");

  return (
    <button
      data-trow={`thread:${thread.id}`}
      ref={refCb}
      tabIndex={tabIndex}
      onFocus={onFocusRow}
      onClick={onToggle}
      aria-pressed={active}
      title={`${thread.arc} — ${plural(thread.memberSessionIds.length, "session")}${
        activeLabels ? ` · active ${activeLabels}` : ""
      }. Click to filter the timeline to this thread.`}
      className={`flex min-h-[32px] w-full items-center gap-2.5 rounded-md px-2 py-1 text-left transition ${
        active ? "bg-accent-soft" : "hover:bg-surface-2/60"
      }`}
    >
      <span
        className={`min-w-0 flex-1 truncate text-[12.5px] leading-snug ${
          active ? "font-medium text-accent-strong" : "font-medium text-ink"
        }`}
      >
        {thread.arc}
      </span>
      <span className="shrink-0 text-[10.5px] tabular-nums text-ink-3">
        {plural(thread.memberSessionIds.length, "session")}
      </span>
      <span className="flex shrink-0 items-center gap-[3px]">
        <span className="sr-only">active {activeLabels || "no days"}</span>
        {windowDatesAsc.map((dt) => (
          <span
            key={dt}
            aria-hidden
            className={`h-[5px] w-[5px] rounded-full ${
              memberDates.has(dt) ? "bg-accent" : "bg-border"
            }`}
          />
        ))}
      </span>
    </button>
  );
}

/** L1 session row — one line: title, project tail, time, worked-on (or the
 *  muted no-digest state with a small Digest action). Expands in place to the
 *  full digest card (L2). */
function SessionRow({
  sessionId,
  card,
  digest,
  expanded,
  onToggle,
  tabIndex,
  refCb,
  onFocusRow,
  selected,
  onSelect,
  onApply,
  announce,
  inlineLoops,
  transition,
  showProject = true,
}: {
  sessionId: string;
  card: SessionCard | undefined;
  digest: SessionDigest | undefined;
  expanded: boolean;
  onToggle: () => void;
  tabIndex: number;
  refCb: (el: HTMLButtonElement | null) => void;
  onFocusRow: () => void;
  selected: boolean;
  onSelect: (id: string) => void;
  onApply: (id: string, d: SessionDigest) => void;
  announce: Announce;
  inlineLoops: boolean;
  transition: typeof SPRING | { duration: number };
  /** Show the per-row project tail. Off inside a project cluster (the cluster
   *  header already establishes the project — show it once, not N times). */
  showProject?: boolean;
}) {
  const title = card?.title || "(untitled session)";
  const projectTail = card ? card.projectShortName ?? card.displayProject : sessionId.slice(0, 8);
  const regionId = `tl-sess-${sessionId}`;

  return (
    <div>
      <div className="group flex items-center gap-1.5">
        <button
          data-trow={`sess:${sessionId}`}
          ref={refCb}
          tabIndex={tabIndex}
          onFocus={onFocusRow}
          onClick={onToggle}
          aria-expanded={expanded}
          aria-controls={expanded ? regionId : undefined}
          className={`flex min-h-[32px] min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1.5 text-left transition ${
            selected ? "bg-accent-soft/70" : "hover:bg-surface-2/60"
          }`}
        >
          <ChevronRight
            size={12}
            aria-hidden
            className={`shrink-0 text-ink-4 transition-transform ${expanded ? "rotate-90" : ""}`}
          />
          <span className="max-w-[220px] shrink-0 truncate text-[12.5px] font-medium text-ink">
            {title}
          </span>
          {showProject && (
            <span className="max-w-[110px] shrink-0 truncate text-[11px] text-ink-3">
              {projectTail}
            </span>
          )}
          <span className="shrink-0 text-[11px] tabular-nums text-ink-4">
            {relativeTime(card?.lastTs ?? null)}
          </span>
          {digest && (
            <span className="min-w-0 flex-1 truncate text-[12px] text-ink-2">
              {digest.workedOn}
            </span>
          )}
        </button>
        {!digest && (
          <DigestAction sessionId={sessionId} title={title} onApply={onApply} announce={announce} />
        )}
      </div>

      {/* Open-loops flatten mode surfaces loops inline at L1. */}
      {inlineLoops && digest && digest.openLoops.length > 0 && (
        <ul className="mb-1 ml-8 space-y-0.5">
          {digest.openLoops.map((l, i) => (
            <li key={i} className="flex items-start gap-1.5 text-[11.5px] text-ink-2">
              <span aria-hidden className="mt-[6px] h-1 w-1 shrink-0 rounded-full bg-warn" />
              <span>
                <span className="sr-only">open loop: </span>
                {l}
              </span>
            </li>
          ))}
        </ul>
      )}

      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            key="region"
            id={regionId}
            role="region"
            aria-label={`Digest for ${title}`}
            data-owner={`sess:${sessionId}`}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={transition}
            className="overflow-hidden"
          >
            <div className="py-1 pl-6">
              <DigestCard
                sessionId={sessionId}
                card={card}
                digest={digest}
                selected={selected}
                onSelect={onSelect}
                onApply={onApply}
                announce={announce}
              />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/** Within-day project cluster — groups a day's sessions by project so a
 *  25-session / 4-project day reads as four scannable clusters, not a flat
 *  endless scroll. The project label appears ONCE here (per FIX 2); rows inside
 *  hide their per-row project tail. Collapses past COLLAPSE_THRESHOLD with a
 *  "show N more" affordance (default collapsed). Mirrors the day-row disclosure
 *  pattern: chevron + spine, AnimatePresence + the shared spring. */
function ProjectCluster({
  projectKey,
  ids,
  stage,
  onToggle,
  renderRow,
}: {
  projectKey: string;
  ids: string[];
  /** 0 = collapsed (0 rows), 1 = preview (first N), 2 = all. The toggle cycle
   *  is size-aware: small clusters (≤ threshold) skip stage 1 (a preview of 2
   *  of 2 is pointless) and go 0 → 2 → 0, so EVERY cluster collapses at 0. */
  stage: number;
  onToggle: () => void;
  renderRow: (id: string, inlineLoops: boolean, showProject?: boolean) => React.ReactNode;
}) {
  const overThreshold = ids.length > COLLAPSE_THRESHOLD;
  // Stage 0 is always collapsed. Small clusters at stage 0 still hide — they
  // just expand straight to all (stage 2) on the first click, skipping preview.
  const isOpen = stage > 0;
  const inPreview = isOpen && stage < 2 && overThreshold;
  const visible = !isOpen ? [] : inPreview ? ids.slice(0, COLLAPSE_THRESHOLD) : ids;
  const hiddenCount = ids.length - visible.length;

  return (
    <div>
      <button
        onClick={onToggle}
        aria-expanded={isOpen}
        className="group/cluster flex min-h-[28px] w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left transition hover:bg-surface-2/60"
      >
        <ChevronRight
          size={11}
          aria-hidden
          className={`shrink-0 text-ink-4 transition-transform ${isOpen ? "rotate-90" : ""}`}
        />
        <span className="max-w-[180px] shrink-0 truncate text-[11.5px] font-semibold text-ink">
          {projectKey}
        </span>
        <span className="shrink-0 text-[10.5px] tabular-nums text-ink-4">
          {plural(ids.length, "session")}
        </span>
      </button>
      {isOpen && (
        <div className="ml-[7px] space-y-0.5 border-l border-border/70 py-0.5 pl-2.5">
          {visible.map((id) => renderRow(id, false, false))}
          {inPreview && hiddenCount > 0 && (
            <button
              onClick={onToggle}
              className="ml-1 rounded-sm px-1.5 py-1 text-left text-[11px] text-ink-3 transition hover:bg-surface-2/60 hover:text-ink-2"
            >
              show {hiddenCount} more
            </button>
          )}
          {!inPreview && overThreshold && (
            <button
              onClick={onToggle}
              className="ml-1 rounded-sm px-1.5 py-1 text-left text-[11px] text-ink-3 transition hover:bg-surface-2/60 hover:text-ink-2"
            >
              show less
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** The small L1 "Digest" action for sessions without a digest row. Announces
 *  completion politely and failures assertively; degrades honestly on
 *  no-recap sessions. */
function DigestAction({
  sessionId,
  title,
  onApply,
  announce,
}: {
  sessionId: string;
  title: string;
  onApply: (id: string, d: SessionDigest) => void;
  announce: Announce;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<TagError | null>(null);

  const run = async () => {
    setBusy(true);
    setErr(null);
    try {
      onApply(sessionId, await digestSession(sessionId));
      announce(`Digest ready for ${title}`);
    } catch (ex) {
      const t = asTagError(ex);
      setErr(t);
      announce(t.message, "assertive");
    } finally {
      setBusy(false);
    }
  };

  if (err?.kind === "no_recap") {
    return <span className="shrink-0 px-1 text-[10.5px] text-ink-4">no recap captured</span>;
  }
  if (err) {
    return (
      <span className="flex shrink-0 items-center gap-1.5 px-1 text-[10.5px]">
        <span className="text-danger" title={err.message}>
          failed
        </span>
        <button
          onClick={run}
          className="min-h-[32px] font-medium text-accent transition hover:text-accent-strong"
        >
          Retry
        </button>
      </span>
    );
  }
  return (
    <button
      onClick={run}
      disabled={busy}
      title={`Generate a digest for ${title}`}
      className={`inline-flex min-h-[32px] shrink-0 items-center gap-1 rounded-sm px-1.5 text-[11px] font-medium text-ink-3 transition hover:text-ink hover:bg-surface-2/60 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:text-ink disabled:opacity-100 ${
        busy ? "opacity-100" : "opacity-0"
      }`}
    >
      {busy ? (
        <Loader2 size={11} className="animate-spin text-accent" />
      ) : (
        <Sparkles size={11} />
      )}
      Digest
    </button>
  );
}

/** 4px accent dot marking a hand-edited field — a quiet marker, never a banner.
 *  Mirrors the ManualDot convention in SessionDetail. */
function ManualDot({ show }: { show: boolean }) {
  if (!show) return null;
  return (
    <span
      title="Edited by hand — regeneration will not overwrite"
      className="ml-1 inline-block h-1 w-1 shrink-0 rounded-full bg-accent align-middle"
    />
  );
}

/** Always-visible session metadata, joined from the index skeleton. Renders a
 *  truthful minimal fallback if the card isn't in the loaded list. */
function MetaLine({ card, sessionId }: { card: SessionCard | undefined; sessionId: string }) {
  if (!card) {
    return <div className="font-mono text-[11px] text-ink-4">session {sessionId.slice(0, 8)}</div>;
  }
  return (
    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-[11px] text-ink-3">
      <span className="max-w-[180px] truncate font-medium text-ink-2">
        {card.projectShortName ?? card.displayProject}
      </span>
      <span className="inline-flex items-center gap-0.5">
        <Clock size={10} />
        {relativeTime(card.lastTs)}
      </span>
      <span className="inline-flex items-center gap-0.5 tabular-nums">
        <MessageSquare size={10} />
        {card.messageCount}
      </span>
    </div>
  );
}

/** An inline-editable line. Click to edit; commit on blur/Enter, cancel on Esc.
 *  Same idiom as the editable tag fields in SessionDetail. */
function EditableLine({
  value,
  manual,
  onCommit,
  placeholder,
  className,
}: {
  value: string;
  manual: boolean;
  onCommit: (v: string) => void;
  placeholder: string;
  className: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  useEffect(() => {
    if (!editing) setDraft(value);
  }, [value, editing]);

  if (editing) {
    return (
      <textarea
        autoFocus
        rows={1}
        value={draft}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          setEditing(false);
          const v = draft.trim();
          if (v !== value) onCommit(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            (e.target as HTMLTextAreaElement).blur();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation(); // cancel the edit only — never collapse the card
            setDraft(value);
            setEditing(false);
          }
        }}
        className={`${className} w-full resize-none rounded-sm border border-accent/40 bg-surface px-1.5 py-1 focus:outline-none`}
      />
    );
  }

  return (
    <span
      onClick={(e) => {
        e.stopPropagation();
        setEditing(true);
      }}
      title="Click to edit"
      className={`${className} cursor-text rounded-sm hover:bg-surface-2/60`}
    >
      {value || <span className="text-ink-4">{placeholder}</span>}
      <ManualDot show={manual} />
    </span>
  );
}

interface DigestCardProps {
  sessionId: string;
  card: SessionCard | undefined;
  digest: SessionDigest | undefined;
  selected: boolean;
  onSelect: (id: string) => void;
  onApply: (id: string, d: SessionDigest) => void;
  announce: Announce;
}

/** L2 — the full digest card, unchanged in substance from the flat timeline:
 *  worked-on / outcome / open-loops, provenance markers, inline edit, retry.
 *  Clicking it (or its labelled open button) hands off to SessionDetail (L3). */
function DigestCard({
  sessionId,
  card,
  digest,
  selected,
  onSelect,
  onApply,
  announce,
}: DigestCardProps) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<TagError | null>(null);
  const reduceMotion = useReducedMotion() ?? false;
  const cardTransition = reduceMotion ? ({ duration: 0 } as const) : SPRING;

  const title = card?.title || "(untitled session)";

  const runDigest = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setBusy(true);
    setErr(null);
    try {
      onApply(sessionId, await digestSession(sessionId));
      announce(`Digest ready for ${title}`);
    } catch (ex) {
      const t = asTagError(ex);
      setErr(t);
      announce(t.message, "assertive");
    } finally {
      setBusy(false);
    }
  };

  const commitField = async (patch: DigestPatch) => {
    setErr(null);
    try {
      onApply(sessionId, await updateSessionDigest(sessionId, patch));
    } catch (ex) {
      const t = asTagError(ex);
      setErr(t);
      announce(t.message, "assertive");
    }
  };

  // The keyboard-reachable L3 handoff — the card body stays clickable too.
  const openDetail = (
    <button
      onClick={(e) => {
        e.stopPropagation();
        onSelect(sessionId);
      }}
      aria-label={`Open full detail for ${title}`}
      title="Open full session detail"
      className="-m-2 shrink-0 rounded-md p-2 transition hover:bg-surface-2/60"
    >
      <ChevronRight
        size={15}
        aria-hidden
        className={selected ? "text-accent" : "text-ink-4"}
      />
    </button>
  );

  return (
    <motion.div
      onClick={() => onSelect(sessionId)}
      whileHover={reduceMotion ? undefined : { y: -1 }}
      transition={cardTransition}
      className={`group cursor-pointer rounded-lg border p-3.5 transition ${
        selected
          ? "border-accent/40 bg-accent-soft/70 shadow-sm"
          : "border-border bg-surface hover:border-border-strong hover:shadow-sm"
      }`}
    >
      {digest ? (
        <motion.div
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={cardTransition}
          className="space-y-1.5"
        >
          {/* Worked on — the hero line. Verified content reads confident; an
              unverified / low-confidence digest stays quieter than verified. */}
          <div className="flex items-start gap-2">
            <EditableLine
              value={digest.workedOn}
              manual={digest.manualFields.includes("workedOn")}
              onCommit={(v) => commitField({ workedOn: v })}
              placeholder="What was worked on"
              className={`flex-1 text-[13.5px] leading-snug ${
                digest.verified ? "font-medium text-ink" : "text-ink-2"
              }`}
            />
            {digest.stale && (
              <span
                title="This session changed after the digest was written — regenerate to refresh"
                className="mt-[1px] shrink-0 rounded-full bg-surface-3 px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide text-ink-4"
              >
                stale
              </span>
            )}
            {openDetail}
          </div>

          {/* Title — secondary to the worked-on hero. */}
          {title && <div className="truncate text-[11.5px] text-ink-3">{title}</div>}

          {/* Outcome — secondary line, editable. */}
          <EditableLine
            value={digest.outcome}
            manual={digest.manualFields.includes("outcome")}
            onCommit={(v) => commitField({ outcome: v })}
            placeholder="Outcome"
            className="block text-[12px] leading-snug text-ink-2"
          />

          {/* Open loops — compact list with a subtle marker. */}
          {digest.openLoops.length > 0 && (
            <div className="pt-0.5">
              <ul className="space-y-0.5">
                {digest.openLoops.map((l, i) => (
                  <li key={i} className="flex items-start gap-1.5 text-[11.5px] text-ink-2">
                    <span aria-hidden className="mt-[6px] h-1 w-1 shrink-0 rounded-full bg-warn" />
                    <span>
                      <span className="sr-only">open loop: </span>
                      {l}
                    </span>
                  </li>
                ))}
              </ul>
              <ManualDot show={digest.manualFields.includes("openLoops")} />
            </div>
          )}

          <MetaLine card={card} sessionId={sessionId} />

          {/* An edit that failed must say so — degrade to the message for any
              kind (informational kinds read softer, same as SessionDetail). */}
          {err && (
            <p
              className={`text-[11px] ${
                err.kind === "no_api_key" || err.kind === "cli_not_found"
                  ? "text-ink-2"
                  : "text-danger"
              }`}
            >
              {err.message}
            </p>
          )}

          {!digest.verified && (
            <p className="text-[10.5px] italic text-ink-4">
              unverified
              {digest.confidence != null
                ? ` · ${Math.round(digest.confidence * 100)}% confidence`
                : ""}
            </p>
          )}
        </motion.div>
      ) : (
        // No digest row yet — render index facts truthfully + a Digest action.
        <div className="space-y-1.5">
          <div className="flex items-start gap-2">
            <p className="flex-1 text-[13px] font-medium leading-snug text-ink">{title}</p>
            {openDetail}
          </div>
          <MetaLine card={card} sessionId={sessionId} />
          <div className="pt-0.5" onClick={(e) => e.stopPropagation()}>
            {err?.kind === "no_recap" ? (
              <span className="text-[11px] text-ink-4">no recap captured</span>
            ) : err ? (
              <div className="flex items-center gap-2 text-[11px]">
                <span className="text-danger">{err.message}</span>
                <button
                  onClick={runDigest}
                  className="font-medium text-accent transition hover:text-accent-strong"
                >
                  Retry
                </button>
              </div>
            ) : (
              <button
                onClick={runDigest}
                disabled={busy}
                title="Generate a digest for this session"
                className="inline-flex min-h-[32px] items-center gap-1.5 rounded-sm border border-border bg-surface px-2.5 py-1 text-[11.5px] font-medium text-ink-2 shadow-xs transition hover:border-border-strong hover:text-ink disabled:opacity-50"
              >
                {busy ? (
                  <Loader2 size={12} className="animate-spin text-accent" />
                ) : (
                  <Sparkles size={12} />
                )}
                Digest
              </button>
            )}
          </div>
        </div>
      )}
    </motion.div>
  );
}
