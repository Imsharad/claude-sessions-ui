/**
 * Type-safe IPC bindings. Mirror the Rust types in src-tauri/src/lib.rs exactly.
 * Rust structs carry #[serde(rename_all = "camelCase")] so field names match
 * (project_dir on the Rust side ↔ projectDir here). If you add a command in
 * Rust, add it here too.
 */
import { invoke } from "@tauri-apps/api/core";

/**
 * Debug wrapper around invoke — logs every call + result/error to the webview
 * console so IPC issues are visible in devtools (⌥⌘I in the running app).
 * No-op in production builds.
 */
const DEBUG = import.meta.env.DEV;
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const t0 = DEBUG ? performance.now() : 0;
  try {
    const result = await invoke<T>(cmd, args);
    if (DEBUG) {
      const ms = (performance.now() - t0).toFixed(1);
      console.log(`%c[${cmd}]%c ${ms}ms`, "color:#4c7bf5;font-weight:bold", "color:#8a857c", args ?? "");
    }
    return result;
  } catch (e) {
    console.error(`%c[${cmd}] FAILED`, "color:#d6483a;font-weight:bold", args ?? "", e);
    throw e;
  }
}

export interface ScanStats {
  filesSeen: number;
  filesReindexed: number;
  filesSkippedUptodate: number;
  sessionsUpserted: number;
  durationMs: number;
  mode: string;
  error: string | null;
}

export interface SessionFilter {
  projectDir?: string | null;
  query?: string | null;
  sinceDays?: number | null;
  limit?: number | null;
}

export interface SessionCard {
  id: string;
  projectDir: string;
  cwd: string;
  displayProject: string;
  gitBranch: string | null;
  title: string;
  firstTs: string | null;
  lastTs: string | null;
  messageCount: number;
  durationMs: number;
  planMode: boolean;
  hasRecap: boolean;
  recap: string | null;
  inputToks: number;
  outputToks: number;
  cacheReadToks: number;
  costUsd: number;
  costSource: string;
  pinned: boolean;
  // Tag & triage (F3 populates, F2 renders, F4 places). Null = untagged.
  areaOfLife: string | null;
  projectShortName: string | null;
  goalCompleted: boolean | null;
  completionPct: number | null;
  tagRationale: string | null;
  taggedAt: string | null;
  manualFields: string[];
  kanbanStatus: string | null;
  kanbanOrder: number | null;
}

export interface Recap {
  uuid: string;
  capturedTs: string | null;
  content: string;
  seq: number;
  isFinal: boolean;
}

export interface Todo {
  seq: number;
  content: string;
  status: string;
}

export interface ModelUsage {
  model: string;
  inputToks: number;
  outputToks: number;
  cacheCreateToks: number;
  cacheReadToks: number;
  costUsd: number;
  costSource: string | null;
}

export interface SessionDetail {
  card: SessionCard;
  recaps: Recap[];
  todos: Todo[];
  usage: ModelUsage[];
  filesTouched: [string, number][];
  errors: string[];
  turns: [number, number | null, number | null][];
}

export interface RecapHit {
  sessionId: string;
  title: string;
  cwd: string;
  project: string;
  capturedTs: string | null;
  /** Windowed snippet around the best-match region. The frontend highlights
   *  `snippetMatch` between `snippetBefore`/`snippetAfter`. */
  snippetBefore: string;
  snippetMatch: string;
  snippetAfter: string;
  /** Ranker score (higher = more relevant). Surfaced for debug/transparency. */
  score: number;
}

export interface DigestDay {
  day: string;
  sessions: DigestEntry[];
}

export interface DigestEntry {
  sessionId: string;
  title: string;
  cwd: string;
  project: string;
  lastTs: string | null;
  recap: string | null;
  messageCount: number;
}

export interface GlobalStats {
  totalSessions: number;
  totalMessages: number;
  totalInputToks: number;
  totalOutputToks: number;
  totalCacheReadToks: number;
  totalCostUsd: number;
  measuredCostUsd: number;
  estimatedCostUsd: number;
  earliestTs: string | null;
  latestTs: string | null;
  nWithRecap: number;
  nPlanMode: number;
}

export interface IndexStatus {
  lastScanTs: string | null;
  lastScanMode: string | null;
  sessionCount: number;
  recapCount: number;
}

