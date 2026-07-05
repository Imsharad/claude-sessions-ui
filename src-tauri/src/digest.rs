//! Timeline digest pipeline (P6).
//!
//! LLM output is untrusted input: schema-validated, referentially verified,
//! persisted with provenance, and the frontend reads only from the store.
//! Deterministic facts (day grouping, counts, gap days) are computed in SQL /
//! Rust, never by the LLM. The model only fills per-session digest slots
//! (`session_digests`) and a thread-linking pass that may reference only
//! existing rows.
//!
//! Two model passes, both over the direct-Anthropic transport (anthropic.rs),
//! same TagError channel as tagging:
//!   1. per-session digest — one strict-JSON object per session, cached by a
//!      stable content_hash (FNV-1a, survives process restarts).
//!   2. thread linking — groups sessions whose digests continue one arc; every
//!      member id is validated against the input set in Rust.

use crate::claude::TagError;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Bump when the prompt/schema changes: a row whose `prompt_version` differs is
/// stale (surfaced to the UI) and its content_hash won't match, forcing regen.
pub const DIGEST_PROMPT_VERSION: i64 = 1;

/// Same tier as tagging — fast + cheap, present in the pricing table.
pub const DIGEST_MODEL: &str = "claude-haiku-4-5";

// Server-side validation limits (chars). The prompt asks for these; we enforce
// them regardless of what the model returns — never a silent pass.
const WORKED_ON_MAX: usize = 200;
const OUTCOME_MAX: usize = 200;
const OPEN_LOOP_MAX: usize = 120;
const MAX_OPEN_LOOPS: usize = 3;
const CITATION_MAX: usize = 200;
const ARC_MAX: usize = 160;

/// Recap fed to the digest prompt is capped so a very long session's tail can't
/// blow the token budget; the digest reads the lead, not the tail.
const RECAP_PROMPT_MAX: usize = 2000;

/// Bounded concurrency for the batch pass (spec: 4).
const DIGEST_WORKERS: usize = 4;

/// Meta-session detection — the timeline watching itself work. A session is
/// `meta_session` when ALL THREE signals hold (the AND is the honesty — any one
/// alone is too noisy): a prompt-echo title prefix, a tiny message count, and
/// the app's own project. Mirrors the two-source blacklist signal: a real row
/// we deliberately exclude from the main list while still surfacing it honestly
/// in a side channel (`meta_session_ids`).
const META_MSG_THRESHOLD: i64 = 6;
const APP_PROJECT: &str = "claude-sessions-ui";
const META_TITLE_PREFIXES: &[&str] = &[
    "You are writing",
    "You are triaging",
    "You are grouping",
    "You are summarizing",
];

// ─── Wire types (camelCase; mirrored in src/lib/ipc.ts) ──────────────────────

/// One session's digest row. `verified`/`confidence` drive quiet-vs-confident
/// styling; `stale` flags a source that changed after generation; `manualFields`
/// lists hand-edited fields (never overwritten by regeneration).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionDigest {
    pub session_id: String,
    pub worked_on: String,
    pub outcome: String,
    pub open_loops: Vec<String>,
    pub citations: Vec<String>,
    pub verified: bool,
    pub confidence: Option<f64>,
    pub model: Option<String>,
    pub prompt_version: i64,
    pub generated_at: Option<String>,
    pub stale: bool,
    pub manual_fields: Vec<String>,
}

/// One day in the window. Gap days arrive as `sessionCount` 0 with empty
/// `sessionIds` — a truthful zero, never invented activity.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDay {
    pub date: String, // YYYY-MM-DD, local
    pub session_count: i64,
    pub session_ids: Vec<String>,
}

/// A multi-day workstream linking sessions by arc.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub arc: String,
    pub member_session_ids: Vec<String>,
    pub generated_at: Option<String>,
}

/// Everything the timeline needs in one round-trip.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineResponse {
    pub days: Vec<TimelineDay>,
    pub digests: HashMap<String, SessionDigest>,
    pub threads: Vec<Thread>,
    /// Harness/self sessions (triage, digest, grouping calls this very feature
    /// spawns) excluded from `days` and every total. Surfaced here so the UI can
    /// render them in a separate collapsed "app activity" group — never counted
    /// in day summaries. Empty when the window has no such sessions.
    pub meta_session_ids: Vec<String>,
}

/// Result of a batch backfill over the window.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DigestBatchReport {
    pub generated: i64,
    pub cached: i64,
    pub failed: i64,
    pub skipped_no_recap: i64,
}

// ─── Small helpers ───────────────────────────────────────────────────────────

fn db_err(e: impl std::fmt::Display) -> TagError {
    TagError::new("db", e.to_string())
}

/// Char-bounded truncation (no ellipsis) — the validation limits are hard caps.
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Last path segment of a cwd (the human-readable project tail).
fn project_tail(cwd: &str) -> String {
    cwd.split('/').next_back().unwrap_or(cwd).to_string()
}

