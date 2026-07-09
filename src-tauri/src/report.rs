//! Report-card pipeline — the Review tab's fourth digest sibling.
//!
//! For a window (7/14/30d), every digested session is grouped under its
//! ontology-derived project, and one LLM report card per project answers
//! **what was built / how / why** — where every "built" claim cites real
//! session evidence (validated in Rust), and a desired-vs-real table
//! reconciles intent with artifacts.
//!
//! Same guardrails as the digest pass: schema-validated JSON, referential
//! check against the input set, FNV-1a content_hash (survives restart) gating
//! regeneration, and manual_fields protecting a hand-edited headline. The
//! model runs over the direct-Anthropic transport (anthropic.rs), same TagError
//! channel.

use crate::claude::TagError;
use crate::ontology::ProjectIdentity;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};

/// Bump when the prompt/schema changes: a row whose `prompt_version` differs is
/// stale (surfaced to the UI) and its content_hash won't match, forcing regen.
pub const REPORT_PROMPT_VERSION: i64 = 1;

/// Same tier as the digest pass — fast + cheap, present in the pricing table.
pub const REPORT_MODEL: &str = "claude-haiku-4-5";

// Server-side validation limits (chars / counts). The prompt asks for these;
// we enforce them regardless of what the model returns — never a silent pass.
const HEADLINE_MAX: usize = 120;
const CLAIM_MAX: usize = 160;
const DVR_FIELD_MAX: usize = 160;
const BUILT_MAX: usize = 4;
const HOW_MAX: usize = 3;
const WHY_MAX: usize = 2;
const DVR_MAX: usize = 3;

/// One project per worker — there are usually few, so concurrency is gentle.
const REPORT_WORKERS: usize = 4;

// ─── Wire types (camelCase; mirrored in src/lib/ipc.ts) ──────────────────────

/// A "what was built" claim with its evidence session ids. A bullet with zero
/// resolvable evidence ids is rejected in Rust before it ever persists.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BuiltClaim {
    pub claim: String,
    pub evidence: Vec<String>,
}

/// One row of the desired-vs-real reconciliation.
/// `status`: "landed" | "partial" | "open".
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DesiredVsRealRow {
    pub desired: String,
    pub real: String,
    pub status: String,
}

/// One project's report card for a window. `notDigestedCount` is shown
/// honestly on the card (sessions counted but excluded from prose). `stale`
/// flags a source set that changed after generation; `manualFields` lists
/// hand-edited fields (only the headline is editable in v1).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReport {
    pub project_key: String,
    pub hub: Option<String>,
    pub name: String,
    pub headline: Option<String>,
    pub built: Vec<BuiltClaim>,
    pub how: Vec<String>,
    pub why: Vec<String>,
    pub desired_vs_real: Vec<DesiredVsRealRow>,
    pub window_days: u32,
    pub window_end: String,
    pub session_ids: Vec<String>,
    pub not_digested_count: i64,
    pub files_touched: i64,
    pub cost_usd: f64,
    pub stale: bool,
    pub manual_fields: Vec<String>,
    pub generated_at: Option<String>,
}

/// Everything the Review view needs in one round-trip. Sorted by recency.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResponse {
    pub window_days: u32,
    pub window_end: String,
    pub cards: Vec<ProjectReport>,
}

/// Result of a batch report generation over the window.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReportBatchReport {
    pub generated: i64,
    pub cached: i64,
    pub failed: i64,
    pub skipped_no_digest: i64,
}

// ─── Small helpers (mirror digest.rs) ────────────────────────────────────────

fn db_err(e: impl std::fmt::Display) -> TagError {
    TagError::new("db", e.to_string())
}

/// Char-bounded truncation (no ellipsis) — the validation limits are hard caps.
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Last path segment of a cwd (kept for parity with digest's project_tail).
fn project_tail(cwd: &str) -> String {
    cwd.split('/').next_back().unwrap_or(cwd).to_string()
}

/// Parse a JSON array-of-strings column into a Vec; null/invalid → empty.
fn parse_str_array(raw: Option<String>) -> Vec<String> {
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

/// Add a field name to a manual_fields list, deduped.
fn add_manual(mut manual: Vec<String>, field: &str) -> Vec<String> {
    if !manual.iter().any(|m| m == field) {
        manual.push(field.to_string());
    }
    manual
}

/// FNV-1a 64-bit. Stable across process restarts, so the content_hash cache
/// survives an app relaunch. (Identical to digest.rs — copied locally so this
/// module stays self-contained, same convention as the digest/home copies of
/// dir_blacklisted.)
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

/// The `days`-day window [today-(days-1) .. today] in local time, as
/// (today_naive, oldest_date_string). One place so generation + read agree.
fn window_bounds(days: u32) -> (chrono::NaiveDate, String) {
    let days = days.max(1);
    let today = chrono::Local::now().date_naive();
    let oldest = today - chrono::Duration::days((days - 1) as i64);
    (today, oldest.format("%Y-%m-%d").to_string())
}

/// True when a session's cwd or encoded project_dir matches ANY blacklist
/// pattern — the same dual check the indexer / digest pass apply.
fn dir_blacklisted(cwd: &str, project_dir: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        crate::indexer::is_blacklisted(cwd, p) || crate::indexer::is_blacklisted_encoded(project_dir, p)
    })
}

