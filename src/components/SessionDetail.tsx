/**
 * Right pane: session detail.
 * Shows the full recap(s), todos (final state), per-model usage, files
 * touched, errors, and the Resume action (P3).
 *
 * Loads on selection via getSessionDetail; shows a light skeleton while
 * loading. Recap is the hero — set in a comfortable reading width.
 */
import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import {
  Play,
  GitBranch,
  Clock,
  MessageSquare,
  Sparkles,
  CheckCircle2,
  Circle,
  Loader2,
  FileCode,
  AlertTriangle,
  TrendingUp,
  Wand2,
  Tags,
  X,
  ArrowRight,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import type {
  SessionDetail as SessionDetailT,
  ModelUsage,
  Recap,
  SessionTags,
  TagPatch,
  TagError,
} from "../lib/ipc";
import {
  getSessionDetail,
  resumeSession,
  tagSession,
  updateSessionTags,
  asTagError,
  AREAS_OF_LIFE,
  SHOW_AI_TAGGING,
} from "../lib/ipc";
import {
  relativeTime,
  formatTokens,
  formatCost,
  formatDuration,
  timeLabel,
  prettyStatus,
} from "../lib/format";

interface Props {
  sessionId: string | null;
  /** When provided, a close affordance is shown (used by the board overlay). */
  onClose?: () => void;
}

export function SessionDetail({ sessionId, onClose }: Props) {
  const [detail, setDetail] = useState<SessionDetailT | null>(null);
  const [resuming, setResuming] = useState(false);
  const [resumeErr, setResumeErr] = useState<string | null>(null);
  // Split-button fork menu. Fork is per-invocation, never a sticky mode.
  const [forkMenuOpen, setForkMenuOpen] = useState(false);
  const forkMenuRef = useRef<HTMLDivElement>(null);

  // Collapsed earlier recaps (final recap is the always-visible hero).
  const [recapsOpen, setRecapsOpen] = useState(false);

  const [loading, setLoading] = useState(false);

  // Tagging state (F3). Errors carry a `kind` so we can special-case cli_not_found.
  const [tagging, setTagging] = useState(false);
  const [tagErr, setTagErr] = useState<TagError | null>(null);

  useEffect(() => {
    if (!sessionId) return setDetail(null);
    setLoading(true); setResumeErr(null); setTagErr(null); setRecapsOpen(false);
    getSessionDetail(sessionId).then(setDetail).catch(e => console.error(e)).finally(() => setLoading(false));
  }, [sessionId]);

  // Close the fork menu on outside click or Escape.
  useEffect(() => {
    if (!forkMenuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (forkMenuRef.current && !forkMenuRef.current.contains(e.target as Node)) setForkMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setForkMenuOpen(false); };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [forkMenuOpen]);

  const handleResume = async (fork: boolean) => {
    if (!sessionId) return;
    setResuming(true); setResumeErr(null); setForkMenuOpen(false);
    try { await resumeSession(sessionId, fork); }
    catch (e: any) { setResumeErr(e.message || String(e)); }
    finally { setResuming(false); }
  };

  // Fold fresh tag fields into the loaded detail, no reload.
  const applyTags = (t: SessionTags) =>
    setDetail((d) => (d ? { ...d, card: { ...d.card, ...t } } : d));

  const handleTag = async () => {
    if (!sessionId) return;
    setTagging(true); setTagErr(null);
    try { applyTags(await tagSession(sessionId)); }
    catch (e) { setTagErr(asTagError(e)); }
    finally { setTagging(false); }
  };

  // A single hand-edited field commit (change/blur). Updates state from the
  // authoritative response so the manual dot marker appears immediately.
  const commitTag = async (patch: TagPatch) => {
    if (!sessionId) return;
    setTagErr(null);
    try { applyTags(await updateSessionTags(sessionId, patch)); }
    catch (e) { setTagErr(asTagError(e)); }
  };

  if (!sessionId) return <EmptyState />;
  if (loading) return <div className="flex h-full items-center justify-center text-ink-3"><Loader2 size={20} className="animate-spin" /></div>;
  if (!detail) return <EmptyState />;

  const { card, recaps, todos, usage, filesTouched, errors } = detail;
  const totalCost = usage.reduce((sum, u) => sum + u.costUsd, 0);

  const nextAction = card.nextAction ?? null;

  // Recaps: the final recap is the single hero, shown first. Earlier recaps
  // collapse behind a quiet disclosure. If none is final, the most recent is hero.
  const newestFirst = (a: Recap, b: Recap) =>
    (b.capturedTs ?? "").localeCompare(a.capturedTs ?? "") || b.seq - a.seq;
  const heroRecap =
    recaps.find((r) => r.isFinal) ?? [...recaps].sort(newestFirst)[0] ?? null;
  // Stable "Recap N" numbering by chronological order.
  const recapNumber = new Map(
    [...recaps].sort((a, b) => a.seq - b.seq).map((r, i) => [r.uuid, i + 1] as const),
  );
  const earlierRecaps = recaps
    .filter((r) => r.uuid !== heroRecap?.uuid)
    .sort(newestFirst);

  return (
    <div className="flex h-full flex-col bg-surface">
      {/* Top bar: title + resume */}
      <div className="border-b border-border px-5 py-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-[15px] font-semibold text-ink">
              {card.title || "(untitled session)"}
            </h1>
            <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-ink-3">
              <span className="font-medium text-ink-2">{card.displayProject}</span>
              <span className="font-mono">{card.cwd.replace(/^\/Users\/[^/]+/, "~")}</span>
              {card.gitBranch && card.gitBranch !== "HEAD" && (
                <span className="inline-flex items-center gap-1 font-mono">
                  <GitBranch size={10} />
                  {card.gitBranch}
                </span>
              )}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-ink-3">
              <span className="inline-flex items-center gap-1">
                <Clock size={10} />
                {relativeTime(card.lastTs)}
              </span>
              <span className="inline-flex items-center gap-1 tabular-nums">
                <MessageSquare size={10} />
                {card.messageCount} msgs
              </span>
              {card.durationMs > 0 && (
                <span title={`${timeLabel(card.firstTs)} → ${timeLabel(card.lastTs)}`}>
                  {formatDuration(card.durationMs)}
                </span>
              )}
            </div>
          </div>
          {onClose && (
            <button
              onClick={onClose}
              title="Close (Esc)"
              className="-mr-1 -mt-1 shrink-0 rounded-sm p-1.5 text-ink-3 transition hover:bg-surface-2 hover:text-ink-2"
            >
              <X size={15} />
            </button>
          )}
        </div>

        {/* Next action — the single most valuable line. Quiet, above the fold. */}
        {nextAction && (
          <div className="mt-3 flex items-start gap-2 rounded-sm bg-accent-soft/40 px-2.5 py-1.5 ring-1 ring-accent/10">
            <ArrowRight size={12} className="mt-[3px] shrink-0 text-accent" />
            <span className="mt-[2px] shrink-0 text-[10px] font-medium uppercase tracking-wide text-accent">
              Next
            </span>
            <span className="line-clamp-2 text-[12px] leading-snug text-ink-2">
              {nextAction}
            </span>
          </div>
        )}

        {/* Resume action */}
        <div className="mt-3 flex items-center gap-2">
          {/* Split-button: primary resumes in place; the caret opens fork option. */}
          <div ref={forkMenuRef} className="relative inline-flex">
            <button
              onClick={() => handleResume(false)}
              disabled={resuming}
              className="inline-flex items-center gap-1.5 rounded-l-sm bg-accent px-3.5 py-2 text-[12px] font-semibold text-white shadow-sm transition hover:bg-accent-strong disabled:opacity-50"
            >
              {resuming ? (
                <Loader2 size={13} className="animate-spin" />
              ) : (
                <Play size={13} className="fill-current" />
              )}
              Resume in Terminal
            </button>
            <button
              onClick={() => setForkMenuOpen((o) => !o)}
              disabled={resuming}
              aria-label="More resume options"
              aria-expanded={forkMenuOpen}
              className="inline-flex items-center rounded-r-sm border-l border-white/25 bg-accent px-1.5 py-2 text-white shadow-sm transition hover:bg-accent-strong disabled:opacity-50"
            >
              <ChevronDown size={13} />
            </button>
            {forkMenuOpen && (
              <div className="absolute left-0 top-full z-20 mt-1 w-60 overflow-hidden rounded-md border border-border bg-surface p-1 shadow-lg">
                <button
                  onClick={() => handleResume(true)}
                  title="Doesn't mutate the original session"
                  className="flex w-full flex-col items-start gap-0.5 rounded-sm px-2 py-1.5 text-left transition hover:bg-surface-2"
                >
                  <span className="text-[12px] font-medium text-ink">Resume as fork</span>
                  <span className="text-[10.5px] leading-snug text-ink-3">
                    Doesn't mutate the original session
                  </span>
                </button>
              </div>
            )}
          </div>
          {/* Tag session — secondary action (bordered, not a second filled accent). */}
          <button
            onClick={handleTag}
            disabled={tagging}
            title="Classify this session with AI"
            className="inline-flex items-center gap-1.5 rounded-sm border border-border bg-surface px-3 py-2 text-[12px] font-medium text-ink-2 shadow-xs transition hover:border-border-strong hover:text-ink disabled:opacity-50"
          >
            {tagging ? (
              <Loader2 size={13} className="animate-spin text-accent" />
            ) : (
              <Wand2 size={13} />
            )}
            Tag session
          </button>
          {resumeErr && (
            <span className="text-[11px] text-danger">⚠ {resumeErr}</span>
          )}
        </div>

        {/* Quiet tag error line + retry (never a silent failure). */}
        {tagErr && (
          <div className="mt-2 flex items-center gap-2 text-[11px] text-ink-3">
            <span className={tagErr.kind === "cli_not_found" ? "text-ink-2" : "text-danger"}>
              {tagErr.message}
            </span>
            <button
              onClick={handleTag}
              className="rounded-sm px-1 font-medium text-accent transition hover:text-accent-strong"
            >
              Retry
            </button>
          </div>
        )}
      </div>

      {/* Scrollable body */}
      <div className="flex-1 overflow-y-auto px-5 py-4">
        <motion.div
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2 }}
          className="space-y-6"
        >
          {/* Recap(s) — final recap is the hero; earlier ones collapse below. */}
          {heroRecap && (
            <Section icon={<Sparkles size={13} />} title="Recap">
              <div className="space-y-3">
                <div className="rounded-lg bg-accent-soft/40 p-3 text-[13px] leading-relaxed text-ink ring-1 ring-accent/15">
                  {earlierRecaps.length > 0 && (
                    <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-ink-4">
                      {heroRecap.isFinal ? "Final recap" : "Latest recap"}
                    </div>
                  )}
                  <p style={{ maxWidth: "65ch" }}>{heroRecap.content}</p>
                </div>

                {earlierRecaps.length > 0 && (
                  <div>
                    <button
                      onClick={() => setRecapsOpen((o) => !o)}
                      aria-expanded={recapsOpen}
                      className="inline-flex items-center gap-1 text-[11px] font-medium text-ink-3 transition hover:text-ink-2"
                    >
                      <ChevronRight
                        size={12}
                        className={`transition-transform ${recapsOpen ? "rotate-90" : ""}`}
                      />
                      {earlierRecaps.length} earlier {earlierRecaps.length === 1 ? "recap" : "recaps"}
                    </button>
                    {recapsOpen && (
                      <div className="mt-2 space-y-3">
                        {earlierRecaps.map((r) => (
                          <div
                            key={r.uuid}
                            className="rounded-lg bg-surface-2 p-3 text-[13px] leading-relaxed text-ink-2"
                          >
                            <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-ink-4">
                              Recap {recapNumber.get(r.uuid)} · {timeLabel(r.capturedTs)}
                            </div>
                            <p style={{ maxWidth: "65ch" }}>{r.content}</p>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </Section>
          )}

          {/* Tags — subordinate garnish to the recap, editable inline. */}
          <Section icon={<Tags size={13} />} title="Tags">
            <TagsBody
              card={card}
              tagging={tagging}
              onTag={handleTag}
              onCommit={commitTag}
            />
          </Section>

          {/* Todos */}
          {todos.length > 0 && (
            <Section icon={<CheckCircle2 size={13} />} title={`Todos (${todos.length})`}>
              <ul className="space-y-1">
                {todos.map((t) => (
                  <li
                    key={t.seq}
                    className="flex items-start gap-2 text-[13px]"
                  >
                    {t.status === "completed" ? (
                      <CheckCircle2 size={14} className="mt-0.5 shrink-0 text-positive" />
                    ) : (
                      <Circle size={14} className="mt-0.5 shrink-0 text-ink-4" />
                    )}
                    <span
                      className={
                        t.status === "completed"
                          ? "text-ink-3 line-through"
                          : "text-ink-2"
                      }
                    >
                      {t.content}
                    </span>
                    {t.status !== "completed" && t.status !== "pending" && (
                      <span className="ml-auto rounded-full bg-warn-soft px-1.5 py-0.5 text-[10px] text-warn">
                        {prettyStatus(t.status)}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </Section>
          )}

          {/* Usage / cost */}
          {usage.length > 0 && (
            <Section
              icon={<TrendingUp size={13} />}
              title={`Usage${totalCost > 0 ? ` · ${formatCost(totalCost)}` : ""}`}
            >
              {usage.length === 1 ? (
                <UsageSummaryLine u={usage[0]} />
              ) : (
              <div className="overflow-hidden rounded-lg border border-border">
                <table className="w-full text-[12px]">
                  <thead className="bg-surface-2 text-[10px] uppercase tracking-wide text-ink-3">
                    <tr>
                      <th className="px-3 py-1.5 text-left font-medium">Model</th>
                      <th className="px-3 py-1.5 text-right font-medium">In</th>
                      <th className="px-3 py-1.5 text-right font-medium">Out</th>
                      <th className="px-3 py-1.5 text-right font-medium">Cache read</th>
                      <th className="px-3 py-1.5 text-right font-medium">Cost</th>
                    </tr>
                  </thead>
                  <tbody>
                    {usage.map((u) => (
                      <tr key={u.model} className="border-t border-border">
                        <td className="px-3 py-1.5 font-mono text-ink-2">{u.model}</td>
                        <td className="px-3 py-1.5 text-right font-mono tabular-nums text-ink-3">
                          {formatTokens(u.inputToks)}
                        </td>
                        <td className="px-3 py-1.5 text-right font-mono tabular-nums text-ink-3">
                          {formatTokens(u.outputToks)}
                        </td>
                        <td className="px-3 py-1.5 text-right font-mono tabular-nums text-ink-3">
                          {formatTokens(u.cacheReadToks)}
                        </td>
                        <td className="px-3 py-1.5 text-right font-mono tabular-nums text-ink-2">
                          {formatCost(u.costUsd)}
                          {u.costSource === "estimate" && (
                            <span className="ml-1 text-[9px] text-ink-4">est</span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              )}
            </Section>
          )}

          {/* Files touched */}
          {filesTouched.length > 0 && (
            <Section icon={<FileCode size={13} />} title={`Files touched (${filesTouched.length})`}>
              <ul className="space-y-0.5">
                {filesTouched.slice(0, 30).map(([path, count]) => (
                  <li
                    key={path}
                    className="flex items-center gap-2 truncate font-mono text-[11px] text-ink-3"
                  >
                    <span className="truncate">{path}</span>
                    {count > 1 && (
                      <span className="shrink-0 rounded-full bg-surface-3 px-1.5 text-[9px] tabular-nums">
                        ×{count}
                      </span>
                    )}
                  </li>
                ))}
                {filesTouched.length > 30 && (
                  <li className="text-[11px] text-ink-4">
                    + {filesTouched.length - 30} more
                  </li>
                )}
              </ul>
            </Section>
          )}

          {/* Errors / friction */}
          {errors.length > 0 && (
            <Section
              icon={<AlertTriangle size={13} />}
              title={`Friction (${errors.length})`}
            >
              <ul className="space-y-1">
                {errors.slice(0, 10).map((e, i) => (
                  <li key={i} className="rounded-sm bg-danger-soft px-2 py-1 text-[11px] text-danger">
                    {e}
                  </li>
                ))}
              </ul>
            </Section>
          )}
        </motion.div>
      </div>
    </div>
  );
}

/** Single-model usage: one dense summary line instead of a 5-column table. */
function UsageSummaryLine({ u }: { u: ModelUsage }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-[12px]">
      <span className="font-mono text-ink-2">{u.model}</span>
      <span className="font-mono tabular-nums text-ink-3">
        {formatTokens(u.inputToks)} in · {formatTokens(u.outputToks)} out ·{" "}
        {formatTokens(u.cacheReadToks)} cache · {formatCost(u.costUsd)}
        {u.costSource === "estimate" && (
          <span className="ml-1 text-[9px] text-ink-4">est</span>
        )}
      </span>
    </div>
  );
}

/** 4px accent dot marking a hand-edited field (auto-tag will not overwrite it).
 *  A quiet marker, never a banner. */
function ManualDot({ show }: { show: boolean }) {
  if (!show) return null;
  return (
    <span
      title="Edited by hand — auto-tag will not overwrite"
      className="inline-block h-1 w-1 shrink-0 rounded-full bg-accent"
    />
  );
}

function FieldLabel({ children, manual }: { children: React.ReactNode; manual: boolean }) {
  return (
    <span className="inline-flex items-center gap-1 text-[10px] font-medium uppercase tracking-wide text-ink-4">
      {children}
      <ManualDot show={manual} />
    </span>
  );
}

/** A small muted text button (Edit / Edit tags / Done). */
function TextButton({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className="text-[11px] font-medium text-ink-3 transition hover:text-accent"
    >
      {children}
    </button>
  );
}

/** A compact read-view chip for a set tag field, with the manual-dot marker. */
function TagChip({
  children,
  manual,
  className = "",
}: {
  children: React.ReactNode;
  manual: boolean;
  className?: string;
}) {
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full bg-surface-2 px-2 py-0.5 text-[11px] text-ink-2 ${className}`}
    >
      {children}
      <ManualDot show={manual} />
    </span>
  );
}

/** The Tags body. Untagged → a single muted "Not tagged" line + "Edit tags".
 *  Tagged → compact read chips + "Edit". Either opens the editable form, where
 *  each commit calls updateSessionTags and the parent folds the response back. */
function TagsBody({
  card,
  tagging,
  onTag,
  onCommit,
}: {
  card: SessionTags & { taggedAt: string | null };
  tagging: boolean;
  onTag: () => void;
  onCommit: (patch: TagPatch) => void;
}) {
  const [editing, setEditing] = useState(false);
  const manual = card.manualFields ?? [];
  const isManual = (f: string) => manual.includes(f);
  const tagged =
    card.taggedAt != null ||
    card.areaOfLife != null ||
    card.projectShortName != null ||
    card.completionPct != null ||
    card.goalCompleted != null;

  // Read view — no empty scaffolding until the user chooses to edit.
  if (!editing) {
    if (!tagged) {
      return (
        <div className="flex items-center gap-2 text-[11.5px] text-ink-4">
          <span>Not tagged</span>
          <TextButton onClick={() => setEditing(true)}>Edit tags</TextButton>
        </div>
      );
    }
    return (
      <div className="space-y-2">
        <div className="flex flex-wrap items-center gap-1.5">
          {card.areaOfLife != null && (
            <TagChip manual={isManual("areaOfLife")}>{card.areaOfLife}</TagChip>
          )}
          {card.projectShortName != null && (
            <TagChip manual={isManual("projectShortName")} className="font-mono">
              {card.projectShortName}
            </TagChip>
          )}
          {card.completionPct != null && (
            <TagChip manual={isManual("completionPct")} className="tabular-nums">
              {card.completionPct}%
            </TagChip>
          )}
          {card.goalCompleted === true && (
            <TagChip manual={isManual("goalCompleted")}>
              <CheckCircle2 size={11} className="text-positive" />
              Goal done
            </TagChip>
          )}
          <TextButton onClick={() => setEditing(true)}>Edit</TextButton>
        </div>
        {card.tagRationale && (
          <p className="text-[11.5px] italic leading-snug text-ink-3" style={{ maxWidth: "60ch" }}>
            {card.tagRationale}
          </p>
        )}
      </div>
    );
  }

  // Edit view — the existing editable select/input/number/checkbox form.
  return (
    <motion.div
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className="space-y-2.5"
    >
      <div className="flex items-center gap-2">
        {!tagged && (
          <p className="text-[11.5px] text-ink-4">Not tagged yet — set the fields below.</p>
        )}
        <span className="ml-auto">
          <TextButton onClick={() => setEditing(false)}>Done</TextButton>
        </span>
      </div>
      <div className="flex flex-wrap items-start gap-x-6 gap-y-2.5">
        {/* Area */}
        <label className="flex flex-col gap-1">
          <FieldLabel manual={isManual("areaOfLife")}>Area</FieldLabel>
          <select
            value={card.areaOfLife ?? ""}
            onChange={(e) => onCommit({ areaOfLife: e.target.value })}
            className="rounded-sm border border-border bg-surface px-2 py-1 text-[12px] text-ink-2 transition hover:border-border-strong focus:border-accent focus:outline-none"
          >
            {card.areaOfLife == null && <option value="">—</option>}
            {AREAS_OF_LIFE.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </label>

        {/* Short name */}
        <label className="flex flex-col gap-1">
          <FieldLabel manual={isManual("projectShortName")}>Short name</FieldLabel>
          <input
            key={`name-${card.projectShortName ?? ""}`}
            type="text"
            defaultValue={card.projectShortName ?? ""}
            maxLength={24}
            onBlur={(e) => {
              const v = e.target.value.trim();
              if (v && v !== (card.projectShortName ?? "")) onCommit({ projectShortName: v });
            }}
            className="w-40 rounded-sm border border-border bg-surface px-2 py-1 text-[12px] text-ink-2 transition hover:border-border-strong focus:border-accent focus:outline-none"
          />
        </label>

        {/* Completion % */}
        <label className="flex flex-col gap-1">
          <FieldLabel manual={isManual("completionPct")}>Completion %</FieldLabel>
          <input
            key={`pct-${card.completionPct ?? ""}`}
            type="number"
            min={0}
            max={100}
            defaultValue={card.completionPct ?? ""}
            onBlur={(e) => {
              // Empty ≠ 0: absence stays absent. Only commit a real entry.
              const raw = e.target.value.trim();
              if (raw === "") return;
              const n = Math.max(0, Math.min(100, Math.round(Number(raw) || 0)));
              if (n !== card.completionPct) onCommit({ completionPct: n });
            }}
            className="w-20 rounded-sm border border-border bg-surface px-2 py-1 text-[12px] tabular-nums text-ink-2 transition hover:border-border-strong focus:border-accent focus:outline-none"
          />
        </label>

        {/* Goal completed */}
        <label className="flex flex-col gap-1">
          <FieldLabel manual={isManual("goalCompleted")}>Goal done</FieldLabel>
          <span className="inline-flex h-[27px] items-center">
            <input
              type="checkbox"
              checked={card.goalCompleted === true}
              onChange={(e) => onCommit({ goalCompleted: e.target.checked })}
              className="accent-accent"
            />
          </span>
        </label>
      </div>

      {card.tagRationale && (
        <p className="text-[11.5px] italic leading-snug text-ink-3" style={{ maxWidth: "60ch" }}>
          {card.tagRationale}
        </p>
      )}

      {SHOW_AI_TAGGING && (
        <button
          onClick={onTag}
          disabled={tagging}
          title="Re-run AI tagging (overwrites auto fields only)"
          className="inline-flex items-center gap-1 text-[11px] font-medium text-ink-3 transition hover:text-accent disabled:opacity-50"
        >
          {tagging ? <Loader2 size={11} className="animate-spin text-accent" /> : <Wand2 size={11} />}
          Re-tag
        </button>
      )}
    </motion.div>
  );
}

function Section({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wide text-ink-3"><span className="text-accent">{icon}</span>{title}</h3>
      {children}
    </section>
  );
}

function EmptyState() {
  const hints = [ { keys: ["↑", "↓"], label: "Navigate sessions" }, { keys: ["⏎"], label: "Open in detail" }, { keys: ["⌘", "K"], label: "Search (coming)" }, { keys: ["⌘", "⏎"], label: "Resume in Terminal (coming)" } ];
  return (
    <div className="flex h-full flex-1 items-center justify-center bg-surface px-6">
      <div className="w-full max-w-xs">
        <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-xl bg-accent-soft"><Sparkles size={20} className="text-accent" /></div>
        <p className="text-[15px] font-semibold text-ink">Select a session</p>
        <p className="mt-1 text-[12.5px] leading-relaxed text-ink-3">Click any session to read its recap, todos, and usage — then resume it in Terminal.</p>
        <div className="mt-4 space-y-1.5 border-t border-border pt-4 text-[11px] text-ink-3">
          {hints.map((h, i) => (
            <div key={i} className="flex items-center justify-between">
              <span className="text-ink-3">{h.label}</span>
              <span className="flex gap-1">{h.keys.map(k => <kbd key={k} className="rounded-sm border border-border bg-surface-2 px-1.5 py-0.5 font-mono text-[10px] text-ink-2 shadow-xs">{k}</kbd>)}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
