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
} from "lucide-react";
import type { SessionDetail as SessionDetailT } from "../lib/ipc";
import { getSessionDetail, resumeSession } from "../lib/ipc";
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
  const [loading, setLoading] = useState(false);
  const [resumeFork, setResumeFork] = useState(false);
  const [resuming, setResuming] = useState(false);
  const [resumeErr, setResumeErr] = useState<string | null>(null);

  useEffect(() => {
    if (!sessionId) {
      setDetail(null);
      return;
    }
    setLoading(true);
    setResumeErr(null);
    getSessionDetail(sessionId)
      .then((d) => setDetail(d))
      .catch((e) => console.error("detail load failed", e))
      .finally(() => setLoading(false));
  }, [sessionId]);

  async function handleResume() {
    if (!sessionId) return;
    setResuming(true);
    setResumeErr(null);
    try {
      await resumeSession(sessionId, resumeFork);
    } catch (e: unknown) {
      setResumeErr(e instanceof Error ? e.message : String(e));
    } finally {
      setResuming(false);
    }
  }

  if (!sessionId) {
    return <EmptyState />;
  }
  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-ink-3">
        <Loader2 size={20} className="animate-spin" />
      </div>
    );
  }
  if (!detail) {
    return <EmptyState />;
  }

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
            className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-2 text-[12px] font-semibold text-white shadow-sm transition hover:bg-accent-strong disabled:opacity-50"
          >
            {resuming ? (
              <Loader2 size={13} className="animate-spin" />
            ) : (
              <Play size={13} className="fill-current" />
            )}
            Resume in Terminal
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
                  <li key={i} className="rounded-md bg-danger-soft px-2 py-1 text-[11px] text-danger">
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

function Section({
  icon,
  title,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <h3 className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wide text-ink-3">
        <span className="text-accent">{icon}</span>
        {title}
      </h3>
      {children}
    </section>
  );
}

function EmptyState() {
  return (
    <div className="flex h-full flex-1 items-center justify-center bg-surface">
      <div className="max-w-xs text-center">
        <Sparkles size={28} className="mx-auto mb-3 text-ink-4" />
        <p className="text-[13px] font-medium text-ink-2">Select a session</p>
        <p className="mt-1 text-[12px] text-ink-3">
          Pick a session from the list to see its recap, todos, usage, and
          files — then resume it in Terminal.
        </p>
      </div>
    </div>
  );
}
