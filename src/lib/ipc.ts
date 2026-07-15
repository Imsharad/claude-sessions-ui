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
  /** Lifecycle status of this session's project, derived override-wins from the
   *  `projects` table (active | labs | archived | inbox). Archived sessions are
   *  filtered out of Launcher/Home/Digest by default; the sidebar groups by it. */
  projectStatus: string | null;
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
  /** The session's stated next step, derived at read time from the final recap's
   *  text. Only get_session_detail populates it (detail.card.nextAction); the list
   *  query leaves it null. Null when no next-step marker is found. */
  nextAction: string | null;
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

// ─── Timeline digest (P6) ─────────────────────────────────────────────
// A read-only weekly reconstruction surface. get_timeline is instant (no LLM);
// per-session digests are generate-or-cached over the same TagError channel as
// tagging (asTagError()). Manual edits durably override generated fields.

/** One session's digest row. Renders truthfully even when generated: `verified`
 *  / `confidence` drive quiet-vs-confident styling, `stale` flags a source that
 *  changed after generation, `manualFields` lists hand-edited fields (accent-dot
 *  marked, never overwritten by regeneration). */
export interface SessionDigest {
  sessionId: string;
  workedOn: string;
  outcome: string;
  openLoops: string[];
  citations: string[];
  verified: boolean;
  confidence: number | null;
  model: string | null;
  promptVersion: number;
  generatedAt: string | null;
  stale: boolean;
  manualFields: string[];
}

/** One day in the window. Gap days arrive as sessionCount 0 with empty
 *  sessionIds — a truthful zero, never invented activity. */
export interface TimelineDay {
  date: string; // YYYY-MM-DD
  sessionCount: number;
  sessionIds: string[];
}

/** A multi-day workstream linking sessions by arc. */
export interface Thread {
  id: string;
  arc: string;
  memberSessionIds: string[];
  generatedAt: string | null;
}

/** Everything the timeline needs in one round-trip: the day skeleton, the
 *  digests keyed by sessionId, and the linked threads. `metaSessionIds` carries
 *  harness/self sessions (the triage + digest calls this feature spawns) —
 *  excluded from days/totals, surfaced in a separate collapsed "app activity"
 *  group. Empty when the window has none. */
export interface TimelineResponse {
  days: TimelineDay[];
  digests: Record<string, SessionDigest>;
  threads: Thread[];
  metaSessionIds: string[];
}

/** Result of a batch backfill over the window. */
export interface DigestBatchReport {
  generated: number;
  cached: number;
  failed: number;
  skippedNoRecap: number;
}

/** Fields a hand-edit can patch on a digest (each becomes durable/manual). */
export interface DigestPatch {
  workedOn?: string;
  outcome?: string;
  openLoops?: string[];
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
  /**
   * Honest live count of sessions this pattern hides: indexed-but-filtered rows
   * plus files skipped at scan time (which never entered the sessions table).
   */
  matchCount: number;
}

/** What a candidate pattern would hide, over indexed sessions only. Mirrors
 *  BlacklistPreview in lib.rs. */
