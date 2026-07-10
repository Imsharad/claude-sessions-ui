/**
 * Review tab — per-project report cards for a 7/14/30-day window.
 *
 * Three disclosure tiers, each answering a question, nothing from a deeper
 * tier leaking upward:
 *   Tier 0 (shelf, default) — one row per project: name, hub prefix, headline
 *     hero, right-aligned stat cluster (sessions · files · cost).
 *   Tier 1 (expanded, click row) — Built / How / Why sections + desired-vs-real
 *     rows; each Built claim's evidence renders as session chips.
 *   Tier 2 (evidence, click chip) — opens SessionDetail (mounted by App).
 *
 * Modeled on TimelineView: self-fetching (useCallback + useEffect), with
 * loading/error/empty states, the same 7d/14d/30d segmented control, and the
 * app-wide SPRING. Warm paper canvas, cream surfaces, headline-as-hero.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { Loader2, RefreshCw, Sparkles, ChevronRight } from "lucide-react";
import type { ReviewResponse, ProjectReport, ReportBatchReport } from "../lib/ipc";
import { getReview, generateReports, asTagError } from "../lib/ipc";
import { relativeTime, formatCost } from "../lib/format";

// The one spring the app uses everywhere — light, quick, no gratuitous bounce.
const SPRING = { type: "spring", stiffness: 400, damping: 30 } as const;

interface ReviewViewProps {
  /** The full sessions list App already holds — unused for fetch, reserved for
   *  future cross-project context. Kept for prop-parity with TimelineView. */
  sessions: unknown[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function ReviewView({ selectedId, onSelect }: ReviewViewProps) {
  const [days, setDays] = useState(7);
  const [resp, setResp] = useState<ReviewResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // null = idle, "running" = generation in flight, report = last finished counts.
  const [genRun, setGenRun] = useState<null | "running" | ReportBatchReport>(null);
  // Expanded cards (Tier 1), keyed by projectKey.
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const reduceMotion = useReducedMotion() ?? false;

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setResp(await getReview(days));
    } catch (e) {
      setError(asTagError(e).message);
    } finally {
      setLoading(false);
    }
  }, [days]);

  useEffect(() => {
    load();
  }, [load]);

  // Generate (or refresh) report cards for the whole window, then reload.
  const handleGenerate = useCallback(async () => {
    setGenRun("running");
    try {
      const report = await generateReports(days);
      setGenRun(report);
      await load();
    } catch (e) {
      setError(asTagError(e).message);
      setGenRun(null);
    }
  }, [days, load]);

  const toggle = (key: string) =>
    setExpanded((prev) => {
      const n = new Set(prev);
      n.has(key) ? n.delete(key) : n.add(key);
      return n;
    });

  // ─── States ──────────────────────────────────────────────────────────────

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
        <p className="text-[13px] text-ink-2">Couldn't load the review.</p>
        <p className="text-[11.5px] text-ink-4">{error}</p>
        <button onClick={load} className="mt-1 text-[12px] font-medium text-accent transition hover:text-accent-strong">
          Retry
        </button>
      </div>
    );
  }

  const cards = resp?.cards ?? [];
  const needsGeneration = cards.length > 0 && cards.every((c) => c.headline === null);
  const generatedAt = cards.find((c) => c.generatedAt)?.generatedAt ?? null;

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-canvas">
      {/* Header: title, window selector, refresh, age. */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-5 py-3">
        <div className="flex items-center gap-2">
          <h2 className="text-[14px] font-semibold text-ink">Review</h2>
          {generatedAt && (
            <span className="text-[11px] text-ink-4">as of {relativeTime(generatedAt)}</span>
          )}
          {genRun && genRun !== "running" && (
            <span className="text-[11px] text-ink-4">
              · {genRun.generated} new · {genRun.cached} cached
              {genRun.failed > 0 ? ` · ${genRun.failed} failed` : ""}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {/* 7d / 14d / 30d segmented control — same idiom as TimelineView. */}
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
            onClick={handleGenerate}
            disabled={genRun === "running" || cards.length === 0}
            className="inline-flex items-center gap-1 rounded-md border border-border bg-surface px-2.5 py-1 text-[12px] font-medium text-ink-2 transition hover:bg-surface-2 disabled:opacity-40"
          >
            {genRun === "running" ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <RefreshCw size={12} />
            )}
            {cards.length > 0 && needsGeneration ? "Generate" : "Refresh"}
          </button>
        </div>
      </div>

      {/* Body. */}
      <div className="flex-1 overflow-y-auto px-5 py-4">
        <div className="mx-auto max-w-[720px]">
          {cards.length === 0 ? (
            <EmptyReview onGenerate={handleGenerate} generating={genRun === "running"} hasSessions={false} />
          ) : needsGeneration && !genRun ? (
            <EmptyReview onGenerate={handleGenerate} generating={genRun === "running"} hasSessions />
          ) : (
            <div className="flex flex-col gap-2.5">
              {cards.map((card) => (
                <ProjectCard
                  key={card.projectKey}
                  card={card}
                  expanded={expanded.has(card.projectKey)}
                  onToggle={() => toggle(card.projectKey)}
                  onSelect={onSelect}
                  selectedId={selectedId}
                  reduceMotion={reduceMotion}
                  onRefresh={handleGenerate}
                  refreshing={genRun === "running"}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Empty state ─────────────────────────────────────────────────────────────

function EmptyReview({
  onGenerate,
  generating,
  hasSessions,
}: {
  onGenerate: () => void;
  generating: boolean;
  hasSessions: boolean;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-20 text-center">
      <Sparkles size={28} className="text-ink-4" />
      <div>
        <p className="text-[13.5px] font-medium text-ink-2">
          {hasSessions ? "No report cards yet for this window." : "No digested sessions in this window."}
        </p>
        <p className="mt-1 text-[12px] text-ink-4">
          {hasSessions
            ? "Generate per-project report cards — what was built, how, and why."
            : "Sessions get digested on the Timeline tab first, then they appear here."}
        </p>
      </div>
      {hasSessions && (
        <button
          onClick={onGenerate}
          disabled={generating}
          className="mt-1 inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-[12.5px] font-medium text-white transition hover:bg-accent-strong disabled:opacity-50"
        >
          {generating ? <Loader2 size={13} className="animate-spin" /> : <Sparkles size={13} />}
          Generate report cards
        </button>
      )}
    </div>
  );
}

// ─── Tier 0 + Tier 1 (one project card) ──────────────────────────────────────

function ProjectCard({
  card,
  expanded,
  onToggle,
  onSelect,
  selectedId,
  reduceMotion,
  onRefresh,
  refreshing,
}: {
  card: ProjectReport;
  expanded: boolean;
  onToggle: () => void;
  onSelect: (id: string) => void;
  selectedId: string | null;
  reduceMotion: boolean;
  onRefresh: () => void;
  refreshing: boolean;
}) {
  const hasReport = card.headline !== null;
  const sessionCount = card.sessionIds.length;

  return (
    <motion.div
      layout={!reduceMotion}
      className="overflow-hidden rounded-lg border border-border bg-surface shadow-xs"
    >
      {/* Tier 0 — the shelf row. */}
      <button
        onClick={onToggle}
        className="flex w-full items-center gap-3 px-4 py-3 text-left transition hover:bg-surface-2"
      >
        <ChevronRight
          size={14}
          className={`shrink-0 text-ink-4 transition-transform ${expanded ? "rotate-90" : ""}`}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-1.5">
            {card.hub && (
              <span className="shrink-0 text-[11px] font-medium uppercase tracking-wide text-ink-4">
                {card.hub} /
              </span>
            )}
            <span className="truncate text-[13.5px] font-semibold text-ink">{card.name}</span>
            {card.manualFields.includes("headline") && (
              <span
                title="Headline edited by hand — auto-regeneration won't overwrite it"
                className="inline-block h-1 w-1 shrink-0 rounded-full bg-accent"
              />
            )}
          </div>
          {hasReport ? (
            <p className="mt-0.5 truncate text-[12.5px] text-ink-2">{card.headline}</p>
          ) : (
            <p className="mt-0.5 truncate text-[12px] italic text-ink-4">
              No report yet — expand to generate.
            </p>
          )}
        </div>
        {/* Right-aligned stat cluster: sessions · files · cost. Dot-separated,
            one inline-flex group so it scans in one pass (FRONTEND.md pillar 5). */}
        <div className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[11px] tabular-nums text-ink-3">
          <span>{sessionCount} session{sessionCount === 1 ? "" : "s"}</span>
          <span className="text-ink-4">·</span>
          <span>{card.filesTouched} file{card.filesTouched === 1 ? "" : "s"}</span>
          {card.costUsd > 0.05 && (
            <>
              <span className="text-ink-4">·</span>
              <span>{formatCost(card.costUsd)}</span>
            </>
          )}
        </div>
      </button>

      {/* Tier 1 — expanded card. */}
      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            key="body"
            initial={reduceMotion ? false : { height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={reduceMotion ? undefined : { height: 0, opacity: 0 }}
            transition={SPRING}
            className="overflow-hidden"
          >
            <div className="border-t border-border px-4 py-3.5">
              {!hasReport ? (
                <div className="flex items-center justify-between gap-3">
                  <p className="text-[12px] text-ink-3">
                    {sessionCount === 0
                      ? "No sessions in this window."
                      : `${sessionCount} session${sessionCount === 1 ? "" : "s"} ready to summarize.`}
                  </p>
                  <button
                    onClick={onRefresh}
                    disabled={refreshing || sessionCount === 0}
                    className="inline-flex items-center gap-1 rounded-md bg-accent px-2.5 py-1 text-[11.5px] font-medium text-white transition hover:bg-accent-strong disabled:opacity-40"
                  >
                    {refreshing ? <Loader2 size={11} className="animate-spin" /> : <Sparkles size={11} />}
                    Generate
                  </button>
                </div>
              ) : (
                <>
                  {/* Built — the hero section. */}
                  {card.built.length > 0 && (
                    <Section title="Built">
                      <ul className="flex flex-col gap-2">
                        {card.built.map((claim, i) => (
                          <li key={i} className="flex flex-col gap-1">
                            <span className="text-[12.5px] leading-snug text-ink-2">{claim.claim}</span>
                            {claim.evidence.length > 0 && (
                              <div className="flex flex-wrap gap-1">
                                {claim.evidence.map((sid) => (
                                  <EvidenceChip
                                    key={sid}
                                    sessionId={sid}
                                    active={selectedId === sid}
                                    onClick={() => onSelect(sid)}
                                  />
                                ))}
                              </div>
                            )}
                          </li>
                        ))}
                      </ul>
                    </Section>
                  )}

                  {/* How. */}
                  {card.how.length > 0 && (
                    <Section title="How">
                      <ul className="flex flex-col gap-1">
                        {card.how.map((h, i) => (
                          <li key={i} className="text-[12.5px] leading-snug text-ink-2">
                            {h}
                          </li>
                        ))}
                      </ul>
                    </Section>
                  )}

                  {/* Why. */}
                  {card.why.length > 0 && (
                    <Section title="Why">
                      <ul className="flex flex-col gap-1">
                        {card.why.map((w, i) => (
                          <li key={i} className="text-[12.5px] leading-snug text-ink-2">
                            {w}
                          </li>
                        ))}
                      </ul>
                    </Section>
                  )}

                  {/* Desired vs real — the reconciliation table. */}
                  {card.desiredVsReal.length > 0 && (
                    <Section title="Desired vs real">
                      <div className="flex flex-col gap-1.5">
                        {card.desiredVsReal.map((row, i) => (
                          <DvrRow key={i} row={row} />
                        ))}
                      </div>
                    </Section>
                  )}

                  {/* Honest footer: un-digested sessions + refresh + age. */}
                  <div className="mt-3 flex items-center justify-between gap-2 border-t border-border pt-2.5">
                    <div className="flex items-center gap-2 text-[11px] text-ink-4">
                      {card.notDigestedCount > 0 && (
                        <span>
                          {card.notDigestedCount} session{card.notDigestedCount === 1 ? "" : "s"} not yet digested
                        </span>
                      )}
                      {card.stale && (
                        <span className="text-warn">· sources changed — refresh to update</span>
                      )}
                      {card.generatedAt && (
                        <span>{relativeTime(card.generatedAt)}</span>
                      )}
                    </div>
                    <button
                      onClick={onRefresh}
                      disabled={refreshing}
                      title="Regenerate this project's report"
                      className="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-ink-3 transition hover:bg-surface-2 hover:text-ink-2 disabled:opacity-40"
                    >
                      {refreshing ? <Loader2 size={10} className="animate-spin" /> : <RefreshCw size={10} />}
                    </button>
                  </div>
                </>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

// ─── Tier 2 — evidence chip (opens SessionDetail via App) ────────────────────

function EvidenceChip({
  sessionId,
  active,
  onClick,
}: {
  sessionId: string;
  active: boolean;
  onClick: () => void;
}) {
  // Short id for display — session ids are long; show the tail.
  const short = sessionId.length > 10 ? sessionId.slice(-8) : sessionId;
  return (
    <button
      onClick={onClick}
      className={`inline-flex items-center rounded-full px-1.5 py-0.5 font-mono text-[10.5px] transition ${
        active
          ? "bg-accent-soft text-accent-strong"
          : "bg-surface-2 text-ink-3 hover:bg-surface-3 hover:text-ink-2"
      }`}
      title={`Open session ${sessionId}`}
    >
      {short}
    </button>
  );
}

// ─── Small presentational helpers ────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-3 last:mb-0">
      <h3 className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-ink-4">
        {title}
      </h3>
      {children}
    </section>
  );
}

/** Status glyph as a text marker (no emojis). Semantic green/amber/neutral only. */
function DvrRow({ row }: { row: { desired: string; real: string; status: string } }) {
  const { glyph, cls } =
    row.status === "landed"
      ? { glyph: "✓", cls: "bg-positive-soft text-positive" }
      : row.status === "partial"
        ? { glyph: "~", cls: "bg-warn-soft text-warn" }
        : { glyph: "○", cls: "bg-surface-3 text-ink-3" };
  return (
    <div className="flex items-start gap-2">
      <span
        className={`mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[10px] font-semibold ${cls}`}
      >
        {glyph}
      </span>
      <div className="min-w-0 flex-1 text-[12px] leading-snug">
        <span className="text-ink-2">
          <span className="text-ink-3">Wanted:</span> {row.desired || "—"}
        </span>
        <span className="mt-0.5 block text-ink-3">
          <span className="text-ink-4">Got:</span> {row.real || "—"}
        </span>
      </div>
    </div>
  );
}
