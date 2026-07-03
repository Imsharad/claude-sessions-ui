/**
 * Display formatters. Centralized so cards/stats stay consistent.
 */
import {
  formatDistanceToNow,
  format,
  isToday,
  isYesterday,
  differenceInDays,
} from "date-fns";

/** "2h ago", "yesterday" — relative time for card timestamps. */
export function relativeTime(iso: string | null): string {
  if (!iso) return "—";
  try {
    return formatDistanceToNow(new Date(iso), { addSuffix: true });
  } catch {
    return "—";
  }
}

/**
 * "Today, 2:14 PM" / "Yesterday" / "Jun 14" — for digest grouping + headers.
 * More readable than relative for anything older than a day.
 */
export function dayLabel(iso: string | null): string {
  if (!iso) return "Unknown day";
  try {
    const d = new Date(iso);
    if (isToday(d)) return "Today";
    if (isYesterday(d)) return "Yesterday";
    const days = differenceInDays(new Date(), d);
    if (days < 7) return format(d, "EEEE"); // "Tuesday"
    return format(d, "MMM d"); // "Jun 14"
  } catch {
    return "—";
  }
}

/** "2:14 PM" — time-only for digest entries. */
export function timeLabel(iso: string | null): string {
  if (!iso) return "—";
  try {
    return format(new Date(iso), "h:mm a");
  } catch {
    return "—";
  }
}

/** 12,345 → "12.3K" / "1.2M" / "5.2B" — token scale. */
export function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + "B";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return n.toString();
}

/** Cost in dollars with appropriate precision. */
export function formatCost(usd: number): string {
  if (usd >= 100) return "$" + usd.toFixed(0);
  if (usd >= 1) return "$" + usd.toFixed(2);
  if (usd >= 0.01) return "$" + usd.toFixed(2);
  if (usd > 0) return "<$0.01";
  return "$0";
}

/** Duration ms → "3m 12s" / "1h 4m" / "45s". */
export function formatDuration(ms: number): string {
  if (!ms || ms < 0) return "—";
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rs = s % 60;
  if (m < 60) return rs ? `${m}m ${rs}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm ? `${h}h ${rm}m` : `${h}h`;
}

/** Shorten a project cwd for compact display: ".../NOW/brain" */
export function shortCwd(cwd: string): string {
  if (!cwd) return "";
  const parts = cwd.replace(/^\/Users\/[^/]+/, "~").split("/");
  if (parts.length <= 3) return parts.join("/");
  return "…/" + parts.slice(-2).join("/");
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
  return status
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/**
 * Clean a session title for display. aiTitle is usually clean, but first-prompt
 * fallbacks can leak raw prompt content: XML tags, markdown headers, file
 * references. Produce something a human would recognize as a title.
 *
 *   "<instructions><references><file>@criteria"  →  "instructions references"
 *   "## Task: do X"                              →  "Task: do X"
 *   "Fix the bug\n\nMore text"                   →  "Fix the bug"
 */
export function sanitizeTitle(raw: string): string {
  if (!raw) return "";
  let s = raw.trim();
  // Strip XML/HTML tags but keep their inner text (so <instructions> → instructions).
  s = s.replace(/<[^>]+>/g, " ");
  // Strip markdown headers / emphasis markers.
  s = s.replace(/^#{1,6}\s*/, "").replace(/[*_`]/g, "");
  // Collapse whitespace and newlines to single spaces.
  s = s.replace(/\s+/g, " ").trim();
  // Drop leading file-reference / slash-command noise.
  s = s.replace(/^[@/\\]+\S*\s*/, "").trim();
  // If it's now empty or too short, leave it (caller falls back).
  return s;
}

/**
 * Resolve the best display title for a session, with graceful fallback.
 * Priority: clean aiTitle → clean lastPrompt → clean firstUserMsg → "(untitled)".
 * Never returns prompt-content garbage.
 */
export function resolveTitle(opts: {
  title?: string | null;
  lastPrompt?: string | null;
  firstUserMsg?: string | null;
}): string {
  for (const candidate of [opts.title, opts.lastPrompt, opts.firstUserMsg]) {
    if (!candidate) continue;
    const clean = sanitizeTitle(candidate);
    if (clean.length >= 3) return truncate(clean, 120);
  }
  return "(untitled session)";
}