export interface PricingRow {
  model: string;
  inputPerMtok: number;
  outputPerMtok: number;
  cacheWritePerMtok: number;
  cacheReadPerMtok: number;
}

export interface BlacklistEntry {
  pattern: string;
  createdAt: string | null;
  /** Live count of indexed sessions this pattern currently hides. */
  matchCount: number;
}

/** The controlled area-of-life vocabulary (mirrors AREAS_OF_LIFE in lib.rs). */
export const AREAS_OF_LIFE = ["Building", "Research", "Content", "Ops", "Personal"] as const;

/** Tag fields returned by tag_session / update_session_tags. Mirrors
 *  SessionTags in lib.rs. Null = unset. */
export interface SessionTags {
  areaOfLife: string | null;
  projectShortName: string | null;
  goalCompleted: boolean | null;
  completionPct: number | null;
  tagRationale: string | null;
  taggedAt: string | null;
  manualFields: string[];
}

/** Fields a hand-edit can patch (each Some field is validated + flagged manual). */
export interface TagPatch {
  areaOfLife?: string;
  projectShortName?: string;
  goalCompleted?: boolean;
  completionPct?: number;
}

/** Typed error the tag commands reject with — tauri rejects with the serialized
 *  object, so a catch block should be typed as this (branch on `kind`). */
export interface TagError {
  kind: "cli_not_found" | "timeout" | "cli_failed" | "bad_output" | "invalid_json" | "db";
  message: string;
}

/** Coerce an unknown thrown value (tauri reject) into a TagError. */
export function asTagError(e: unknown): TagError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    return e as TagError;
  }
  return { kind: "cli_failed", message: e instanceof Error ? e.message : String(e) };
}

// ─── Command wrappers ──────────────────────────────────────────────────
// Each mirrors a #[tauri::command] in lib.rs. Args are passed as-is; Rust
// deserializes with the same camelCase convention.

export const reindex = (forceFull = false): Promise<ScanStats> =>
  call<ScanStats>("reindex", { forceFull });

export const indexStatus = (): Promise<IndexStatus> =>
  call<IndexStatus>("index_status");

export const listSessions = (filter?: SessionFilter): Promise<SessionCard[]> =>
  call<SessionCard[]>("list_sessions", { filter: filter ?? null });

export const getSessionDetail = (id: string): Promise<SessionDetail> =>
  call<SessionDetail>("get_session_detail", { id });

export const searchRecaps = (query: string): Promise<RecapHit[]> =>
  call<RecapHit[]>("search_recaps", { query });

export const digest = (days = 30): Promise<DigestDay[]> =>
  call<DigestDay[]>("digest", { days });

export const getStats = (): Promise<GlobalStats> =>
  call<GlobalStats>("get_stats");

export const resumeSession = (id: string, fork = false): Promise<void> =>
  call<void>("resume_session", { id, fork });

export const togglePin = (encodedDir: string): Promise<void> =>
  call<void>("toggle_pin", { encodedDir });

export const getPricing = (): Promise<PricingRow[]> =>
  call<PricingRow[]>("get_pricing");

export const setPricing = (rows: PricingRow[]): Promise<void> =>
  call<void>("set_pricing", { rows });

// Mutators return the refreshed list so the UI updates in one round-trip.
export const listBlacklist = (): Promise<BlacklistEntry[]> =>
  call<BlacklistEntry[]>("list_blacklist");

export const addBlacklistPattern = (pattern: string): Promise<BlacklistEntry[]> =>
  call<BlacklistEntry[]>("add_blacklist_pattern", { pattern });

export const removeBlacklistPattern = (pattern: string): Promise<BlacklistEntry[]> =>
  call<BlacklistEntry[]>("remove_blacklist_pattern", { pattern });

// AI tagging (F3). tag_session shells out to the local claude CLI (may take a
// second or two); update_session_tags is a synchronous hand-edit. Both reject
// with a serialized TagError — catch with asTagError().
export const tagSession = (id: string): Promise<SessionTags> =>
  call<SessionTags>("tag_session", { id });

export const updateSessionTags = (id: string, patch: TagPatch): Promise<SessionTags> =>
  call<SessionTags>("update_session_tags", { id, ...patch });