/// Parse a JSON array-of-strings column into a Vec; null/invalid → empty.
fn parse_str_array(raw: Option<String>) -> Vec<String> {
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

/// Add a field name to a manual_fields list, deduped. Pure — unit-tested.
fn add_manual(mut manual: Vec<String>, field: &str) -> Vec<String> {
    if !manual.iter().any(|m| m == field) {
        manual.push(field.to_string());
    }
    manual
}

/// FNV-1a 64-bit. Stable across process restarts (unlike std DefaultHasher),
/// so the content_hash cache survives an app relaunch.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x00000100000001b3;
    let mut hash = OFFSET_BASIS;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Stable cache key over the exact inputs that determine a digest: the final
/// recap uuid + its content + the prompt version + the model. A change to any
/// of these changes the hash → regeneration; an unchanged source → cache hit.
fn content_hash(recap_uuid: &str, recap_content: &str, model: &str) -> String {
    // Unit separator between parts so concatenation can't be spoofed.
    let joined = format!(
        "{recap_uuid}\u{1f}{recap_content}\u{1f}{DIGEST_PROMPT_VERSION}\u{1f}{model}"
    );
    format!("{:016x}", fnv1a_64(joined.as_bytes()))
}

/// Pull the first `{`…last `}` JSON object out of the model's text (models wrap
/// JSON in prose or ```json fences). Same idiom as tag_session. Failure →
/// typed "invalid_json", never a silent pass.
fn extract_json_object(text: &str) -> Result<serde_json::Value, TagError> {
    let (start, end) = match (text.find('{'), text.rfind('}')) {
        (Some(s), Some(e)) if e > s => (s, e),
        _ => return Err(TagError::new("invalid_json", "model output contained no JSON object")),
    };
    serde_json::from_str(&text[start..=end])
        .map_err(|e| TagError::new("invalid_json", format!("model JSON did not parse: {e}")))
}

// ─── Per-session digest generation ───────────────────────────────────────────

/// The validated + normalized digest fields we actually persist.
#[derive(Debug, Clone)]
struct ValidatedDigest {
    worked_on: String,
    outcome: String,
    open_loops: Vec<String>,
    citations: Vec<String>,
    confidence: Option<f64>,
}

/// Server-side validation. Truncate strings to limits, clamp confidence to
/// [0,1], cap open_loops at 3, drop non-string array entries. Missing required
/// keys (worked_on, outcome) → "invalid_json".
fn validate_digest(v: &serde_json::Value) -> Result<ValidatedDigest, TagError> {
    let obj = v
        .as_object()
        .ok_or_else(|| TagError::new("invalid_json", "expected a JSON object"))?;

    let worked_raw = obj
        .get("worked_on")
        .and_then(|x| x.as_str())
        .ok_or_else(|| TagError::new("invalid_json", "missing/invalid worked_on"))?;
    let worked_on = truncate(worked_raw.trim(), WORKED_ON_MAX);
    if worked_on.is_empty() {
        return Err(TagError::new("invalid_json", "worked_on was empty"));
    }

    let outcome_raw = obj
        .get("outcome")
        .and_then(|x| x.as_str())
        .ok_or_else(|| TagError::new("invalid_json", "missing/invalid outcome"))?;
    let outcome = truncate(outcome_raw.trim(), OUTCOME_MAX);
    // The prompt permits "outcome unclear"; an empty string collapses to it so
    // the UI never renders a blank outcome.
    let outcome = if outcome.is_empty() {
        "outcome unclear".to_string()
    } else {
        outcome
    };

    // open_loops: array, drop non-strings + empties, each ≤120 chars, cap 3.
    let open_loops = obj
        .get("open_loops")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str())
                .map(|s| truncate(s.trim(), OPEN_LOOP_MAX))
                .filter(|s| !s.is_empty())
                .take(MAX_OPEN_LOOPS)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // citations: SHAs / file paths; drop non-strings + empties, cap length.
    let citations = obj
        .get("citations")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str())
                .map(|s| truncate(s.trim(), CITATION_MAX))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let confidence = obj
        .get("confidence")
        .and_then(|x| x.as_f64())
        .map(|c| c.clamp(0.0, 1.0));

    Ok(ValidatedDigest {
        worked_on,
        outcome,
        open_loops,
        citations,
        confidence,
    })
}

/// The recap that drives one digest: its identifying uuid + the text. The final
/// recap when marked; else the last two recap rows concatenated.
struct RecapInput {
    uuid: String,
    content: String,
}

