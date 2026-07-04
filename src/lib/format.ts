/**
 * Display formatters. Centralized so cards/stats stay consistent.
 */
import { formatDistanceToNow, format } from "date-fns";

/** "2h ago", "yesterday" — relative time for card timestamps. */
export function relativeTime(iso: string | null): string {
  if (!iso) return "—";
  try { return formatDistanceToNow(new Date(iso), { addSuffix: true }); }
  catch { return "—"; }
}

/** "2:14 PM" — time-only */
export function timeLabel(iso: string | null): string {
  if (!iso) return "—";
  try { return format(new Date(iso), "h:mm a"); }
  catch { return "—"; }
}

/** 12,345 → "12.3K" / "1.2M" / "5.2B" — token scale. */
export function formatTokens(n: number): string {
  if (n >= 1e9) return (n / 1e9).toFixed(2) + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + "K";
  return String(n);
}

/** Cost in dollars with appropriate precision. */
export function formatCost(usd: number): string {
  if (usd >= 100) return "$" + usd.toFixed(0);
  if (usd >= 0.01) return "$" + usd.toFixed(2);
  if (usd > 0) return "<$0.01";
  return "$0";
}

/** Duration ms → "3m 12s" / "1h 4m" / "45s". */
export function formatDuration(ms: number): string {
  if (!ms || ms < 0) return "—";
  const s = Math.round(ms / 1000), m = Math.floor(s / 60), h = Math.floor(m / 60);
  if (s < 60) return `${s}s`;
  if (m < 60) return s % 60 ? `${m}m ${s % 60}s` : `${m}m`;
  return m % 60 ? `${h}h ${m % 60}m` : `${h}h`;
}

/** Shorten a project cwd for compact display: ".../NOW/brain" */
export function shortCwd(cwd: string): string {
  const parts = (cwd || "").replace(/^\/Users\/[^/]+/, "~").split("/");
  return parts.length <= 3 ? parts.join("/") : "…/" + parts.slice(-2).join("/");
}

/** Truncate text to N chars at a word boundary, with ellipsis. */
export function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const lastSpace = cut.lastIndexOf(" ");
  return (lastSpace > max * 0.6 ? cut.slice(0, lastSpace) : cut) + "…";
}

/** Title-case a status: "in_progress" → "In Progress" */
export function prettyStatus(status: string): string {
  return status.split("_").map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
}
