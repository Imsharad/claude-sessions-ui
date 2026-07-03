/**
 * Type-safe IPC bindings. Mirror the Rust types in src-tauri/src/lib.rs exactly —
 * serde camelCases on the wire (Tauri default), so Rust `project_dir` becomes
 * `projectDir` here. If you add a command in Rust, add it here too.
 */
import { invoke } from "@tauri-apps/api/core";

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
  capturedTs: string | null;
  snippet: string;
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
// Each mirrors a #[tauri::command] in lib.rs. invoke() camelCases the arg names.

export const reindex = (forceFull = false): Promise<ScanStats> =>
  invoke<ScanStats>("reindex", { forceFull });

export const indexStatus = (): Promise<IndexStatus> =>
  invoke<IndexStatus>("index_status");

export const listSessions = (filter?: SessionFilter): Promise<SessionCard[]> =>
  invoke<SessionCard[]>("list_sessions", { filter: filter ?? null });

export const getSessionDetail = (id: string): Promise<SessionDetail> =>
  invoke<SessionDetail>("get_session_detail", { id });

export const searchRecaps = (query: string): Promise<RecapHit[]> =>
  invoke<RecapHit[]>("search_recaps", { query });

export const digest = (days = 30): Promise<DigestDay[]> =>
  invoke<DigestDay[]>("digest", { days });

export const getStats = (): Promise<GlobalStats> =>
  invoke<GlobalStats>("get_stats");

export const resumeSession = (id: string, fork = false): Promise<void> =>
  invoke<void>("resume_session", { id, fork });

export const togglePin = (encodedDir: string): Promise<void> =>
  invoke<void>("toggle_pin", { encodedDir });

export const getPricing = (): Promise<PricingRow[]> =>
  invoke<PricingRow[]>("get_pricing");

export const setPricing = (rows: PricingRow[]): Promise<void> =>
  invoke<void>("set_pricing", { rows });
