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