export interface BlacklistPreview {
  projectCount: number;
  sessionCount: number;
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
  kind:
    | "cli_not_found"
    | "timeout"
    | "cli_failed"
    | "bad_output"
    | "invalid_json"
    | "db"
    | "no_recap"
    | "no_api_key"
    | "api_http"
    | "rate_limited"
    // Forward-compat: an unrecognized kind still carries a message and must
    // degrade to rendering it, never a blank.
    | (string & {});
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

/** One row of the full project list (incl. archived), for the sidebar's
 *  status-grouped view. Mirrors ProjectEntry in lib.rs. */
export interface ProjectEntry {
  encodedDir: string;
  cwd: string;
  displayName: string;
  sessionCount: number;
  pinned: boolean;
  lastTs: string | null;
  status: string | null;
}

/** Full project list including archived (which list_sessions filters out), so
 *  the sidebar can surface + un-archive them. Sorted: pinned first, then by
 *  last activity desc. */
export const listProjects = (): Promise<ProjectEntry[]> =>
  call<ProjectEntry[]>("list_projects");

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

export const previewBlacklistPattern = (pattern: string): Promise<BlacklistPreview> =>
  call<BlacklistPreview>("preview_blacklist_pattern", { pattern });

// Feature flag: AI tagging is PARKED. Manual keyboard triage (TriageMode) is the
// primary path — zero latency, zero cost. Flip to true to resurface the Wand2
// auto-tag actions and the backfill command; the code paths stay wired underneath.
export const SHOW_AI_TAGGING = false;

// AI tagging (F3). tag_session hits the direct Anthropic API when ANTHROPIC_API_KEY
// is set (~1s), else falls back to the local claude CLI (~11s); update_session_tags
// is a synchronous hand-edit. Both reject with a serialized TagError — catch with
// asTagError().
export const tagSession = (id: string): Promise<SessionTags> =>
  call<SessionTags>("tag_session", { id });

export const updateSessionTags = (id: string, patch: TagPatch): Promise<SessionTags> =>
  call<SessionTags>("update_session_tags", { id, ...patch });

// Bulk backfill (F3b). Tags every untagged session concurrently over the API path;
// resumable. A no-op (skippedNoKey: true) with no API key. Subscribe to the
// "backfill_progress" event ([done, total]) for a progress bar.
export interface BackfillReport {
  tagged: number;
  failed: number;
  skippedNoKey: boolean;
}

export const backfillTags = (): Promise<BackfillReport> =>
  call<BackfillReport>("backfill_tags", {});

// Kanban board (F4). The three columns; a drag sets an explicit status override
// (null clears it, falling back to the %-derived column) plus a per-column order.
export type KanbanStatus = "planned" | "in_progress" | "completed";

export const setKanban = (
  id: string,
  status: KanbanStatus | null,
  order: number | null,
): Promise<void> => call<void>("set_kanban", { id, status, order });

// ─── Project ontology (lifecycle status) ─────────────────────────────
// A per-project status (Active / Labs / Archived / Inbox). Set via the
// sidebar (context menu on a project). Archived projects are hidden from
// Launcher / Home / Digest by default; the sidebar groups all sessions by
// status. Mirrors set_kanban's nullable-override shape; vocab validated
// server-side against PROJECT_STATUSES in lib.rs.
export type ProjectStatus = "active" | "labs" | "archived" | "inbox";

/** Display labels for each status key (mirrors PROJECT_STATUSES in lib.rs). */
export const PROJECT_STATUS_LABELS: Record<ProjectStatus, string> = {
  active: "Active",
  labs: "Labs",
  archived: "Archived",
  inbox: "Inbox",
};

/** Order used for sidebar section rendering (pinned first, then by this). */
export const PROJECT_STATUS_ORDER: ProjectStatus[] = ["active", "labs", "inbox", "archived"];

export const setProjectStatus = (
  encodedDir: string,
  status: ProjectStatus | null,
): Promise<void> => call<void>("set_project_status", { encodedDir, status });

// Timeline digest (P6). get_timeline is read-only and instant. digest_session
// and digest_pending run the LLM behind the same TagError channel as tagging
// (catch with asTagError()); "no_recap" is a truthful skip, not an error.
export const getTimeline = (days = 7): Promise<TimelineResponse> =>
  call<TimelineResponse>("get_timeline", { days });

export const digestSession = (id: string): Promise<SessionDigest> =>
  call<SessionDigest>("digest_session", { id });

export const digestPending = (days = 7): Promise<DigestBatchReport> =>
  call<DigestBatchReport>("digest_pending", { days });

export const linkThreads = (days = 7): Promise<Thread[]> =>
  call<Thread[]>("link_threads", { days });

// Manual edit — each patched field becomes durable (returned in manualFields).
export const updateSessionDigest = (id: string, patch: DigestPatch): Promise<SessionDigest> =>
  call<SessionDigest>("update_session_digest", { id, ...patch });

// ─── Review tab (project report cards) ─────────────────────────────────
// Per-project report card for a window (7/14/30d): what was built / how / why,
// where every "built" claim cites real session evidence (validated in Rust).
// get_review is a cache-first read (no LLM); generate_reports runs the
// report-card pass (and refreshes threads first). Same TagError channel as the
// digest pass — "no_digest" is a truthful skip for a project with no digests.

/** A "what was built" claim with its evidence session ids. */
export interface BuiltClaim {
  claim: string;
  evidence: string[];
}

export type DvrStatus = "landed" | "partial" | "open";

/** One row of the desired-vs-real reconciliation. */
export interface DesiredVsRealRow {
  desired: string;
  real: string;
  status: DvrStatus;
}

/** One project's report card for a window. `headline` is null when no report
 *  has been generated yet (the UI offers to generate). `notDigestedCount` is
 *  shown honestly on the card. `stale` flags a source set that changed after
 *  generation; `manualFields` lists hand-edited fields (headline only in v1). */
export interface ProjectReport {
  projectKey: string;
  hub: string | null;
  name: string;
  headline: string | null;
  built: BuiltClaim[];
  how: string[];
  why: string[];
  desiredVsReal: DesiredVsRealRow[];
  windowDays: number;
  windowEnd: string;
  sessionIds: string[];
  notDigestedCount: number;
  filesTouched: number;
  costUsd: number;
  stale: boolean;
  manualFields: string[];
  generatedAt: string | null;
}

/** Everything the Review view needs in one round-trip. Sorted by recency. */
export interface ReviewResponse {
  windowDays: number;
  windowEnd: string;
  cards: ProjectReport[];
}

/** Result of a batch report generation over the window. */
export interface ReportBatchReport {
  generated: number;
  cached: number;
  failed: number;
  skippedNoDigest: number;
}

export const getReview = (days = 7): Promise<ReviewResponse> =>
  call<ReviewResponse>("get_review", { days });

export const generateReports = (days = 7): Promise<ReportBatchReport> =>
  call<ReportBatchReport>("generate_reports", { days });

// ─── Home screen (first screen) ────────────────────────────────────────
// list_threads clusters sessions into ranked threads (project / project·branch)
// and returns the top `limit` plus the orientation counts. Computed at query
// time; the frontend renders it verbatim — the why-sentence is pre-templated by
// the backend, not assembled here.

/** One ranked thread on the home screen. `whySentence` and `openTodos` are
 *  already prepared by the backend; render verbatim. */
export interface HomeThread {
  key: string;
  displayName: string;
  areaOfLife: string | null;
  gitBranch: string | null;
  sessionCount: number;
  activeDays14: number;
  lastTs: string | null;
  latestSessionId: string;
  latestTitle: string;
  latestRecap: string | null;
  openTodos: string[]; // up to 3, already filtered to status != completed
  /** Digest open loops (stated unfinished intent). Optional until backend L-term ships. */
  openLoops?: string[];
  workedOn?: string | null;
  outcome?: string | null;
  completionPct: number | null;
  whySentence: string; // pre-templated by backend, render verbatim
  score: number;
}

/** Everything the home screen needs in one round-trip. `stale` flips the hero
 *  from resume framing to memory-jog framing (global last activity > 14 days). */
export interface HomeData {
  threads: HomeThread[];
  totalSessions: number;
  activeThreadsThisWeek: number;
  stale: boolean; // global last activity older than 14 days
  lastActivityTs: string | null;
}

export const listThreads = (limit = 5): Promise<HomeData> =>
  call<HomeData>("list_threads", { limit });