/// Stable cache key over the exact inputs that determine a report: the project
/// key, the window end, the sorted set of input digest content_hashes, the
/// prompt version, and the model. A change to any of these flips the hash →
/// regeneration; an unchanged week → cache hit.
fn content_hash(project_key: &str, window_end: &str, digest_hashes: &[String], model: &str) -> String {
    // Sorted + unit-separated so concatenation can't be spoofed and order is stable.
    let mut sorted: Vec<&String> = digest_hashes.iter().collect();
    sorted.sort();
    let joined_hashes = sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",");
    let joined = format!(
        "{project_key}\u{1f}{window_end}\u{1f}{joined_hashes}\u{1f}{REPORT_PROMPT_VERSION}\u{1f}{model}"
    );
    format!("{:016x}", fnv1a_64(joined.as_bytes()))
}

/// Pull the first `{`…last `}` JSON object out of the model's text. Same idiom
/// as the digest pass. Failure → typed "invalid_json", never a silent pass.
fn extract_json_object(text: &str) -> Result<serde_json::Value, TagError> {
    let (start, end) = match (text.find('{'), text.rfind('}')) {
        (Some(s), Some(e)) if e > s => (s, e),
        _ => return Err(TagError::new("invalid_json", "model output contained no JSON object")),
    };
    serde_json::from_str(&text[start..=end])
        .map_err(|e| TagError::new("invalid_json", format!("model JSON did not parse: {e}")))
}

// ─── Grouping (deterministic Rust, no LLM) ───────────────────────────────────

/// One project's collected window data, the input to report generation.
struct ProjectWindow {
    identity: ProjectIdentity,
    /// Newest-first session ids in the window for this project (all of them,
    /// including un-digested ones, which are counted but excluded from prose).
    session_ids: Vec<String>,
    /// Ids that have a digest row — the only ones the model may cite.
    digested_ids: HashSet<String>,
    /// Per-session digest payloads for the prompt (worked_on, outcome, ...).
    digests: Vec<(String, SessionDigestLite)>,
    /// Thread arcs touching this project's digested sessions.
    arcs: Vec<String>,
    files_touched: i64,
    cost_usd: f64,
}

/// The digest fields the report prompt needs (a trimmed view of SessionDigest).
#[derive(Clone)]
struct SessionDigestLite {
    worked_on: String,
    outcome: String,
    open_loops: Vec<String>,
    verified: bool,
    confidence: Option<f64>,
}