fn load_recap_input(conn: &Connection, id: &str) -> Result<RecapInput, TagError> {
    // Marked-final recap wins.
    let final_row: Option<(String, String)> = conn
        .query_row(
            "SELECT uuid, content FROM recaps WHERE session_id = ?1 AND is_final = 1 LIMIT 1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(db_err)?;
    if let Some((uuid, content)) = final_row {
        return Ok(RecapInput { uuid, content });
    }

    // Fallback: concatenate the last two recap rows (chronological order).
    let mut stmt = conn
        .prepare("SELECT uuid, content FROM recaps WHERE session_id = ?1 ORDER BY seq DESC LIMIT 2")
        .map_err(db_err)?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(db_err)?
        .filter_map(Result::ok)
        .collect();
    if rows.is_empty() {
        return Err(TagError::new("no_recap", "session has no recap rows to digest"));
    }
    // rows are DESC (newest first): the newest uuid identifies the input; the
    // concatenated content is oldest → newest for readability.
    let uuid = rows[0].0.clone();
    let content = rows
        .iter()
        .rev()
        .map(|(_, c)| c.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(RecapInput { uuid, content })
}

/// Metadata + recap assembled for the digest prompt.
struct DigestContext {
    title: String,
    project: String,
    date: String,
    message_count: i64,
    recap: String,
}

fn load_digest_context(
    conn: &Connection,
    id: &str,
    recap_content: String,
) -> Result<DigestContext, TagError> {
    let (title, cwd, first_ts, message_count): (String, String, Option<String>, i64) = conn
        .query_row(
            "SELECT COALESCE(title,''), cwd, first_ts, message_count FROM sessions WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(db_err)?;
    let date = first_ts
        .as_deref()
        .and_then(|t| t.get(..10))
        .unwrap_or("unknown")
        .to_string();
    Ok(DigestContext {
        title,
        project: project_tail(&cwd),
        date,
        message_count,
        recap: truncate(&recap_content, RECAP_PROMPT_MAX),
    })
}

/// Compact digest prompt: metadata + recap → ONE strict JSON object. Demands
/// only what the recap supports.
fn build_digest_prompt(ctx: &DigestContext) -> String {
    format!(
        "You are writing a one-session digest for a Claude Code coding session. Read the \
recap and metadata, then report ONLY what the recap actually supports — never invent work.\n\n\
Metadata:\n\
- Title: {title}\n\
- Project: {project}\n\
- Date: {date}\n\
- Messages: {msgs}\n\n\
Recap (auto-generated summary of what happened):\n{recap}\n\n\
Return ONLY a strict JSON object, no prose and no markdown fences, with EXACTLY these keys:\n\
{{\n\
  \"worked_on\": a concrete sentence at most 200 characters naming the task, files, or decisions,\n\
  \"outcome\": a sentence at most 200 characters on what changed or got decided; use \"outcome unclear\" if the recap does not say,\n\
  \"open_loops\": an array of 0 to 3 strings, each at most 120 characters, ONLY items the recap leaves explicitly unresolved,\n\
  \"citations\": an array of commit SHAs or file paths mentioned in the recap, may be empty,\n\
  \"confidence\": a number from 0 to 1 for how well the recap supports this digest\n\
}}",
        title = ctx.title,
        project = ctx.project,
        date = ctx.date,
        msgs = ctx.message_count,
        recap = ctx.recap,
    )
}

/// One digest completion. Direct API first; degrade to the CLI (its own OAuth,
/// slower) on no-key / transient transport failure — but only when `allow_cli`
/// (the batch pass can't afford 11s-per-failure, mirroring backfill_tags).
fn generate_text(prompt: &str, max_tokens: u32, allow_cli: bool) -> Result<String, TagError> {
    match crate::anthropic::call_api(prompt, DIGEST_MODEL, max_tokens) {
        Ok(t) => Ok(t),
        Err(e) if allow_cli && matches!(e.kind.as_str(), "no_api_key" | "timeout" | "api_http") => {
            crate::claude::run_headless(prompt, DIGEST_MODEL)
        }
        Err(e) => Err(e),
    }
}

/// A stored digest row, read verbatim (before wire mapping).
struct StoredDigest {
    content_hash: String,
    prompt_version: i64,
    model: Option<String>,
    worked_on: String,
    outcome: String,
    open_loops: Vec<String>,
    citations: Vec<String>,
    verified: bool,
    confidence: Option<f64>,
    generated_at: Option<String>,
    manual_fields: Vec<String>,
}

fn load_stored_digest(conn: &Connection, id: &str) -> Option<StoredDigest> {
    conn.query_row(
        "SELECT content_hash, prompt_version, model, worked_on, outcome, open_loops,
                citations, verified, confidence, generated_at, manual_fields
         FROM session_digests WHERE session_id = ?1",
        params![id],
        |r| {
            Ok(StoredDigest {
                content_hash: r.get(0)?,
                prompt_version: r.get(1)?,
                model: r.get(2)?,
                worked_on: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                outcome: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                open_loops: parse_str_array(r.get::<_, Option<String>>(5)?),
                citations: parse_str_array(r.get::<_, Option<String>>(6)?),
                verified: r.get::<_, Option<i64>>(7)?.unwrap_or(1) != 0,
                confidence: r.get(8)?,
                generated_at: r.get(9)?,
                manual_fields: parse_str_array(r.get::<_, Option<String>>(10)?),
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn stored_to_wire(id: &str, s: &StoredDigest) -> SessionDigest {
    SessionDigest {
        session_id: id.to_string(),
        worked_on: s.worked_on.clone(),
        outcome: s.outcome.clone(),
        open_loops: s.open_loops.clone(),
        citations: s.citations.clone(),
        verified: s.verified,
        confidence: s.confidence,
        model: s.model.clone(),
        prompt_version: s.prompt_version,
        generated_at: s.generated_at.clone(),
        // A prompt-version mismatch means the stored row predates the current
        // schema/prompt — the frontend flags it as stale.
        stale: s.prompt_version != DIGEST_PROMPT_VERSION,
        manual_fields: s.manual_fields.clone(),
    }
}

/// The single upsert both auto-generation and manual-edit funnel through, so the
/// column list lives in one place.
#[allow(clippy::too_many_arguments)]
fn write_digest_row(
    conn: &Connection,
    id: &str,
    content_hash: &str,
    prompt_version: i64,
    model: &str,
    worked_on: &str,
    outcome: &str,
    open_loops: &[String],
    citations: &[String],
    verified: bool,
    confidence: Option<f64>,
    generated_at: &str,
    manual_fields: &[String],
) -> Result<(), TagError> {
    let open_loops_json = serde_json::to_string(open_loops).unwrap_or_else(|_| "[]".to_string());
    let citations_json = serde_json::to_string(citations).unwrap_or_else(|_| "[]".to_string());
    let manual_json = serde_json::to_string(manual_fields).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO session_digests
            (session_id, content_hash, prompt_version, model, worked_on, outcome,
             open_loops, citations, verified, confidence, generated_at, manual_fields)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
         ON CONFLICT(session_id) DO UPDATE SET
            content_hash=excluded.content_hash,
            prompt_version=excluded.prompt_version,
            model=excluded.model,
            worked_on=excluded.worked_on,
            outcome=excluded.outcome,
            open_loops=excluded.open_loops,
            citations=excluded.citations,
            verified=excluded.verified,
            confidence=excluded.confidence,
            generated_at=excluded.generated_at,
            manual_fields=excluded.manual_fields",
        params![
            id,
            content_hash,
            prompt_version,
            model,
            worked_on,
            outcome,
            open_loops_json,
            citations_json,
            verified as i64,
            confidence,
            generated_at,
            manual_json,
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

/// Persist an auto-generated digest, honoring manual_fields: a field is written
/// from the fresh generation ONLY if its camelCase name is not hand-edited;
/// otherwise the stored (hand-edited) value is kept. content_hash / prompt_version
/// / model / generated_at always update.
fn persist_auto_digest(
    conn: &Connection,
    id: &str,
    hash: &str,
    v: &ValidatedDigest,
) -> Result<SessionDigest, TagError> {
    let existing = load_stored_digest(conn, id);
    let manual = existing
        .as_ref()
        .map(|e| e.manual_fields.clone())
        .unwrap_or_default();
    let protected = |field: &str| manual.iter().any(|m| m == field);

    let worked_on = if protected("workedOn") {
        existing
            .as_ref()
            .map(|e| e.worked_on.clone())
            .unwrap_or_else(|| v.worked_on.clone())
    } else {
        v.worked_on.clone()
    };
    let outcome = if protected("outcome") {
        existing
            .as_ref()
            .map(|e| e.outcome.clone())
            .unwrap_or_else(|| v.outcome.clone())
    } else {
        v.outcome.clone()
    };
    let open_loops = if protected("openLoops") {
        existing
            .as_ref()
            .map(|e| e.open_loops.clone())
            .unwrap_or_else(|| v.open_loops.clone())
    } else {
        v.open_loops.clone()
    };

    let generated_at = chrono::Utc::now().to_rfc3339();
    write_digest_row(
        conn,
        id,
        hash,
        DIGEST_PROMPT_VERSION,
        DIGEST_MODEL,
        &worked_on,
        &outcome,
        &open_loops,
        &v.citations, // citations always update
        true,
        v.confidence, // confidence always updates
        &generated_at,
        &manual,
    )?;
    read_digest(conn, id)
}

fn read_digest(conn: &Connection, id: &str) -> Result<SessionDigest, TagError> {
    load_stored_digest(conn, id)
        .map(|s| stored_to_wire(id, &s))
        .ok_or_else(|| TagError::new("db", "digest row missing after write"))
}

/// Whether a digest completion actually ran (Generated) or the cache answered
/// (Cached). Lets the batch report split the two.
pub enum DigestOutcome {
    Generated(SessionDigest),
    Cached(SessionDigest),
}

impl DigestOutcome {
    pub fn into_inner(self) -> SessionDigest {
        match self {
            DigestOutcome::Generated(d) | DigestOutcome::Cached(d) => d,
        }
    }
}

/// Generate-or-return-cached for one session. Cache hit when a stored row's
/// content_hash matches the current source (and its prompt_version is current).
/// No recap → typed "no_recap".
pub fn generate_or_cache(
    conn: &Connection,
    id: &str,
    allow_cli: bool,
) -> Result<DigestOutcome, TagError> {
    let recap = load_recap_input(conn, id)?;
    let hash = content_hash(&recap.uuid, &recap.content, DIGEST_MODEL);

    if let Some(existing) = load_stored_digest(conn, id) {
        if existing.content_hash == hash && existing.prompt_version == DIGEST_PROMPT_VERSION {
            return Ok(DigestOutcome::Cached(stored_to_wire(id, &existing)));
        }
    }

    let ctx = load_digest_context(conn, id, recap.content)?;
    let prompt = build_digest_prompt(&ctx);
    let text = generate_text(&prompt, 512, allow_cli)?;
    let json = extract_json_object(&text)?;
    let validated = validate_digest(&json)?;
    let wire = persist_auto_digest(conn, id, &hash, &validated)?;
    Ok(DigestOutcome::Generated(wire))
}

/// Manual-edit path: apply only the provided fields, validate to the same
/// limits, and flag each edited field in manual_fields (deduped) so a future
/// auto-regeneration won't clobber it. Does NOT change content_hash /
/// prompt_version / generated_at — a later regen with the same source still
/// caches, preserving the edit.
pub fn update_digest(
    conn: &Connection,
    id: &str,
    worked_on: Option<String>,
    outcome: Option<String>,
    open_loops: Option<Vec<String>>,
) -> Result<SessionDigest, TagError> {
    let existing = load_stored_digest(conn, id);
    let mut manual = existing
        .as_ref()
        .map(|e| e.manual_fields.clone())
        .unwrap_or_default();

    // Base values from the existing row (or empty for a manual-first insert).
    let mut wo = existing.as_ref().map(|e| e.worked_on.clone()).unwrap_or_default();
    let mut oc = existing.as_ref().map(|e| e.outcome.clone()).unwrap_or_default();
    let mut ol = existing
        .as_ref()
        .map(|e| e.open_loops.clone())
        .unwrap_or_default();
    let citations = existing
        .as_ref()
        .map(|e| e.citations.clone())
        .unwrap_or_default();
    let confidence = existing.as_ref().and_then(|e| e.confidence);
    let verified = existing.as_ref().map(|e| e.verified).unwrap_or(true);

    if let Some(w) = worked_on {
        let t = truncate(w.trim(), WORKED_ON_MAX);
        if t.is_empty() {
            return Err(TagError::new("invalid_json", "worked_on was empty"));
        }
        wo = t;
        manual = add_manual(manual, "workedOn");
    }
    if let Some(o) = outcome {
        let t = truncate(o.trim(), OUTCOME_MAX);
        oc = if t.is_empty() { "outcome unclear".to_string() } else { t };
        manual = add_manual(manual, "outcome");
    }
    if let Some(loops) = open_loops {
        ol = loops
            .iter()
            .map(|s| truncate(s.trim(), OPEN_LOOP_MAX))
            .filter(|s| !s.is_empty())
            .take(MAX_OPEN_LOOPS)
            .collect();
        manual = add_manual(manual, "openLoops");
    }

    // Preserve provenance from the existing row. For a manual-first insert (no
    // row yet) derive a content_hash from the recap if one exists, else a
    // sentinel — content_hash is NOT NULL and a manual edit still needs a row.
    let hash = existing.as_ref().map(|e| e.content_hash.clone()).unwrap_or_else(|| {
        load_recap_input(conn, id)
            .ok()
            .map(|r| content_hash(&r.uuid, &r.content, DIGEST_MODEL))
            .unwrap_or_else(|| format!("{:016x}", fnv1a_64(b"manual")))
    });
    let prompt_version = existing
        .as_ref()
        .map(|e| e.prompt_version)
        .unwrap_or(DIGEST_PROMPT_VERSION);
    let model = existing
        .as_ref()
        .and_then(|e| e.model.clone())
        .unwrap_or_else(|| DIGEST_MODEL.to_string());
    let generated_at = existing
        .as_ref()
        .and_then(|e| e.generated_at.clone())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    write_digest_row(
        conn,
        id,
        &hash,
        prompt_version,
        &model,
        &wo,
        &oc,
        &ol,
        &citations,
        verified,
        confidence,
        &generated_at,
        &manual,
    )?;
    read_digest(conn, id)
}

// ─── Window helpers (deterministic; no LLM) ──────────────────────────────────

/// The `days`-day window [today-(days-1) .. today] in local time, as
/// (today_naive, oldest_date_string). One place so timeline + batch agree.
fn window_bounds(days: u32) -> (chrono::NaiveDate, String) {
    let days = days.max(1);
    let today = chrono::Local::now().date_naive();
    let oldest = today - chrono::Duration::days((days - 1) as i64);
    (today, oldest.format("%Y-%m-%d").to_string())
}

/// True when a session's cwd or encoded project_dir matches ANY blacklist
/// pattern — the same dual check the indexer / list_sessions apply.
fn dir_blacklisted(cwd: &str, project_dir: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        crate::indexer::is_blacklisted(cwd, p) || crate::indexer::is_blacklisted_encoded(project_dir, p)
    })
}

/// True when a session is the timeline watching itself work — a harness call
/// (triage/digest/group/summarize) this very feature spawns. Three
/// jointly-required signals (the AND is the honesty): prompt-echo title prefix,
/// tiny message count, and the app's own project. `project_tail` is the cwd's
/// last path segment, matching how `display_project` is derived everywhere else.
fn is_meta_session(title: &str, message_count: i64, project_tail: &str) -> bool {
    message_count < META_MSG_THRESHOLD
        && project_tail == APP_PROJECT
        && META_TITLE_PREFIXES.iter().any(|p| title.starts_with(p))
}

/// Session ids in the window, blacklist-filtered. Backs the batch + linking
/// passes (the timeline command builds its own richer projection).
fn window_session_ids(conn: &Connection, days: u32) -> Result<Vec<String>, TagError> {
    let (_, oldest) = window_bounds(days);
    let patterns = crate::db::load_blacklist_patterns(conn);
    let mut stmt = conn
        .prepare(
            "SELECT id, cwd, project_dir FROM sessions
             WHERE date(first_ts,'localtime') >= ?1",
        )
        .map_err(db_err)?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![oldest], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(db_err)?
        .filter_map(Result::ok)
        .collect();
    Ok(rows
        .into_iter()
        .filter(|(_, cwd, pd)| !dir_blacklisted(cwd, pd, &patterns))
        .map(|(id, _, _)| id)
        .collect())
}

/// Digests for a set of session ids, keyed by id. Only ids with a stored row
/// appear (a truthful absence for un-digested sessions).
fn load_digests_for(conn: &Connection, ids: &HashSet<String>) -> HashMap<String, SessionDigest> {
    let mut out = HashMap::new();
    for id in ids {
        if let Some(s) = load_stored_digest(conn, id) {
            out.insert(id.clone(), stored_to_wire(id, &s));
        }
    }
    out
}

/// Every thread with its members, one query.
fn load_all_threads(conn: &Connection) -> Vec<Thread> {
    let mut stmt = match conn.prepare(
        "SELECT t.id, t.arc, t.generated_at, m.session_id
         FROM threads t LEFT JOIN thread_members m ON m.thread_id = t.id",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    // Preserve first-seen order for stable output.
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Thread> = HashMap::new();
    for (tid, arc, gen, member) in rows.flatten() {
        let entry = map.entry(tid.clone()).or_insert_with(|| {
            order.push(tid.clone());
            Thread {
                id: tid.clone(),
                arc: arc.unwrap_or_default(),
                member_session_ids: Vec::new(),
                generated_at: gen,
            }
        });
        if let Some(sid) = member {
            entry.member_session_ids.push(sid);
        }
    }
    order.into_iter().filter_map(|id| map.remove(&id)).collect()
}

/// Threads with at least one member inside the window.
fn load_threads_intersecting(conn: &Connection, ids: &HashSet<String>) -> Vec<Thread> {
    let mut threads = load_all_threads(conn);
    threads.retain(|t| t.member_session_ids.iter().any(|m| ids.contains(m)));
    threads
}

// ─── Timeline (read-only, no LLM) ────────────────────────────────────────────

/// Build the day skeleton + digests + intersecting threads for the window. Every
/// calendar day in the window appears, newest first, including zero-count gap
/// days. Deterministic — no model call.
pub fn build_timeline(conn: &Connection, days: u32) -> Result<TimelineResponse, TagError> {
    let days = days.max(1);
    let (today, oldest) = window_bounds(days);
    let patterns = crate::db::load_blacklist_patterns(conn);

    // Sessions in the window, with their local-date bucket. Sorted by LAST
    // activity (not session start) so the per-day lists read in the same order
    // as the relative-time display (`card.lastTs`) — no broken-random chronology.
    let mut stmt = conn
        .prepare(
            "SELECT id, cwd, project_dir, title, message_count, date(first_ts,'localtime') AS d
             FROM sessions
             WHERE date(first_ts,'localtime') >= ?1
             ORDER BY last_ts DESC",
        )
        .map_err(db_err)?;
    let rows: Vec<(String, String, String, String, i64, String)> = stmt
        .query_map(params![oldest], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .map_err(db_err)?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    let mut by_date: HashMap<String, Vec<String>> = HashMap::new();
    let mut window_ids: HashSet<String> = HashSet::new();
    let mut meta_ids: Vec<String> = Vec::new();
    for (id, cwd, pd, title, msg, d) in rows {
        if dir_blacklisted(&cwd, &pd, &patterns) {
            continue;
        }
        // Meta-sessions (the timeline watching itself work) are routed to a
        // side channel — excluded from days/totals/digests/threads, surfaced
        // only in `meta_session_ids`. Same shape as the blacklist `continue`.
        let tail = cwd.split('/').next_back().unwrap_or(&cwd);
        if is_meta_session(&title, msg, tail) {
            meta_ids.push(id);
            continue;
        }
        by_date.entry(d).or_default().push(id.clone());
        window_ids.insert(id);
    }

    // Full skeleton: every day today → today-(days-1), newest first. Gap days
    // materialize as an empty list (a truthful zero).
    let mut days_vec = Vec::with_capacity(days as usize);
    for i in 0..days {
        let d = today - chrono::Duration::days(i as i64);
        let ds = d.format("%Y-%m-%d").to_string();
        let ids = by_date.remove(&ds).unwrap_or_default();
        days_vec.push(TimelineDay {
            session_count: ids.len() as i64,
            session_ids: ids,
            date: ds,
        });
    }

    let digests = load_digests_for(conn, &window_ids);
    let threads = load_threads_intersecting(conn, &window_ids);

    Ok(TimelineResponse {
        days: days_vec,
        digests,
        threads,
        meta_session_ids: meta_ids,
    })
}

// ─── Batch pass ──────────────────────────────────────────────────────────────

/// Generate digests for every window session that needs one (missing / stale /
/// hash-changed), bounded concurrency of 4. Never aborts on one failure; a
/// no-recap session is a truthful skip, not a failure. API-only (no CLI
/// fallback), mirroring backfill_tags.
pub fn digest_pending_blocking(days: u32) -> Result<DigestBatchReport, TagError> {
    let conn = crate::db::open().map_err(db_err)?;
    let ids = window_session_ids(&conn, days)?;
    drop(conn);

    let generated = AtomicI64::new(0);
    let cached = AtomicI64::new(0);
    let failed = AtomicI64::new(0);
    let skipped = AtomicI64::new(0);

    // Fixed worker count, chunk-and-join — same shape as backfill_tags. Each
    // worker owns its own DB connection (WAL serializes the tiny writes).
    let chunk = ids.len().div_ceil(DIGEST_WORKERS.max(1)).max(1);
    std::thread::scope(|s| {
        for group in ids.chunks(chunk) {
            let (g, c, f, sk) = (&generated, &cached, &failed, &skipped);
            s.spawn(move || {
                let conn = match crate::db::open() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                for id in group {
                    match generate_or_cache(&conn, id, false) {
                        Ok(DigestOutcome::Generated(_)) => g.fetch_add(1, Ordering::Relaxed),
                        Ok(DigestOutcome::Cached(_)) => c.fetch_add(1, Ordering::Relaxed),
                        Err(e) if e.kind == "no_recap" => sk.fetch_add(1, Ordering::Relaxed),
                        Err(_) => f.fetch_add(1, Ordering::Relaxed),
                    };
                }
            });
        }
    });

    Ok(DigestBatchReport {
        generated: generated.into_inner(),
        cached: cached.into_inner(),
        failed: failed.into_inner(),
        skipped_no_recap: skipped.into_inner(),
    })
}

// ─── Thread linking (second LLM pass) ────────────────────────────────────────

/// One row of the linking prompt: only fields the model needs to group work.
struct ThreadInput {
    session_id: String,
    worked_on: String,
    project: String,
    date: String,
}

/// Digest-bearing sessions in the window, as linking inputs.
fn load_thread_inputs(
    conn: &Connection,
    id_set: &HashSet<String>,
) -> Result<Vec<ThreadInput>, TagError> {
    let mut stmt = conn
        .prepare(
            "SELECT d.session_id, COALESCE(d.worked_on,''), s.cwd, date(s.first_ts,'localtime')
             FROM session_digests d JOIN sessions s ON s.id = d.session_id",
        )
        .map_err(db_err)?;
    let rows: Vec<(String, String, String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(db_err)?
        .filter_map(Result::ok)
        .collect();
    Ok(rows
        .into_iter()
        .filter(|(sid, ..)| id_set.contains(sid))
        .map(|(session_id, worked_on, cwd, date)| ThreadInput {
            session_id,
            worked_on,
            project: project_tail(&cwd),
            date: date.unwrap_or_default(),
        })
        .collect())
}

fn build_thread_prompt(inputs: &[ThreadInput]) -> String {
    let mut lines = String::new();
    for i in inputs {
        lines.push_str(&format!(
            "- id={} | date={} | project={} | worked_on={}\n",
            i.session_id, i.date, i.project, i.worked_on
        ));
    }
    format!(
        "You are grouping Claude Code coding sessions into narrative threads. Below are \
sessions, each with an id, date, project, and what was worked on. Group sessions that \
continue the SAME line of work into threads.\n\n\
Rules:\n\
- Use ONLY the ids listed below; never invent an id.\n\
- A session may belong to at most one thread.\n\
- A thread needs 2 or more members; ignore sessions that do not clearly continue another.\n\n\
Sessions:\n{lines}\n\
Return ONLY a strict JSON object, no prose and no markdown fences:\n\
{{\n\
  \"threads\": [\n\
    {{ \"arc\": a one-line narrative at most 160 characters, \"member_session_ids\": [two or more ids from the list above] }}\n\
  ]\n\
}}",
        lines = lines
    )
}

/// Delete prior threads whose members ALL fall inside the window (replace-not-
/// append). A thread spanning outside the window is left alone. Cascade removes
/// its members.
fn delete_in_window_threads(conn: &Connection, id_set: &HashSet<String>) -> Result<(), TagError> {
    for t in load_all_threads(conn) {
        if !t.member_session_ids.is_empty()
            && t.member_session_ids.iter().all(|m| id_set.contains(m))
        {
            conn.execute("DELETE FROM threads WHERE id = ?1", params![t.id])
                .map_err(db_err)?;
        }
    }
    Ok(())
}

/// Monotonic tie-breaker so two threads minted in the same nanosecond can't
/// collide on their generated id.
static THREAD_SEQ: AtomicU64 = AtomicU64::new(0);

/// A stable-ish unique id without a uuid dependency: FNV over time + a process
/// counter + the member set. Uniqueness matters (it's a PK); randomness does not.
fn new_thread_id(members: &[String]) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = THREAD_SEQ.fetch_add(1, Ordering::Relaxed);
    let seed = format!("{nanos}:{seq}:{}", members.join(","));
    format!("th-{:016x}", fnv1a_64(seed.as_bytes()))
}

/// Second-phase LLM pass over ONLY existing digest rows in the window. Every
/// returned member id is validated against the input set in Rust; a session
/// lands in at most one thread (first wins); threads with <2 valid members are
/// dropped. Replace-not-append.
pub fn link_threads_blocking(days: u32) -> Result<Vec<Thread>, TagError> {
    let conn = crate::db::open().map_err(db_err)?;
    let ids = window_session_ids(&conn, days)?;
    let id_set: HashSet<String> = ids.into_iter().collect();

    let inputs = load_thread_inputs(&conn, &id_set)?;
    // Nothing to link with fewer than two digests — return the current state,
    // untouched (no destructive replace on a degenerate input).
    if inputs.len() < 2 {
        return Ok(load_threads_intersecting(&conn, &id_set));
    }

    let prompt = build_thread_prompt(&inputs);
    let text = generate_text(&prompt, 1024, true)?;
    let json = extract_json_object(&text)?;

    let arr = json
        .get("threads")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    // Referential validation: drop unknown ids, dedup, one thread per session.
    let mut used: HashSet<String> = HashSet::new();
    let mut validated: Vec<(String, Vec<String>)> = Vec::new();
    for t in &arr {
        let arc = t
            .get("arc")
            .and_then(|a| a.as_str())
            .map(|s| truncate(s.trim(), ARC_MAX))
            .unwrap_or_default();
        let members_raw = t
            .get("member_session_ids")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();
        let mut members: Vec<String> = Vec::new();
        for m in &members_raw {
            if let Some(sid) = m.as_str() {
                let sid = sid.to_string();
                if id_set.contains(&sid) && !used.contains(&sid) && !members.contains(&sid) {
                    members.push(sid);
                }
            }
        }
        if members.len() < 2 {
            continue;
        }
        for sid in &members {
            used.insert(sid.clone());
        }
        validated.push((arc, members));
    }

    // Replace: clear prior in-window threads, then insert the fresh set.
    delete_in_window_threads(&conn, &id_set)?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut out = Vec::with_capacity(validated.len());
    for (arc, members) in validated {
        let tid = new_thread_id(&members);
        conn.execute(
            "INSERT INTO threads (id, arc, prompt_version, generated_at) VALUES (?1,?2,?3,?4)",
            params![tid, arc, DIGEST_PROMPT_VERSION, now],
        )
        .map_err(db_err)?;
        for sid in &members {
            conn.execute(
                "INSERT INTO thread_members (thread_id, session_id) VALUES (?1,?2)",
                params![tid, sid],
            )
            .map_err(db_err)?;
        }
        out.push(Thread {
            id: tid,
            arc,
            member_session_ids: members,
            generated_at: Some(now.clone()),
        });
    }
    Ok(out)
}

// ─── Tests (pure parts) ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_64_known_vectors() {
        // Canonical FNV-1a/64 test vectors.
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn content_hash_is_stable_and_input_sensitive() {
        let a = content_hash("uuid-1", "did some work", DIGEST_MODEL);
        let b = content_hash("uuid-1", "did some work", DIGEST_MODEL);
        assert_eq!(a, b, "same inputs must hash identically across calls");
        // 16 lowercase hex chars.
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        // Any input change flips the hash.
        assert_ne!(a, content_hash("uuid-2", "did some work", DIGEST_MODEL));
        assert_ne!(a, content_hash("uuid-1", "did other work", DIGEST_MODEL));
        assert_ne!(a, content_hash("uuid-1", "did some work", "other-model"));
    }

    #[test]
    fn extract_json_object_handles_fences_and_prose() {
        let fenced = "```json\n{\"worked_on\":\"x\"}\n```";
        assert!(extract_json_object(fenced).is_ok());
        let prosey = "Here you go: {\"confidence\": 0.7} — done!";
        let v = extract_json_object(prosey).unwrap();
        assert_eq!(v.get("confidence").and_then(|x| x.as_f64()), Some(0.7));
        assert_eq!(
            extract_json_object("no json at all").unwrap_err().kind,
            "invalid_json"
        );
    }

    #[test]
    fn validate_digest_clamps_truncates_and_caps() {
        let long_worked = "w".repeat(500);
        let long_loop = "l".repeat(300);
        let v = serde_json::json!({
            "worked_on": long_worked,
            "outcome": "shipped it",
            "open_loops": [long_loop, "loop b", "loop c", "loop d (dropped)", 42, "  "],
            "citations": ["abc123", "src/foo.rs", 7, ""],
            "confidence": 2.5
        });
        let out = validate_digest(&v).unwrap();
        assert_eq!(out.worked_on.chars().count(), WORKED_ON_MAX, "worked_on truncated to 200");
        assert_eq!(out.open_loops.len(), 3, "open_loops capped at 3");
        assert_eq!(out.open_loops[0].chars().count(), OPEN_LOOP_MAX, "each loop ≤120");
        // Non-string + empty entries dropped from citations.
        assert_eq!(out.citations, vec!["abc123".to_string(), "src/foo.rs".to_string()]);
        assert_eq!(out.confidence, Some(1.0), "confidence clamped to 1.0");
    }

    #[test]
    fn validate_digest_empty_outcome_becomes_unclear() {
        let v = serde_json::json!({
            "worked_on": "did a thing",
            "outcome": "   ",
        });
        let out = validate_digest(&v).unwrap();
        assert_eq!(out.outcome, "outcome unclear");
        assert!(out.open_loops.is_empty(), "missing open_loops → empty");
        assert!(out.citations.is_empty(), "missing citations → empty");
        assert_eq!(out.confidence, None, "missing confidence → None");
    }

    #[test]
    fn validate_digest_rejects_missing_required_keys() {
        // Missing worked_on.
        let no_worked = serde_json::json!({ "outcome": "x" });
        assert_eq!(validate_digest(&no_worked).unwrap_err().kind, "invalid_json");
        // Empty worked_on.
        let empty_worked = serde_json::json!({ "worked_on": "  ", "outcome": "x" });
        assert_eq!(validate_digest(&empty_worked).unwrap_err().kind, "invalid_json");
        // Missing outcome.
        let no_outcome = serde_json::json!({ "worked_on": "x" });
        assert_eq!(validate_digest(&no_outcome).unwrap_err().kind, "invalid_json");
    }

    #[test]
    fn add_manual_dedups_and_appends() {
        let m = add_manual(vec![], "workedOn");
        assert_eq!(m, vec!["workedOn".to_string()]);
        let m = add_manual(m, "workedOn"); // no-op
        assert_eq!(m, vec!["workedOn".to_string()]);
        let m = add_manual(m, "openLoops");
        assert_eq!(m, vec!["workedOn".to_string(), "openLoops".to_string()]);
    }

    /// Manual-fields merge: an auto value overwrites an unprotected field but is
    /// blocked on a hand-edited one. Exercises persist_auto_digest against an
    /// in-memory DB (schema mirrors migrate_v4).
    #[test]
    fn persist_auto_digest_respects_manual_fields() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY);
             CREATE TABLE session_digests (
                session_id TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                prompt_version INTEGER NOT NULL,
                model TEXT, worked_on TEXT, outcome TEXT,
                open_loops TEXT, citations TEXT,
                verified INTEGER DEFAULT 1, confidence REAL,
                generated_at TEXT, manual_fields TEXT
             );",
        )
        .unwrap();
        conn.execute("INSERT INTO sessions (id) VALUES ('s1')", []).unwrap();

        // Seed a row with a hand-edited workedOn.
        write_digest_row(
            &conn, "s1", "hash0", DIGEST_PROMPT_VERSION, DIGEST_MODEL,
            "HAND EDITED", "old outcome", &[], &[], true, Some(0.5),
            "2026-01-01T00:00:00Z", &["workedOn".to_string()],
        )
        .unwrap();

        // Auto-regenerate: outcome should update, workedOn must be preserved.
        let v = ValidatedDigest {
            worked_on: "auto worked".to_string(),
            outcome: "auto outcome".to_string(),
            open_loops: vec!["a loop".to_string()],
            citations: vec!["sha1".to_string()],
            confidence: Some(0.9),
        };
        let out = persist_auto_digest(&conn, "s1", "hash1", &v).unwrap();
        assert_eq!(out.worked_on, "HAND EDITED", "protected field preserved");
        assert_eq!(out.outcome, "auto outcome", "unprotected field updated");
        assert_eq!(out.citations, vec!["sha1".to_string()], "citations always update");
        assert!(out.manual_fields.contains(&"workedOn".to_string()));
        // content_hash always advances.
        let hash: String = conn
            .query_row("SELECT content_hash FROM session_digests WHERE session_id='s1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hash, "hash1");
    }

    /// An ISO-8601 UTC timestamp that buckets (via date(...,'localtime')) to the
    /// local calendar day `days_ago` days back. Noon local avoids DST edges.
    fn local_noon_utc(days_ago: i64) -> String {
        use chrono::TimeZone;
        let d = chrono::Local::now().date_naive() - chrono::Duration::days(days_ago);
        let naive = d.and_hms_opt(12, 0, 0).unwrap();
        chrono::Local
            .from_local_datetime(&naive)
            .single()
            .unwrap()
            .with_timezone(&chrono::Utc)
            .to_rfc3339()
    }

    fn seed_session(conn: &Connection, id: &str, days_ago: i64, recap: Option<&str>) {
        let ts = local_noon_utc(days_ago);
        conn.execute(
            "INSERT INTO sessions (id, project_dir, cwd, file_path, file_mtime, title, first_ts, last_ts, message_count)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?6, 5)",
            params![
                id,
                format!("-tmp-proj-{id}"),
                "/tmp/proj",
                format!("/tmp/{id}.jsonl"),
                format!("session {id}"),
                ts,
            ],
        )
        .unwrap();
        if let Some(content) = recap {
            conn.execute(
                "INSERT INTO recaps (session_id, uuid, content, seq, is_final) VALUES (?1, ?2, ?3, 0, 1)",
                params![id, format!("recap-{id}"), content],
            )
            .unwrap();
        }
    }

    /// No-network smoke test against a temp on-disk DB with the REAL migrated
    /// schema: full-calendar day coverage (gap day included, newest first, local
    /// bucketing), digest row round-trip through the timeline, and manual-field
    /// survival across an auto-regeneration.
    #[test]
    fn timeline_and_digest_roundtrip_on_temp_db() {
        let path = std::env::temp_dir().join(format!(
            "csui-digest-it-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        crate::db::migrate(&conn).unwrap();

        // Three sessions across 3 active days in a 4-day window; day -1 is a gap.
        seed_session(&conn, "s-today", 0, Some("wired the timeline view"));
        seed_session(&conn, "s-mid", 2, Some("built migrate_v4 digest tables"));
        seed_session(&conn, "s-old", 3, None); // no recap → no digest possible

        // 1. Full day coverage, newest first, gap day as a truthful zero.
        let tl = build_timeline(&conn, 4).unwrap();
        assert_eq!(tl.days.len(), 4, "every calendar day in the window appears");
        let today = chrono::Local::now().date_naive();
        for (i, day) in tl.days.iter().enumerate() {
            let expect = (today - chrono::Duration::days(i as i64))
                .format("%Y-%m-%d")
                .to_string();
            assert_eq!(day.date, expect, "days are newest-first with no holes");
            assert_eq!(day.session_count, day.session_ids.len() as i64);
        }
        assert_eq!(tl.days[0].session_ids, vec!["s-today".to_string()]);
        assert_eq!(tl.days[1].session_count, 0, "gap day present with zero count");
        assert!(tl.days[1].session_ids.is_empty());
        assert_eq!(tl.days[2].session_ids, vec!["s-mid".to_string()]);
        assert_eq!(tl.days[3].session_ids, vec!["s-old".to_string()]);
        assert!(tl.digests.is_empty(), "no digest rows yet — truthful absence");

        // 2. Digest round-trip: persist a fake validated digest (no network),
        //    then read it back through the timeline.
        let v = ValidatedDigest {
            worked_on: "auto worked".to_string(),
            outcome: "auto outcome".to_string(),
            open_loops: vec!["finish tests".to_string()],
            citations: vec!["src/digest.rs".to_string()],
            confidence: Some(0.8),
        };
        persist_auto_digest(&conn, "s-today", "hash-a", &v).unwrap();
        let tl = build_timeline(&conn, 4).unwrap();
        let d = tl.digests.get("s-today").expect("digest keyed by session id");
        assert_eq!(d.worked_on, "auto worked");
        assert_eq!(d.outcome, "auto outcome");
        assert_eq!(d.open_loops, vec!["finish tests".to_string()]);
        assert_eq!(d.prompt_version, DIGEST_PROMPT_VERSION);
        assert!(!d.stale, "current prompt_version is not stale");
        assert!(d.manual_fields.is_empty());

        // 3. Manual edit via the update_session_digest path, then regenerate:
        //    the hand-edited field survives, the rest updates.
        let edited = update_digest(
            &conn,
            "s-today",
            Some("HAND EDITED worked_on".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(edited.worked_on, "HAND EDITED worked_on");
        assert_eq!(edited.manual_fields, vec!["workedOn".to_string()]);

        let v2 = ValidatedDigest {
            worked_on: "regenerated worked".to_string(),
            outcome: "regenerated outcome".to_string(),
            open_loops: vec![],
            citations: vec![],
            confidence: Some(0.9),
        };
        let regen = persist_auto_digest(&conn, "s-today", "hash-b", &v2).unwrap();
        assert_eq!(regen.worked_on, "HAND EDITED worked_on", "manual field survives regeneration");
        assert_eq!(regen.outcome, "regenerated outcome", "unprotected field updates");
        assert!(regen.manual_fields.contains(&"workedOn".to_string()));

        drop(conn);
        let _ = std::fs::remove_file(&path);
    }
}
