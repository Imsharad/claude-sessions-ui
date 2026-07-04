/**
 * Right pane: session detail.
 * Shows the full recap(s), todos (final state), per-model usage, files
 * touched, errors, and the Resume action (P3).
 *
 * Loads on selection via getSessionDetail; shows a light skeleton while
 * loading. Recap is the hero — set in a comfortable reading width.
 */
import { useEffect, useState } from "react";
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
} from "lucide-react";
import type {
  SessionDetail as SessionDetailT,
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
}

export function SessionDetail({ sessionId }: Props) {
  const [detail, setDetail] = useState<SessionDetailT | null>(null);
  const [resumeFork, setResumeFork] = useState(false);
  const [resuming, setResuming] = useState(false);
  const [resumeErr, setResumeErr] = useState<string | null>(null);

  const [loading, setLoading] = useState(false);

  // Tagging state (F3). Errors carry a `kind` so we can special-case cli_not_found.
  const [tagging, setTagging] = useState(false);
  const [tagErr, setTagErr] = useState<TagError | null>(null);

  useEffect(() => {
    if (!sessionId) return setDetail(null);
    setLoading(true); setResumeErr(null); setTagErr(null);
    getSessionDetail(sessionId).then(setDetail).catch(e => console.error(e)).finally(() => setLoading(false));
  }, [sessionId]);

  const handleResume = async () => {
    if (!sessionId) return;
    setResuming(true); setResumeErr(null);
    try { await resumeSession(sessionId, resumeFork); }
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
              {card.gitBranch && (
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
              {card.durationMs > 0 && <span>{formatDuration(card.durationMs)}</span>}
              <span className="font-mono text-[10px] text-ink-4">
                {timeLabel(card.firstTs)} → {timeLabel(card.lastTs)}
              </span>
            </div>
          </div>
        </div>

        {/* Resume action */}
        <div className="mt-3 flex items-center gap-2">
          <button
            onClick={handleResume}
            disabled={resuming}
            className="inline-flex items-center gap-1.5 rounded-sm bg-accent px-3.5 py-2 text-[12px] font-semibold text-white shadow-sm transition hover:bg-accent-strong disabled:opacity-50"
          >
            {resuming ? (
              <Loader2 size={13} className="animate-spin" />
            ) : (
              <Play size={13} className="fill-current" />
            )}
            Resume in Terminal
          </button>
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
          <label className="inline-flex cursor-pointer items-center gap-1.5 text-[11px] text-ink-3">
            <input
              type="checkbox"
              checked={resumeFork}
              onChange={(e) => setResumeFork(e.target.checked)}
              className="accent-accent"
            />
            Fork (don't mutate original)
          </label>
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
          {/* Recap(s) — the hero */}
          {recaps.length > 0 && (
            <Section icon={<Sparkles size={13} />} title="Recap">
              <div className="space-y-3">
                {recaps.map((r, i) => (
                  <div
                    key={r.uuid}
                    className={`rounded-lg p-3 text-[13px] leading-relaxed ${
                      r.isFinal
                        ? "bg-accent-soft/40 text-ink ring-1 ring-accent/15"
                        : "bg-surface-2 text-ink-2"
                    }`}
                  >
                    {recaps.length > 1 && (
                      <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-ink-4">
                        {r.isFinal ? "Final recap" : `Recap ${i + 1}`} · {relativeTime(r.capturedTs)}
                      </div>
                    )}
                    <p style={{ maxWidth: "65ch" }}>{r.content}</p>
                  </div>
                ))}
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

/** The editable Tags body. Absent tags → the tag-session prompt, no empty
 *  scaffolding. Present → editable select/input/number/checkbox, each commit
 *  calls updateSessionTags and the parent folds the response back into state. */
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
  const manual = card.manualFields ?? [];
  const isManual = (f: string) => manual.includes(f);
  const tagged =
    card.taggedAt != null ||
    card.areaOfLife != null ||
    card.projectShortName != null ||
    card.completionPct != null ||
    card.goalCompleted != null;

  if (!tagged) {
    return (
      <div className="flex items-center gap-2 text-[12px] text-ink-3">
        <span>Not tagged yet.</span>
        <button
          onClick={onTag}
          disabled={tagging}
          className="inline-flex items-center gap-1 font-medium text-accent transition hover:text-accent-strong disabled:opacity-50"
        >
          {tagging ? <Loader2 size={12} className="animate-spin" /> : <Wand2 size={12} />}
          Tag it
        </button>
      </div>
    );
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className="space-y-2.5"
    >
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
            key={`pct-${card.completionPct ?? 0}`}
            type="number"
            min={0}
            max={100}
            defaultValue={card.completionPct ?? 0}
            onBlur={(e) => {
              const n = Math.max(0, Math.min(100, Math.round(Number(e.target.value) || 0)));
              if (n !== (card.completionPct ?? 0)) onCommit({ completionPct: n });
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

      <button
        onClick={onTag}
        disabled={tagging}
        title="Re-run AI tagging (overwrites auto fields only)"
        className="inline-flex items-center gap-1 text-[11px] font-medium text-ink-3 transition hover:text-accent disabled:opacity-50"
      >
        {tagging ? <Loader2 size={11} className="animate-spin text-accent" /> : <Wand2 size={11} />}
        Re-tag
      </button>
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