/// The digest content_hashes that feed the report's content_hash (cache key).
/// These are the stored per-digest hashes for this project's digested sessions.
fn load_digest_hashes_for(conn: &Connection, ids: &HashSet<String>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for id in ids {
        if let Some(hash) = conn
            .query_row(
                "SELECT content_hash FROM session_digests WHERE session_id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
        {
            out.insert(id.clone(), hash);
        }
    }
    out
}

/// Load the digest payloads (lite) for a set of ids.
fn load_digest_lites(conn: &Connection, ids: &HashSet<String>) -> HashMap<String, SessionDigestLite> {
    let mut out = HashMap::new();
    for id in ids {
        if let Some(row) = conn
            .query_row(
                "SELECT worked_on, outcome, open_loops, verified, confidence
                 FROM session_digests WHERE session_id = ?1",
                params![id],
                |r| {
                    Ok(SessionDigestLite {
                        worked_on: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        outcome: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        open_loops: parse_str_array(r.get::<_, Option<String>>(2)?),
                        verified: r.get::<_, Option<i64>>(3)?.unwrap_or(1) != 0,
                        confidence: r.get(4)?,
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
        {
            out.insert(id.clone(), row);
        }
    }
    out
}

/// All threads with their member session ids.
fn load_all_threads(conn: &Connection) -> Vec<(String, Vec<String>)> {
    let mut stmt = match conn.prepare(
        "SELECT t.id, t.arc, m.session_id
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
        ))
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for row in rows.flatten() {
        let (tid, arc, member) = row;
        let entry = map.entry(tid.clone()).or_insert_with(|| {
            order.push(tid.clone());
            (arc.unwrap_or_default(), Vec::new())
        });
        if let Some(sid) = member {
            entry.1.push(sid);
        }
    }
    order.into_iter().filter_map(|id| map.remove(&id)).collect()
}

/// Group the window's sessions by ontology identity. Blacklist-filtered.
/// Sessions without a digest are counted (per project) but excluded from prose.
pub fn group_window_by_project(conn: &Connection, days: u32) -> Result<Vec<ProjectWindow>, TagError> {
    let (_, oldest) = window_bounds(days);
    let patterns = crate::db::load_blacklist_patterns(conn);

    // Sessions in the window, newest-first. Carry first_ts for sort stability.
    let mut stmt = conn
        .prepare(
            "SELECT id, cwd, project_dir, first_ts
             FROM sessions
             WHERE date(first_ts,'localtime') >= ?1
             ORDER BY first_ts DESC",
        )
        .map_err(db_err)?;
    let rows: Vec<(String, String, String, Option<String>)> = stmt
        .query_map(params![oldest], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(db_err)?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    // Group by ontology key, preserving insertion (newest-first) order.
    let mut order: Vec<String> = Vec::new();
    let mut by_key: HashMap<String, ProjectWindow> = HashMap::new();
    for (id, cwd, pd, _ts) in rows {
        if dir_blacklisted(&cwd, &pd, &patterns) {
            continue;
        }
        let identity = crate::ontology::derive_identity(&cwd);
        let key = identity.key.clone();
        let entry = by_key.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            ProjectWindow {
                identity,
                session_ids: Vec::new(),
                digested_ids: HashSet::new(),
                digests: Vec::new(),
                arcs: Vec::new(),
                files_touched: 0,
                cost_usd: 0.0,
            }
        });
        entry.session_ids.push(id);
    }

    // For each project, resolve digests + arcs + aggregates.
    let all_threads = load_all_threads(conn);
    let mut out: Vec<ProjectWindow> = Vec::with_capacity(order.len());
    for key in order {
        let mut pw = by_key.remove(&key).expect("key present from order pass");
        let id_set: HashSet<String> = pw.session_ids.iter().cloned().collect();

        // Digests: split digested vs not. `not_digested_count` = total - digested.
        let lites = load_digest_lites(conn, &id_set);
        pw.digested_ids = lites.keys().cloned().collect();
        // Newest-first digest order (session_ids is already newest-first).
        pw.digests = pw
            .session_ids
            .iter()
            .filter_map(|sid| lites.get(sid).map(|d| (sid.clone(), d.clone())))
            .collect();

        // Arcs touching this project's digested sessions.
        let mut arcs: Vec<String> = Vec::new();
        for (arc, members) in &all_threads {
            if members.iter().any(|m| pw.digested_ids.contains(m)) {
                if !arc.is_empty() {
                    arcs.push(arc.clone());
                }
            }
        }
        pw.arcs = arcs;

        // Aggregates: distinct files touched + total cost.
        pw.files_touched = aggregate_files_touched(conn, &id_set)?;
        pw.cost_usd = aggregate_cost(conn, &id_set)?;

        out.push(pw);
    }
    Ok(out)
}

/// Count distinct files touched across a project's window sessions.
fn aggregate_files_touched(conn: &Connection, ids: &HashSet<String>) -> Result<i64, TagError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT COUNT(DISTINCT file_path) FROM files_touched WHERE session_id IN ({placeholders})"
    );
    let params: Vec<&dyn rusqlite::ToSql> = ids
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let n: i64 = conn
        .query_row(&sql, params.as_slice(), |r| r.get(0))
        .optional()
        .map_err(db_err)?
        .unwrap_or(0);
    Ok(n)
}

/// Sum cost across a project's window sessions.
fn aggregate_cost(conn: &Connection, ids: &HashSet<String>) -> Result<f64, TagError> {
    if ids.is_empty() {
        return Ok(0.0);
    }
    let placeholders = std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT COALESCE(SUM(cost_usd),0) FROM session_usage WHERE session_id IN ({placeholders})"
    );
    let params: Vec<&dyn rusqlite::ToSql> = ids
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let n: f64 = conn
        .query_row(&sql, params.as_slice(), |r| r.get(0))
        .optional()
        .map_err(db_err)?
        .unwrap_or(0.0);
    Ok(n)
}

// Allow the `?` on OptionalExtension for the aggregate helpers above.
use rusqlite::OptionalExtension;

// ─── Validation ──────────────────────────────────────────────────────────────

/// The validated + normalized report fields we actually persist.
#[derive(Debug, Clone)]
struct ValidatedReport {
    headline: String,
    built: Vec<(String, Vec<String>)>,
    how: Vec<String>,
    why: Vec<String>,
    desired_vs_real: Vec<(String, String, String)>,
}

/// The canonical desired-vs-real statuses.
const DVR_STATUSES: &[&str] = &["landed", "partial", "open"];

/// Server-side validation. Truncate strings to limits, cap array lengths, drop
/// non-string entries, clamp `status` to the 3-value enum.
///
/// **Referential rule (the PRD's honesty guarantee):** a `built` bullet whose
/// `evidence` is empty OR cites an id NOT in `known_session_ids` is rejected —
/// the whole report fails with `invalid_json`. Same shape as thread linking's
/// referential check.
fn validate_report(
    v: &serde_json::Value,
    known_session_ids: &HashSet<String>,
) -> Result<ValidatedReport, TagError> {
    let obj = v
        .as_object()
        .ok_or_else(|| TagError::new("invalid_json", "expected a JSON object"))?;

    // headline — required, ≤120 chars.
    let headline_raw = obj
        .get("headline")
        .and_then(|x| x.as_str())
        .ok_or_else(|| TagError::new("invalid_json", "missing/invalid headline"))?;
    let headline = truncate(headline_raw.trim(), HEADLINE_MAX);
    if headline.is_empty() {
        return Err(TagError::new("invalid_json", "headline was empty"));
    }

    // built — array of {claim, evidence:[...]}. Referential check: every
    // evidence id must resolve to a known session, and evidence must be non-empty.
    let built_raw = obj
        .get("built")
        .and_then(|x| x.as_array())
        .ok_or_else(|| TagError::new("invalid_json", "missing/invalid built"))?;
    let mut built: Vec<(String, Vec<String>)> = Vec::new();
    for entry in built_raw.iter().take(BUILT_MAX) {
        let claim = entry
            .get("claim")
            .and_then(|x| x.as_str())
            .map(|s| truncate(s.trim(), CLAIM_MAX))
            .unwrap_or_default();
        if claim.is_empty() {
            continue;
        }
        let evidence_raw = entry.get("evidence").and_then(|x| x.as_array());
        let mut evidence: Vec<String> = Vec::new();
        if let Some(arr) = evidence_raw {
            for e in arr {
                if let Some(sid) = e.as_str() {
                    let sid = sid.to_string();
                    if known_session_ids.contains(&sid) && !evidence.contains(&sid) {
                        evidence.push(sid);
                    }
                }
            }
        }
        if evidence.is_empty() {
            // A claim with zero resolvable evidence is rejected — the report
            // does not render a claim it cannot back.
            return Err(TagError::new(
                "invalid_json",
                format!("built claim \"{claim}\" cites no resolvable evidence"),
            ));
        }
        built.push((claim, evidence));
    }
    if built.is_empty() {
        return Err(TagError::new("invalid_json", "built had no valid claims"));
    }

    // how — array of strings, ≤3, each ≤CLAIM_MAX.
    let how = string_array(obj, "how", HOW_MAX, CLAIM_MAX);

    // why — array of strings, ≤2.
    let why = string_array(obj, "why", WHY_MAX, CLAIM_MAX);

    // desired_vs_real — array of {desired, real, status}, ≤3.
    let dvr_raw = obj.get("desired_vs_real").and_then(|x| x.as_array());
    let mut desired_vs_real: Vec<(String, String, String)> = Vec::new();
    if let Some(arr) = dvr_raw {
        for row in arr.iter().take(DVR_MAX) {
            let desired = row
                .get("desired")
                .and_then(|x| x.as_str())
                .map(|s| truncate(s.trim(), DVR_FIELD_MAX))
                .unwrap_or_default();
            let real = row
                .get("real")
                .and_then(|x| x.as_str())
                .map(|s| truncate(s.trim(), DVR_FIELD_MAX))
                .unwrap_or_default();
            let status = row
                .get("status")
                .and_then(|x| x.as_str())
                .filter(|s| DVR_STATUSES.contains(s))
                .unwrap_or("open")
                .to_string();
            if desired.is_empty() && real.is_empty() {
                continue;
            }
            desired_vs_real.push((desired, real, status));
        }
    }

    Ok(ValidatedReport {
        headline,
        built,
        how,
        why,
        desired_vs_real,
    })
}

/// Extract a string array from a key: drop non-strings + empties, truncate,
/// cap at `max_items`.
fn string_array(obj: &serde_json::Map<String, serde_json::Value>, key: &str, max_items: usize, max_chars: usize) -> Vec<String> {
    obj.get(key)
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str())
                .map(|s| truncate(s.trim(), max_chars))
                .filter(|s| !s.is_empty())
                .take(max_items)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

// ─── Prompt ──────────────────────────────────────────────────────────────────

/// Build the report-card prompt for one project. Inputs: digests + arcs + the
/// session-id vocabulary the model may cite. Output: one strict JSON object.
fn build_report_prompt(pw: &ProjectWindow) -> Result<String, TagError> {
    if pw.digests.is_empty() {
        return Err(TagError::new("no_digest", "project has no digested sessions"));
    }

    // Session vocabulary: the ONLY ids the model may cite as evidence.
    let mut vocab = String::new();
    for (sid, d) in &pw.digests {
        vocab.push_str(&format!(
            "- id={sid} | worked_on={wo} | outcome={oc} | open_loops=[{lo}] | verified={v}",
            wo = d.worked_on,
            oc = d.outcome,
            lo = d.open_loops.join("; "),
            v = if d.verified { "yes" } else { "no" },
        ));
        if let Some(c) = d.confidence {
            vocab.push_str(&format!(" | confidence={:.2}", c));
        }
        vocab.push('\n');
    }

    let arcs = if pw.arcs.is_empty() {
        "(none identified)".to_string()
    } else {
        pw.arcs.iter().map(|a| format!("- {a}")).collect::<Vec<_>>().join("\n")
    };

    Ok(format!(
        "You are writing a weekly project report card for the \"{name}\" project, summarizing \
{cnt} digested session(s) in this window. Report ONLY what the session digests support — never \
invent work, and never cite a session id that is not listed below.\n\n\
Project: {name}{hub}\n\
Sessions (the ONLY ids you may cite as evidence):\n{vocab}\n\
Narrative arcs across these sessions:\n{arcs}\n\n\
Return ONLY a strict JSON object, no prose and no markdown fences, with EXACTLY these keys:\n\
{{\n\
  \"headline\": one calm sentence at most 120 characters answering \"what happened on this project\",\n\
  \"built\": an array of 1 to 4 objects, each {{ \"claim\": a concrete sentence at most 160 characters naming what was built/changed/decided, \"evidence\": [one or more session ids from the list above that prove this claim] }},\n\
  \"how\": an array of 0 to 3 strings at most 160 characters each on approach or method, tied to the arcs where they exist,\n\
  \"why\": an array of 0 to 2 strings at most 160 characters each on intent or motivation (drawn from the digests' worked_on / open_loops),\n\
  \"desired_vs_real\": an array of 0 to 3 objects {{ \"desired\": the intended outcome, \"real\": what actually landed, \"status\": \"landed\" | \"partial\" | \"open\" }}\n\
}}",
        name = pw.identity.name,
        hub = pw
            .identity
            .hub
            .as_ref()
            .map(|h| format!(" (hub: {h})"))
            .unwrap_or_default(),
        cnt = pw.digests.len(),
        vocab = vocab,
        arcs = arcs,
    ))
}

// ─── Persistence ─────────────────────────────────────────────────────────────

/// A stored report row, read verbatim (before wire mapping).
struct StoredReport {
    content_hash: String,
    prompt_version: i64,
    model: Option<String>,
    headline: Option<String>,
    built: Vec<(String, Vec<String>)>,
    how: Vec<String>,
    why: Vec<String>,
    desired_vs_real: Vec<(String, String, String)>,
    generated_at: Option<String>,
    manual_fields: Vec<String>,
}

/// Deserialize the built JSON column: [{claim, evidence:[...]}].
fn parse_built(raw: Option<String>) -> Vec<(String, Vec<String>)> {
    #[derive(Deserialize)]
    struct Row {
        claim: String,
        evidence: Vec<String>,
    }
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<Row>>(s).ok())
        .map(|rows| rows.into_iter().map(|r| (r.claim, r.evidence)).collect())
        .unwrap_or_default()
}

/// Deserialize the desired_vs_real JSON column.
fn parse_dvr(raw: Option<String>) -> Vec<(String, String, String)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        desired: String,
        real: String,
        status: String,
    }
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<Row>>(s).ok())
        .map(|rows| rows.into_iter().map(|r| (r.desired, r.real, r.status)).collect())
        .unwrap_or_default()
}

fn load_stored_report(
    conn: &Connection,
    project_key: &str,
    window_days: u32,
    window_end: &str,
) -> Option<StoredReport> {
    conn.query_row(
        "SELECT content_hash, prompt_version, model, headline, built, how, why,
                desired_vs_real, generated_at, manual_fields
         FROM project_reports
         WHERE project_key = ?1 AND window_days = ?2 AND window_end = ?3",
        params![project_key, window_days, window_end],
        |r| {
            Ok(StoredReport {
                content_hash: r.get(0)?,
                prompt_version: r.get(1)?,
                model: r.get(2)?,
                headline: r.get(3)?,
                built: parse_built(r.get::<_, Option<String>>(4)?),
                how: parse_str_array(r.get::<_, Option<String>>(5)?),
                why: parse_str_array(r.get::<_, Option<String>>(6)?),
                desired_vs_real: parse_dvr(r.get::<_, Option<String>>(7)?),
                generated_at: r.get(8)?,
                manual_fields: parse_str_array(r.get::<_, Option<String>>(9)?),
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

/// The single upsert both auto-generation and (future) manual-edit funnel
/// through, so the column list lives in one place.
#[allow(clippy::too_many_arguments)]
fn write_report_row(
    conn: &Connection,
    project_key: &str,
    window_days: u32,
    window_end: &str,
    content_hash: &str,
    prompt_version: i64,
    model: &str,
    headline: &str,
    built: &[(String, Vec<String>)],
    how: &[String],
    why: &[String],
    desired_vs_real: &[(String, String, String)],
    generated_at: &str,
    manual_fields: &[String],
) -> Result<(), TagError> {
    // Serialize the structured columns as JSON.
    let built_json = {
        #[derive(Serialize)]
        struct Row<'a> {
            claim: &'a str,
            evidence: &'a [String],
        }
        let rows: Vec<Row> = built
            .iter()
            .map(|(c, e)| Row { claim: c, evidence: e })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
    };
    let how_json = serde_json::to_string(how).unwrap_or_else(|_| "[]".to_string());
    let why_json = serde_json::to_string(why).unwrap_or_else(|_| "[]".to_string());
    let dvr_json = {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Row<'a> {
            desired: &'a str,
            real: &'a str,
            status: &'a str,
        }
        let rows: Vec<Row> = desired_vs_real
            .iter()
            .map(|(d, re, s)| Row { desired: d, real: re, status: s })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
    };
    let manual_json = serde_json::to_string(manual_fields).unwrap_or_else(|_| "[]".to_string());

    conn.execute(
        "INSERT INTO project_reports
            (project_key, window_days, window_end, content_hash, prompt_version, model,
             headline, built, how, why, desired_vs_real, manual_fields, generated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(project_key, window_days, window_end) DO UPDATE SET
            content_hash=excluded.content_hash,
            prompt_version=excluded.prompt_version,
            model=excluded.model,
            headline=excluded.headline,
            built=excluded.built,
            how=excluded.how,
            why=excluded.why,
            desired_vs_real=excluded.desired_vs_real,
            generated_at=excluded.generated_at,
            manual_fields=excluded.manual_fields",
        params![
            project_key,
            window_days,
            window_end,
            content_hash,
            prompt_version,
            model,
            headline,
            built_json,
            how_json,
            why_json,
            dvr_json,
            manual_json,
            generated_at,
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

/// Persist an auto-generated report, honoring manual_fields: the headline is
/// written from the fresh generation ONLY if `"headline"` is not hand-edited;
/// otherwise the stored (hand-edited) value is kept. content_hash /
/// prompt_version / model / generated_at always update.
fn persist_auto_report(
    conn: &Connection,
    pw: &ProjectWindow,
    window_days: u32,
    window_end: &str,
    hash: &str,
    v: &ValidatedReport,
) -> Result<(), TagError> {
    let existing = load_stored_report(conn, &pw.identity.key, window_days, window_end);
    let manual = existing
        .as_ref()
        .map(|e| e.manual_fields.clone())
        .unwrap_or_default();
    let protected = |field: &str| manual.iter().any(|m| m == field);

    let headline = if protected("headline") {
        existing
            .as_ref()
            .and_then(|e| e.headline.clone())
            .unwrap_or_else(|| v.headline.clone())
    } else {
        v.headline.clone()
    };

    let generated_at = chrono::Utc::now().to_rfc3339();
    write_report_row(
        conn,
        &pw.identity.key,
        window_days,
        window_end,
        hash,
        REPORT_PROMPT_VERSION,
        REPORT_MODEL,
        &headline,
        &v.built,
        &v.how,
        &v.why,
        &v.desired_vs_real,
        &generated_at,
        &manual,
    )
}

/// One completion. Direct API first; degrade to the CLI on no-key / transient
/// transport failure — but only when `allow_cli` (the batch pass can't afford
/// 11s-per-failure, mirroring digest_pending_blocking).
fn generate_text(prompt: &str, max_tokens: u32, allow_cli: bool) -> Result<String, TagError> {
    match crate::anthropic::call_api(prompt, REPORT_MODEL, max_tokens) {
        Ok(t) => Ok(t),
        Err(e) if allow_cli && matches!(e.kind.as_str(), "no_api_key" | "timeout" | "api_http") => {
            crate::claude::run_headless(prompt, REPORT_MODEL)
        }
        Err(e) => Err(e),
    }
}

/// Whether a report completion actually ran (Generated) or the cache answered
/// (Cached) — lets the batch report split the two.
pub enum ReportOutcome {
    Generated,
    Cached,
}

/// Generate-or-return-cached for one project. Cache hit when a stored row's
/// content_hash matches the current input (and its prompt_version is current).
/// No digested sessions → typed "no_digest" (a truthful skip).
pub fn generate_or_cache(
    conn: &Connection,
    pw: &ProjectWindow,
    window_days: u32,
    window_end: &str,
    allow_cli: bool,
) -> Result<ReportOutcome, TagError> {
    if pw.digests.is_empty() {
        return Err(TagError::new("no_digest", "project has no digested sessions"));
    }

    // Cache key over the input digest hashes.
    let hash_map = load_digest_hashes_for(conn, &pw.digested_ids);
    let mut digest_hashes: Vec<String> = hash_map.values().cloned().collect();
    digest_hashes.sort();
    let hash = content_hash(&pw.identity.key, window_end, &digest_hashes, REPORT_MODEL);

    if let Some(existing) = load_stored_report(conn, &pw.identity.key, window_days, window_end) {
        if existing.content_hash == hash && existing.prompt_version == REPORT_PROMPT_VERSION {
            return Ok(ReportOutcome::Cached);
        }
    }

    let prompt = build_report_prompt(pw)?;
    let text = generate_text(&prompt, 1024, allow_cli)?;
    let json = extract_json_object(&text)?;
    let validated = validate_report(&json, &pw.digested_ids)?;
    persist_auto_report(conn, pw, window_days, window_end, &hash, &validated)?;
    Ok(ReportOutcome::Generated)
}

// ─── Batch pass ──────────────────────────────────────────────────────────────

/// Generate report cards for every window project that needs one (missing /
/// stale / hash-changed), bounded concurrency of 4. Never aborts on one
/// failure; a project with no digested sessions is a truthful skip
/// (`skipped_no_digest`), not a failure. Runs thread linking first so the
/// "how" section's arcs are always fresh.
pub fn generate_reports_blocking(days: u32) -> Result<ReportBatchReport, TagError> {
    // Refresh threads first — the "how" section ties to arcs where they exist.
    let _ = crate::digest::link_threads_blocking(days)?;

    let conn = crate::db::open().map_err(db_err)?;
    let projects = group_window_by_project(&conn, days)?;
    let (_, window_end) = window_bounds(days);
    drop(conn);

    let generated = AtomicI64::new(0);
    let cached = AtomicI64::new(0);
    let failed = AtomicI64::new(0);
    let skipped = AtomicI64::new(0);

    // One project per worker — there are usually few. Chunk-and-join, each
    // worker owns its own DB connection (WAL serializes the tiny writes).
    let chunk = projects.len().div_ceil(REPORT_WORKERS.max(1)).max(1);
    std::thread::scope(|s| {
        for group in projects.chunks(chunk) {
            let (g, c, f, sk) = (&generated, &cached, &failed, &skipped);
            let window_end = window_end.clone();
            s.spawn(move || {
                let conn = match crate::db::open() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                for pw in group {
                    match generate_or_cache(&conn, pw, days, &window_end, false) {
                        Ok(ReportOutcome::Generated) => g.fetch_add(1, Ordering::Relaxed),
                        Ok(ReportOutcome::Cached) => c.fetch_add(1, Ordering::Relaxed),
                        Err(e) if e.kind == "no_digest" => sk.fetch_add(1, Ordering::Relaxed),
                        Err(_) => f.fetch_add(1, Ordering::Relaxed),
                    };
                }
            });
        }
    });

    Ok(ReportBatchReport {
        generated: generated.into_inner(),
        cached: cached.into_inner(),
        failed: failed.into_inner(),
        skipped_no_digest: skipped.into_inner(),
    })
}

// ─── Cache-first read (get_review) ───────────────────────────────────────────

/// Build the Review response: group the window, load any stored report cards
/// for (project_key, window_days, window_end). Projects with no stored report
/// appear with null fields (the frontend shows a "Generate" affordance). Sorted
/// by recency-weighted activity (newest session first).
pub fn get_review_blocking(days: u32) -> Result<ReviewResponse, TagError> {
    let conn = crate::db::open().map_err(db_err)?;
    let projects = group_window_by_project(&conn, days)?;
    let (_, window_end) = window_bounds(days);

    let mut cards: Vec<ProjectReport> = projects
        .iter()
        .map(|pw| {
            let stored = load_stored_report(&conn, &pw.identity.key, days, &window_end);
            let not_digested_count = (pw.session_ids.len() as i64) - (pw.digested_ids.len() as i64);
            let stale = stored
                .as_ref()
                .map(|s| s.prompt_version != REPORT_PROMPT_VERSION)
                .unwrap_or(false);
            let manual_fields = stored
                .as_ref()
                .map(|s| s.manual_fields.clone())
                .unwrap_or_default();
            let generated_at = stored.as_ref().and_then(|s| s.generated_at.clone());

            let (headline, built, how, why, desired_vs_real) = match &stored {
                Some(s) => (
                    s.headline.clone(),
                    s.built
                        .iter()
                        .map(|(claim, ev)| BuiltClaim {
                            claim: claim.clone(),
                            evidence: ev.clone(),
                        })
                        .collect::<Vec<_>>(),
                    s.how.clone(),
                    s.why.clone(),
                    s.desired_vs_real
                        .iter()
                        .map(|(d, re, st)| DesiredVsRealRow {
                            desired: d.clone(),
                            real: re.clone(),
                            status: st.clone(),
                        })
                        .collect::<Vec<_>>(),
                ),
                None => (None, Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };

            ProjectReport {
                project_key: pw.identity.key.clone(),
                hub: pw.identity.hub.clone(),
                name: pw.identity.name.clone(),
                headline,
                built,
                how,
                why,
                desired_vs_real,
                window_days: days,
                window_end: window_end.clone(),
                session_ids: pw.session_ids.clone(),
                not_digested_count: not_digested_count.max(0),
                files_touched: pw.files_touched,
                cost_usd: pw.cost_usd,
                stale,
                manual_fields,
                generated_at,
            }
        })
        .collect();

    // group_window_by_project already returns newest-first (sessions are
    // newest-first, and insertion order follows), which is the recency sort.
    cards.sort_by(|a, b| {
        // Stable-ish: compare by first (newest) session id, which encodes time.
        b.session_ids
            .first()
            .unwrap_or(&String::new())
            .cmp(a.session_ids.first().unwrap_or(&String::new()))
    });

    Ok(ReviewResponse {
        window_days: days,
        window_end,
        cards,
    })
}

// ─── Tests (pure parts) ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_64_known_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn content_hash_is_stable_and_input_sensitive() {
        let a = content_hash("NOW/brain", "2026-07-08", &["h1".into(), "h2".into()], REPORT_MODEL);
        let b = content_hash("NOW/brain", "2026-07-08", &["h2".into(), "h1".into()], REPORT_MODEL);
        assert_eq!(a, b, "order-insensitive over digest hashes");
        assert_eq!(a.len(), 16);
        // Any input change flips the hash.
        assert_ne!(a, content_hash("NOW/heart", "2026-07-08", &["h1".into(), "h2".into()], REPORT_MODEL));
        assert_ne!(a, content_hash("NOW/brain", "2026-07-09", &["h1".into(), "h2".into()], REPORT_MODEL));
        assert_ne!(a, content_hash("NOW/brain", "2026-07-08", &["h1".into(), "h3".into()], REPORT_MODEL));
        assert_ne!(a, content_hash("NOW/brain", "2026-07-08", &["h1".into(), "h2".into()], "other-model"));
    }

    #[test]
    fn validate_report_accepts_well_formed_input() {
        let mut known = HashSet::new();
        known.insert("s1".to_string());
        known.insert("s2".to_string());
        let v = serde_json::json!({
            "headline": "Shipped the report card",
            "built": [
                { "claim": "Added report.rs", "evidence": ["s1", "s2"] },
                { "claim": "Wired the command", "evidence": ["s2"] }
            ],
            "how": ["mirrored the digest pass"],
            "why": ["Monday-morning reconciliation"],
            "desired_vs_real": [
                { "desired": "land review tab", "real": "shipped tier 0-2", "status": "landed" }
            ]
        });
        let out = validate_report(&v, &known).unwrap();
        assert_eq!(out.headline, "Shipped the report card");
        assert_eq!(out.built.len(), 2);
        assert_eq!(out.built[0].1, vec!["s1".to_string(), "s2".to_string()]);
        assert_eq!(out.how, vec!["mirrored the digest pass"]);
        assert_eq!(out.desired_vs_real[0].2, "landed");
    }

    #[test]
    fn validate_report_rejects_claim_citing_unknown_session() {
        let mut known = HashSet::new();
        known.insert("s1".to_string());
        let v = serde_json::json!({
            "headline": "h",
            "built": [{ "claim": "did x", "evidence": ["bogus-id"] }]
        });
        let err = validate_report(&v, &known).unwrap_err();
        assert_eq!(err.kind, "invalid_json");
        assert!(err.message.contains("no resolvable evidence"));
    }

    #[test]
    fn validate_report_rejects_claim_with_empty_evidence() {
        let mut known = HashSet::new();
        known.insert("s1".to_string());
        let v = serde_json::json!({
            "headline": "h",
            "built": [{ "claim": "did x", "evidence": [] }]
        });
        assert_eq!(validate_report(&v, &known).unwrap_err().kind, "invalid_json");
    }

    #[test]
    fn validate_report_dedups_evidence_and_drops_unknown() {
        let mut known = HashSet::new();
        known.insert("s1".to_string());
        let v = serde_json::json!({
            "headline": "h",
            "built": [{ "claim": "did x", "evidence": ["s1", "s1", "bogus"] }]
        });
        let out = validate_report(&v, &known).unwrap();
        assert_eq!(out.built[0].1, vec!["s1".to_string()], "deduped + unknown dropped");
    }

    #[test]
    fn validate_report_truncates_and_caps_arrays() {
        let mut known = HashSet::new();
        known.insert("s1".to_string());
        let long_claim = "c".repeat(500);
        let long_how = "h".repeat(300);
        let mut built = Vec::new();
        for i in 0..8 {
            built.push(serde_json::json!({ "claim": format!("{long_claim}{i}"), "evidence": ["s1"] }));
        }
        let v = serde_json::json!({
            "headline": "h",
            "built": built,
            "how": [long_how, long_how, long_how, long_how, long_how],
            "why": ["a", "b", "c"],
            "desired_vs_real": [
                { "desired": "d", "real": "r", "status": "bogus" },
                { "desired": "d2", "real": "r2", "status": "partial" }
            ]
        });
        let out = validate_report(&v, &known).unwrap();
        assert_eq!(out.built.len(), BUILT_MAX, "built capped");
        assert_eq!(out.built[0].0.chars().count(), CLAIM_MAX, "claim truncated");
        assert_eq!(out.how.len(), HOW_MAX, "how capped");
        assert_eq!(out.why.len(), WHY_MAX, "why capped");
        assert_eq!(out.desired_vs_real.len(), 2);
        // Unknown status clamps to "open".
        assert_eq!(out.desired_vs_real[0].2, "open");
        assert_eq!(out.desired_vs_real[1].2, "partial");
    }

    #[test]
    fn validate_report_requires_headline_and_built() {
        let mut known = HashSet::new();
        known.insert("s1".to_string());
        // Missing headline.
        let no_headline = serde_json::json!({ "built": [{ "claim": "x", "evidence": ["s1"] }] });
        assert_eq!(validate_report(&no_headline, &known).unwrap_err().kind, "invalid_json");
        // Empty built.
        let no_built = serde_json::json!({ "headline": "h", "built": [] });
        assert_eq!(validate_report(&no_built, &known).unwrap_err().kind, "invalid_json");
    }

    #[test]
    fn add_manual_dedups() {
        let m = add_manual(vec![], "headline");
        let m = add_manual(m, "headline");
        assert_eq!(m, vec!["headline".to_string()]);
    }

    #[test]
    fn project_tail_mirrors_digest() {
        assert_eq!(project_tail("/a/b/brain"), "brain");
        assert_eq!(project_tail("brain"), "brain");
    }
}
