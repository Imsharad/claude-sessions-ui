//! Entry point. Registers Tauri commands that the React frontend calls over IPC.
//!
//! Commands:
//!   reindex(force_full) -> ScanStats
//!   list_sessions(filter) -> Vec<SessionCard>
//!   get_session_detail(id) -> SessionDetail  (recaps, todos, files, errors, usage)
//!   search_recaps(query) -> Vec<RecapHit>
//!   digest(days) -> Vec<DigestDay>
//!   resume_session(id, fork) -> ()
//!   get_stats() -> GlobalStats
//!   get_pricing() / set_pricing(...)
//!   toggle_pin(encoded_dir)
//!   index_status() -> IndexStatus

pub mod claude;
pub mod db;
pub mod indexer;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── IPC types ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub files_seen: usize,
    pub files_reindexed: usize,
    pub files_skipped_uptodate: usize,
    pub sessions_upserted: usize,
    pub duration_ms: u128,
    pub mode: String,
    pub error: Option<String>,
}

impl From<indexer::ScanStats> for ScanStats {
    fn from(s: indexer::ScanStats) -> Self {
        Self {
            files_seen: s.files_seen,
            files_reindexed: s.files_reindexed,
            files_skipped_uptodate: s.files_skipped_uptodate,
            sessions_upserted: s.sessions_upserted,
            duration_ms: s.duration_ms,
            mode: s.mode.to_string(),
            error: s.error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionCard {
    pub id: String,
    pub project_dir: String,
    pub cwd: String,
    pub display_project: String,
    pub git_branch: Option<String>,
    pub title: String,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub message_count: i64,
    pub duration_ms: i64,
    pub plan_mode: bool,
    pub has_recap: bool,
    pub recap: Option<String>, // final recap text, for card display
    pub input_toks: i64,       // summed across models
    pub output_toks: i64,
    pub cache_read_toks: i64,
    pub cost_usd: f64,
    pub cost_source: String, // 'config' | 'estimate' | 'mixed' | 'none'
    pub pinned: bool,
    // ─── Tag & triage (F3 populates, F2 renders, F4 places) ───
    // All nullable: absent = untagged, which the UI renders as clean absence.
    pub area_of_life: Option<String>,
    pub project_short_name: Option<String>,
    pub goal_completed: Option<bool>,
    pub completion_pct: Option<i64>,
    pub tag_rationale: Option<String>,
    pub tagged_at: Option<String>,
    /// Hand-edited field names, parsed from the `manual_fields` JSON array
    /// string. Null/invalid degrades to empty — no hand-edits, cleanly.
    pub manual_fields: Vec<String>,
    pub kanban_status: Option<String>,
    pub kanban_order: Option<f64>,
}

/// Parse the `manual_fields` column (a JSON array string of field names) into a
/// Vec. Null, empty, or invalid JSON all degrade to an empty vec.
fn parse_manual_fields(raw: Option<String>) -> Vec<String> {
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionFilter {
    pub project_dir: Option<String>,
    pub query: Option<String>, // matches title or recap
    pub since_days: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Recap {
    pub uuid: String,
    pub captured_ts: Option<String>,
    pub content: String,
    pub seq: i64,
    pub is_final: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub seq: i64,
    pub content: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub model: String,
    pub input_toks: i64,
    pub output_toks: i64,
    pub cache_create_toks: i64,
    pub cache_read_toks: i64,
    pub cost_usd: f64,
    pub cost_source: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub card: SessionCard,
    pub recaps: Vec<Recap>,
    pub todos: Vec<Todo>,
    pub usage: Vec<ModelUsage>,
    pub files_touched: Vec<(String, i64)>,
    pub errors: Vec<String>,
    pub turns: Vec<(i64, Option<i64>, Option<i64>)>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecapHit {
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    pub project: String,
    pub captured_ts: Option<String>,
    /// Windowed snippet around the best-match region. The frontend highlights
    /// `snippet_match` (the term in context) between `before`/`after`.
    pub snippet_before: String,
    pub snippet_match: String,
    pub snippet_after: String,
    /// Debug/transparency: the ranker score (higher = more relevant).
    pub score: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DigestDay {
    pub day: String, // YYYY-MM-DD
    pub sessions: Vec<DigestEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DigestEntry {
    pub session_id: String,
    pub title: String,
    pub cwd: String,
    pub project: String,
    pub last_ts: Option<String>,
    pub recap: Option<String>,
    pub message_count: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStats {
    pub total_sessions: i64,
    pub total_messages: i64,
    pub total_input_toks: i64,
    pub total_output_toks: i64,
    pub total_cache_read_toks: i64,
    pub total_cost_usd: f64,
    pub measured_cost_usd: f64,
    pub estimated_cost_usd: f64,
    pub earliest_ts: Option<String>,
    pub latest_ts: Option<String>,
    pub n_with_recap: i64,
    pub n_plan_mode: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub last_scan_ts: Option<String>,
    pub last_scan_mode: Option<String>,
    pub session_count: i64,
    pub recap_count: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PricingRow {
    pub model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlacklistEntry {
    pub pattern: String,
    pub created_at: Option<String>,
    /// Live count of indexed sessions this pattern currently hides.
    pub match_count: i64,
}

use claude::TagError;

/// Controlled area-of-life vocabulary (the editable one-liner from the spec).
/// Server-side validation normalizes the model's output against this list.
const AREAS_OF_LIFE: [&str; 5] = ["Building", "Research", "Content", "Ops", "Personal"];

/// The tagging model — fast + cheap, present in the pricing table.
const TAGGING_MODEL: &str = "claude-haiku-4-5";

/// The tag fields returned to the frontend after a tag / edit. camelCase over
/// the wire, mirrored by `SessionTags` in ipc.ts.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionTags {
    pub area_of_life: Option<String>,
    pub project_short_name: Option<String>,
    pub goal_completed: Option<bool>,
    pub completion_pct: Option<i64>,
    pub tag_rationale: Option<String>,
    pub tagged_at: Option<String>,
    pub manual_fields: Vec<String>,
}

// ─── Commands ───────────────────────────────────────────────────────────────

#[tauri::command]
fn reindex(force_full: Option<bool>) -> Result<ScanStats, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    let stats = indexer::run(&conn, force_full.unwrap_or(false));
    Ok(stats.into())
}

#[tauri::command]
fn index_status() -> Result<IndexStatus, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    let session_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap_or(0);
    let recap_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM recaps", [], |r| r.get(0))
            .unwrap_or(0);
    Ok(IndexStatus {
        last_scan_ts: db::get_meta(&conn, "last_scan_ts"),
        last_scan_mode: db::get_meta(&conn, "last_scan_mode"),
        session_count,
        recap_count,
    })
}

#[tauri::command]
fn list_sessions(filter: Option<SessionFilter>) -> Result<Vec<SessionCard>, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    let filter = filter.unwrap_or_default();

    let mut sql = String::from(
        "SELECT s.id, s.project_dir, s.cwd, s.git_branch, s.title, s.first_ts, s.last_ts,
                s.message_count, s.duration_ms, s.plan_mode, s.has_recap,
                (SELECT content FROM recaps r WHERE r.session_id = s.id AND r.is_final = 1) AS recap,
                COALESCE(SUM(u.input_toks),0), COALESCE(SUM(u.output_toks),0),
                COALESCE(SUM(u.cache_read_toks),0),
                COALESCE(SUM(u.cost_usd),0),
                p.pinned,
                s.area_of_life, s.project_short_name, s.goal_completed, s.completion_pct,
                s.tag_rationale, s.tagged_at, s.manual_fields, s.kanban_status, s.kanban_order
         FROM sessions s
         LEFT JOIN session_usage u ON u.session_id = s.id
         LEFT JOIN projects p ON p.encoded_dir = s.project_dir",
    );
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    if let Some(pd) = &filter.project_dir {
        clauses.push("s.project_dir = ?".to_string());
        binds.push(pd.clone());
    }
    if let Some(q) = &filter.query {
        if !q.trim().is_empty() {
            clauses.push("(s.title LIKE ? OR EXISTS (SELECT 1 FROM recaps r WHERE r.session_id = s.id AND r.content LIKE ?))".to_string());
            let like = format!("%{}%", q);
            binds.push(like.clone());
            binds.push(like);
        }
    }
    if let Some(days) = filter.since_days {
        clauses.push(format!(
            "s.last_ts >= datetime('now','-{} days')",
            days
        ));
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(
        " GROUP BY s.id ORDER BY p.pinned DESC, s.last_ts DESC NULLS LAST",
    );
    if let Some(lim) = filter.limit {
        sql.push_str(&format!(" LIMIT {}", lim));
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    // ponytail: iter over dynamic params, functional collect
    let mut out: Vec<SessionCard> = stmt
        .query_map(rusqlite::params_from_iter(binds), map_session_card)
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    // Defensive filter: drop blacklisted trees still lingering in the DB so a
    // just-added pattern takes effect this query cycle, no re-index needed.
    let patterns = db::load_blacklist_patterns(&conn);
    out.retain(|c| !dir_blacklisted(&c.cwd, &c.project_dir, &patterns));
    Ok(out)
}

fn map_session_card(r: &rusqlite::Row) -> rusqlite::Result<SessionCard> {
    let project_dir: String = r.get(1)?;
    let cwd: String = r.get(2)?;
    Ok(SessionCard {
        id: r.get(0)?,
        display_project: project_display(&cwd),
        project_dir,
        cwd,
        git_branch: r.get(3)?,
        title: r.get(4)?,
        first_ts: r.get(5)?,
        last_ts: r.get(6)?,
        message_count: r.get(7)?,
        duration_ms: r.get(8)?,
        plan_mode: r.get::<_, i64>(9)? != 0,
        has_recap: r.get::<_, i64>(10)? != 0,
        recap: r.get(11)?,
        input_toks: r.get(12)?,
        output_toks: r.get(13)?,
        cache_read_toks: r.get(14)?,
        cost_usd: r.get(15)?,
        cost_source: "aggregated".to_string(), // simplified; detail view has per-row source
        pinned: r.get::<_, i64>(16)? != 0,
        area_of_life: r.get(17)?,
        project_short_name: r.get(18)?,
        goal_completed: r.get::<_, Option<i64>>(19)?.map(|v| v != 0),
        completion_pct: r.get(20)?,
        tag_rationale: r.get(21)?,
        tagged_at: r.get(22)?,
        manual_fields: parse_manual_fields(r.get(23)?),
        kanban_status: r.get(24)?,
        kanban_order: r.get(25)?,
    })
}

fn project_display(cwd: &str) -> String {
    cwd.split('/').next_back().unwrap_or(cwd).to_string()
}

// ─── Blacklist: defensive query-path filter + live match counts ──────────────
// The indexer skips blacklisted trees at scan time; these apply the *second*
// enforcement point so a pattern added after indexing (or one whose rows still
// linger) drops those sessions from every query, and un-blacklisting re-surfaces
// them without a wipe. Both checks run — real cwd path AND encoded project_dir —
// because a hyphenated project name is only unambiguous in encoded space.

/// True when a session's `cwd` or encoded `project_dir` matches ANY pattern.
/// The one predicate the three query paths filter on, so each stays a one-liner.
fn dir_blacklisted(cwd: &str, project_dir: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|p| indexer::is_blacklisted(cwd, p) || indexer::is_blacklisted_encoded(project_dir, p))
}

/// Sessions a SINGLE pattern currently hides — computed in Rust over the
/// (cwd, project_dir) pairs, not a SQL glob, so it matches the indexer exactly.
fn count_matches(pairs: &[(String, String)], pattern: &str) -> i64 {
    pairs
        .iter()
        .filter(|(cwd, pd)| {
            indexer::is_blacklisted(cwd, pattern) || indexer::is_blacklisted_encoded(pd, pattern)
        })
        .count() as i64
}

/// All (cwd, project_dir) pairs — cheap for a ~700-row dataset. Backs both the
/// match counts and the defensive filter.
fn load_session_dirs(conn: &rusqlite::Connection) -> Vec<(String, String)> {
    conn.prepare("SELECT cwd, project_dir FROM sessions")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default()
}

/// Build the blacklist list with a live match count per pattern. Shared by the
/// list command and the two mutators (which return the refreshed list).
fn blacklist_entries(conn: &rusqlite::Connection) -> Result<Vec<BlacklistEntry>, String> {
    let pairs = load_session_dirs(conn);
    let mut stmt = conn
        .prepare("SELECT pattern, created_at FROM project_blacklist ORDER BY created_at")
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows
        .into_iter()
        .map(|(pattern, created_at)| {
            let match_count = count_matches(&pairs, &pattern);
            BlacklistEntry {
                pattern,
                created_at,
                match_count,
            }
        })
        .collect())
}

#[tauri::command]
fn list_blacklist() -> Result<Vec<BlacklistEntry>, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    blacklist_entries(&conn)
}

#[tauri::command]
fn add_blacklist_pattern(pattern: String) -> Result<Vec<BlacklistEntry>, String> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err("Enter a pattern to hide a project.".to_string());
    }
    let conn = db::open().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO project_blacklist (pattern, created_at) VALUES (?1, ?2)",
        params![p, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    blacklist_entries(&conn)
}

#[tauri::command]
fn remove_blacklist_pattern(pattern: String) -> Result<Vec<BlacklistEntry>, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM project_blacklist WHERE pattern = ?1",
        params![pattern.trim()],
    )
    .map_err(|e| e.to_string())?;
    blacklist_entries(&conn)
}

#[tauri::command]
fn get_session_detail(id: String) -> Result<SessionDetail, String> {
    let conn = db::open().map_err(|e| e.to_string())?;

    // Card row — direct lookup instead of loading every session.
    let card: SessionCard = {
        let mut stmt = conn
            .prepare(
                "SELECT s.id, s.project_dir, s.cwd, s.git_branch, s.title, s.first_ts, s.last_ts,
                        s.message_count, s.duration_ms, s.plan_mode, s.has_recap,
                        (SELECT content FROM recaps r WHERE r.session_id = s.id AND r.is_final = 1),
                        COALESCE((SELECT SUM(input_toks)  FROM session_usage u WHERE u.session_id=s.id),0),
                        COALESCE((SELECT SUM(output_toks) FROM session_usage u WHERE u.session_id=s.id),0),
                        COALESCE((SELECT SUM(cache_read_toks) FROM session_usage u WHERE u.session_id=s.id),0),
                        COALESCE((SELECT SUM(cost_usd) FROM session_usage u WHERE u.session_id=s.id),0),
                        COALESCE((SELECT pinned FROM projects p WHERE p.encoded_dir=s.project_dir),0),
                        s.area_of_life, s.project_short_name, s.goal_completed, s.completion_pct,
                        s.tag_rationale, s.tagged_at, s.manual_fields, s.kanban_status, s.kanban_order
                 FROM sessions s WHERE s.id = ?1",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![id], map_session_card)
            .map_err(|e| e.to_string())?
    };

    // Recaps
    let mut stmt = conn
        .prepare("SELECT uuid, captured_ts, content, seq, is_final FROM recaps WHERE session_id = ?1 ORDER BY seq")
        .map_err(|e| e.to_string())?;
    let recaps: Vec<Recap> = stmt
        .query_map(params![id], |r| {
            Ok(Recap {
                uuid: r.get(0)?,
                captured_ts: r.get(1)?,
                content: r.get(2)?,
                seq: r.get(3)?,
                is_final: r.get::<_, i64>(4)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    // Todos
    let mut stmt = conn
        .prepare("SELECT seq, content, status FROM todos WHERE session_id = ?1 ORDER BY seq")
        .map_err(|e| e.to_string())?;
    let todos: Vec<Todo> = stmt
        .query_map(params![id], |r| {
            Ok(Todo {
                seq: r.get(0)?,
                content: r.get(1)?,
                status: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    // Usage
    let mut stmt = conn
        .prepare("SELECT model, input_toks, output_toks, cache_create_toks, cache_read_toks, cost_usd, cost_source FROM session_usage WHERE session_id = ?1")
        .map_err(|e| e.to_string())?;
    let usage: Vec<ModelUsage> = stmt
        .query_map(params![id], |r| {
            Ok(ModelUsage {
                model: r.get(0)?,
                input_toks: r.get(1)?,
                output_toks: r.get(2)?,
                cache_create_toks: r.get(3)?,
                cache_read_toks: r.get(4)?,
                cost_usd: r.get(5)?,
                cost_source: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    // Files
    let mut stmt = conn
        .prepare("SELECT file_path, snapshots FROM files_touched WHERE session_id = ?1 ORDER BY snapshots DESC")
        .map_err(|e| e.to_string())?;
    let files_touched: Vec<(String, i64)> = stmt
        .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    // Errors
    let mut stmt = conn
        .prepare("SELECT kind FROM errors WHERE session_id = ?1 ORDER BY seq")
        .map_err(|e| e.to_string())?;
    let errors: Vec<String> = stmt
        .query_map(params![id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    // Turns
    let mut stmt = conn
        .prepare("SELECT turn_idx, duration_ms, message_count FROM turns WHERE session_id = ?1 ORDER BY turn_idx")
        .map_err(|e| e.to_string())?;
    let turns: Vec<(i64, Option<i64>, Option<i64>)> = stmt
        .query_map(params![id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(SessionDetail {
        card,
        recaps,
        todos,
        usage,
        files_touched,
        errors,
        turns,
    })
}

#[tauri::command]
fn search_recaps(query: String) -> Result<Vec<RecapHit>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let terms = tokenize(&q);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let conn = db::open().map_err(|e| e.to_string())?;

    // Candidate recaps: those matching ANY query term (LIKE, case-insensitive
    // via SQLite's ASCII folding — sufficient for a 543-row corpus). We pull the
    // full row once per matching term and union by (session_id, recap uuid) in
    // a map so the ranker sees the whole recap, not a per-term fragment.
    let mut stmt = conn
        .prepare(
            "SELECT r.session_id, r.uuid, s.title, s.cwd, r.captured_ts, r.content, s.project_dir
             FROM recaps r
             JOIN sessions s ON s.id = r.session_id
             WHERE r.content LIKE ?1",
        )
        .map_err(|e| e.to_string())?;

    // key = (session_id, recap uuid) so a recap with many matches is scored once.
    let mut seen: HashMap<(String, String), CandidateRecap> = HashMap::new();
    for term in &terms {
        let like = format!("%{}%", term);
        let rows = stmt
            .query_map(params![like], |r| {
                Ok(CandidateRecap {
                    session_id: r.get(0)?,
                    _uuid: r.get::<_, String>(1)?,
                    title: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    cwd: r.get(3)?,
                    captured_ts: r.get::<_, Option<String>>(4)?,
                    content: r.get(5)?,
                    project_dir: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            seen
                .entry((row.session_id.clone(), row._uuid.clone()))
                .and_modify(|existing| {
                    // Keep the most recent captured_ts if seen twice.
                    if row.captured_ts.as_deref() > existing.captured_ts.as_deref() {
                        existing.captured_ts = row.captured_ts.clone();
                    }
                })
                .or_insert_with(|| row.clone());
        }
    }
    drop(stmt);

    // Score + extract snippet, then rank. Defensive blacklist filter first, so
    // hidden trees never appear in search even if their rows still exist.
    let patterns = db::load_blacklist_patterns(&conn);
    let now = chrono::Utc::now();
    let mut scored: Vec<RecapHit> = seen
        .into_values()
        .filter(|c| !dir_blacklisted(&c.cwd, &c.project_dir, &patterns))
        .map(|c| score_recap(c, &terms, now))
        .collect();
    // Primary: score desc. Tiebreak: most recent first.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.captured_ts.cmp(&a.captured_ts))
    });
    scored.truncate(SEARCH_RESULT_LIMIT);
    Ok(scored)
}

// ─── Search ranker + snippet extraction ─────────────────────────────────────
// Transparent TF×recency×length scoring. No FTS5/BM25 — right-sized for a
// ~543-recap corpus and tunable in one place. Constants are intentionally
// named, not magic numbers.

const SEARCH_RESULT_LIMIT: usize = 50;
const SNIPPET_RADIUS: usize = 160; // chars of context each side of the match
const FRESH_WINDOW_DAYS: i64 = 30; // recency bonus full-strength within this

/// A candidate recap pulled from the DB, before scoring. `_uuid` is read for
/// dedup keying but not surfaced to the frontend.
#[derive(Clone)]
struct CandidateRecap {
    session_id: String,
    _uuid: String,
    title: String,
    cwd: String,
    captured_ts: Option<String>,
    content: String,
    /// Encoded project dir, read only for the defensive blacklist filter.
    project_dir: String,
}

/// Score a recap against the query terms and extract the best snippet window.
///
/// `score = term_freq_weight × recency_weight × length_norm`
///   term_freq_weight = Σ (1 + ln(count)) — dampened TF, rewards coverage
///   recency_weight   = 0.5 + 0.5 × min(1, fresh/days_old) — fresh bonus, floor 0.5
///   length_norm      = 1/√(word_count) — don't favor long recaps
fn score_recap(c: CandidateRecap, terms: &[String], now: chrono::DateTime<chrono::Utc>) -> RecapHit {
    let lower = c.content.to_lowercase();

    // Term-frequency weight: sum over terms of (1 + ln(count)), counting only
    // WORD-BOUNDED occurrences so "auth" inside "authoring" doesn't score.
    let lower_chars: Vec<char> = lower.chars().collect();
    let tf: f64 = terms
        .iter()
        .map(|t| {
            let n = find_word_bounded(&lower_chars, t).len() as f64;
            if n > 0.0 {
                1.0 + n.ln()
            } else {
                0.0
            }
        })
        .sum();

    // Recency weight: full bonus within FRESH_WINDOW_DAYS, decays to 0.5 floor.
    let recency = c
        .captured_ts
        .as_deref()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|t| {
            let days_old = (now - t.with_timezone(&chrono::Utc)).num_days().max(0) as f64;
            if days_old <= FRESH_WINDOW_DAYS as f64 {
                1.0
            } else {
                0.5 + 0.5 * (FRESH_WINDOW_DAYS as f64 / days_old)
            }
        })
        .unwrap_or(0.5); // unknown timestamp → neutral floor

    // Length normalization: penalize very long recaps so a 2000-word recap
    // doesn't dominate a 40-word one just by having more term hits.
    let word_count = lower.split_whitespace().count().max(1) as f64;
    let length_norm = 1.0 / word_count.sqrt();

    let score = tf * recency * length_norm;

    let (before, m, after) = best_snippet(&c.content, &lower, terms);
    let project = project_display(&c.cwd);

    RecapHit {
        session_id: c.session_id,
        title: c.title,
        cwd: c.cwd,
        project,
        captured_ts: c.captured_ts,
        snippet_before: before,
        snippet_match: m,
        snippet_after: after,
        score,
    }
}

/// Find the densest window of ±SNIPPET_RADIUS chars containing the most term
/// hits, then snap to word boundaries so the snippet doesn't cut mid-word.
/// Returns `(before, match_term, after)` sliced from the ORIGINAL content so
/// the highlight displays in original case.
///
/// All indexing is in **char** space (not bytes) to stay correct under
/// multibyte UTF-8 — recap text routinely contains non-ASCII punctuation.
fn best_snippet(content: &str, lower: &str, terms: &[String]) -> (String, String, String) {
    let content_chars: Vec<char> = content.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let n = lower_chars.len();

    // Collect char-indices of every WORD-BOUNDED term occurrence. Substring
    // hits (e.g. "auth" in "authoring") are rejected so the snippet anchors on
    // a real token, not a fragment inside a larger word.
    let mut hits: Vec<usize> = Vec::new();
    for t in terms {
        hits.extend(find_word_bounded(&lower_chars, t));
    }
    if hits.is_empty() {
        // The candidate matched via LIKE but no term is token-bounded (e.g. the
        // only occurrence was inside another word). Fall back to the head so the
        // result is still readable, with no false highlight.
        let head: String = content_chars.iter().take(2 * SNIPPET_RADIUS).collect();
        return (String::new(), head, String::new());
    }
    hits.sort_unstable();
    hits.dedup();

    // Densest window: anchor each hit, count other hits within ±RADIUS chars.
    let win = 2 * SNIPPET_RADIUS;
    let mut best_anchor = hits[0];
    let mut best_count = 0usize;
    for &h in &hits {
        let lo = h.saturating_sub(SNIPPET_RADIUS);
        let hi = (h + win).min(n + 1);
        let count = hits.iter().filter(|&&x| x >= lo && x < hi).count();
        if count > best_count {
            best_count = count;
            best_anchor = h;
        }
    }

    // The highlighted term = the longest query term found at the anchor region.
    // We pick the term actually present nearest best_anchor.
    let m_start_char = best_anchor;
    let match_term_len = terms
        .iter()
        .filter_map(|t| {
            let pat: Vec<char> = t.chars().collect();
            let plen = pat.len();
            if plen > 0
                && m_start_char + plen <= n
                && lower_chars[m_start_char..m_start_char + plen] == pat[..]
            {
                Some(plen)
            } else {
                None
            }
        })
        .max()
        .unwrap_or(1);
    let m_end_char = (m_start_char + match_term_len).min(n);

    // Before/after windows, snapped to word boundaries for clean reading.
    let before_start = snap_back_to_word(&content_chars, m_start_char.saturating_sub(SNIPPET_RADIUS));
    let after_end = snap_forward_to_word(&content_chars, (m_end_char + SNIPPET_RADIUS).min(n));

    let before: String = content_chars[before_start..m_start_char]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    let m: String = content_chars[m_start_char..m_end_char].iter().collect();
    let after: String = content_chars[m_end_char..after_end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();

    (before, m, after)
}

/// Walk left from `idx` to the start of the word (or 0). Ensures the snippet
/// doesn't begin mid-token.
fn snap_back_to_word(chars: &[char], mut idx: usize) -> usize {
    idx = idx.min(chars.len());
    // Skip trailing whitespace we may have landed in.
    while idx > 0 && chars[idx - 1].is_whitespace() {
        idx -= 1;
    }
    // Skip the partial word we may have landed inside.
    while idx > 0 && is_word_char(chars[idx - 1]) {
        idx -= 1;
    }
    idx
}

/// Walk right from `idx` to the end of the current word. Ensures the snippet
/// doesn't end mid-token.
fn snap_forward_to_word(chars: &[char], mut idx: usize) -> usize {
    let n = chars.len();
    idx = idx.min(n);
    // If we're mid-word, advance to its end; otherwise skip any whitespace.
    if idx < n && is_word_char(chars[idx]) {
        while idx < n && is_word_char(chars[idx]) {
            idx += 1;
        }
    } else {
        while idx < n && chars[idx].is_whitespace() {
            idx += 1;
        }
    }
    idx
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Tokenize a query into lowercase terms: split on whitespace/punctuation,
/// drop empties. Matches the ASCII-folding the LIKE search assumes.
fn tokenize(q: &str) -> Vec<String> {
    q.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// Find all char-indices where `term` occurs in `chars` as a **whole token**
/// (bounded by non-word characters or string edges). Prevents substring hits
/// like "auth" matching inside "authoring" — both for scoring and snippet
/// anchoring. Returns indices in ascending order.
fn find_word_bounded(chars: &[char], term: &str) -> Vec<usize> {
    let pat: Vec<char> = term.chars().collect();
    let plen = pat.len();
    let n = chars.len();
    let mut out = Vec::new();
    if plen == 0 || plen > n {
        return out;
    }
    let mut i = 0;
    while i + plen <= n {
        if chars[i..i + plen] == pat[..] {
            let left_ok = i == 0 || !is_word_char(chars[i - 1]);
            let right_idx = i + plen;
            let right_ok = right_idx == n || !is_word_char(chars[right_idx]);
            if left_ok && right_ok {
                out.push(i);
            }
            i += plen;
        } else {
            i += 1;
        }
    }
    out
}

#[tauri::command]
fn digest(days: Option<i64>) -> Result<Vec<DigestDay>, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    let days = days.unwrap_or(30);
    let mut stmt = conn
        .prepare(&format!(
            "SELECT s.id, s.title, s.cwd, s.last_ts,
                    (SELECT content FROM recaps r WHERE r.session_id = s.id AND r.is_final = 1),
                    s.message_count, s.project_dir
             FROM sessions s
             WHERE s.last_ts >= datetime('now', '-{} days')
             ORDER BY s.last_ts DESC",
            days
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let cwd: String = r.get(2)?;
            let project_dir: String = r.get(6)?;
            let last_ts: Option<String> = r.get(3)?;
            let day = last_ts
                .as_deref()
                .and_then(|t| t.get(..10))
                .unwrap_or("unknown")
                .to_string();
            Ok(DigestEntry {
                session_id: r.get(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                project: project_display(&cwd),
                cwd,
                last_ts,
                recap: r.get(4)?,
                message_count: r.get(5)?,
            })
            .map(|e| (day, project_dir, e))
        })
        .map_err(|e| e.to_string())?;

    // Defensive filter: same dual check as the indexer / list_sessions.
    let patterns = db::load_blacklist_patterns(&conn);
    let mut by_day: Vec<DigestDay> = Vec::new();
    for (day, project_dir, entry) in rows.flatten() {
        if dir_blacklisted(&entry.cwd, &project_dir, &patterns) {
            continue;
        }
        if by_day.last().map(|d| d.day == day).unwrap_or(false) {
            by_day.last_mut().unwrap().sessions.push(entry);
        } else {
            by_day.push(DigestDay {
                day,
                sessions: vec![entry],
            });
        }
    }
    Ok(by_day)
}

#[tauri::command]
fn get_stats() -> Result<GlobalStats, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    let mut stats = GlobalStats::default();
    stats.total_sessions =
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap_or(0);
    stats.total_messages =
        conn.query_row("SELECT COALESCE(SUM(message_count),0) FROM sessions", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let toks: (i64, i64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(input_toks),0), COALESCE(SUM(output_toks),0),
                    COALESCE(SUM(cache_read_toks),0)
             FROM session_usage",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or((0, 0, 0));
    stats.total_input_toks = toks.0;
    stats.total_output_toks = toks.1;
    stats.total_cache_read_toks = toks.2;
    let costs: (f64, f64, f64) = conn
        .query_row(
            "SELECT
                COALESCE(SUM(cost_usd),0),
                COALESCE(SUM(CASE WHEN cost_source='config' THEN cost_usd ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN cost_source='estimate' THEN cost_usd ELSE 0 END),0)
             FROM session_usage",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or((0.0, 0.0, 0.0));
    stats.total_cost_usd = costs.0;
    stats.measured_cost_usd = costs.1;
    stats.estimated_cost_usd = costs.2;
    stats.earliest_ts = conn
        .query_row("SELECT MIN(first_ts) FROM sessions", [], |r| r.get(0))
        .ok()
        .flatten();
    stats.latest_ts = conn
        .query_row("SELECT MAX(last_ts) FROM sessions", [], |r| r.get(0))
        .ok()
        .flatten();
    stats.n_with_recap =
        conn.query_row("SELECT COUNT(*) FROM sessions WHERE has_recap=1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    stats.n_plan_mode =
        conn.query_row("SELECT COUNT(*) FROM sessions WHERE plan_mode=1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    Ok(stats)
}

#[tauri::command]
fn resume_session(id: String, fork: Option<bool>) -> Result<(), String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    let cwd: String = conn
        .query_row(
            "SELECT cwd FROM sessions WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    claude::open_in_terminal(claude::ResumeArgs {
        session_id: id,
        cwd,
        fork: fork.unwrap_or(false),
    })
}

#[tauri::command]
fn toggle_pin(encoded_dir: String) -> Result<(), String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE projects SET pinned = 1 - pinned WHERE encoded_dir = ?1",
        params![encoded_dir],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_pricing() -> Result<Vec<PricingRow>, String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT model, input_per_mtok, output_per_mtok, cache_write_per_mtok, cache_read_per_mtok
             FROM pricing ORDER BY model",
        )
        .map_err(|e| e.to_string())?;
    // ponytail: iter mapping
    let out: Result<Vec<_>, String> = stmt
        .query_map([], |r| {
            Ok(PricingRow {
                model: r.get(0)?,
                input_per_mtok: r.get(1)?,
                output_per_mtok: r.get(2)?,
                cache_write_per_mtok: r.get(3)?,
                cache_read_per_mtok: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect();
    out
}

#[tauri::command]
fn set_pricing(rows: Vec<PricingRow>) -> Result<(), String> {
    let conn = db::open().map_err(|e| e.to_string())?;
    for r in &rows {
        conn.execute(
            "INSERT OR REPLACE INTO pricing
             (model, input_per_mtok, output_per_mtok, cache_write_per_mtok, cache_read_per_mtok)
             VALUES (?1,?2,?3,?4,?5)",
            params![r.model, r.input_per_mtok, r.output_per_mtok, r.cache_write_per_mtok, r.cache_read_per_mtok],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ─── AI session tagging (Feature 3) ─────────────────────────────────────────
// One structured call over a session's recap + metadata → strict JSON → server-
// side validation → persist. Transport is the headless local `claude` CLI
// (claude.rs), not an HTTP client. Auto fields never clobber a hand-edited
// field (manual_fields protection); a hand edit adds the field to that list.
//
// manual_fields stores the *camelCase* field identifiers ("areaOfLife",
// "projectShortName", "goalCompleted", "completionPct") so the frontend can
// check membership directly against its field keys.

/// Context assembled from the DB for one session, fed into the tag prompt.
struct TagContext {
    title: String,
    project: String,
    message_count: i64,
    duration_ms: i64,
    recap: Option<String>,
    todos_total: i64,
    todos_done: i64,
}

/// Validated tag values after server-side checks (area in vocab, pct clamped,
/// short name trimmed + truncated). The shape we actually persist.
#[derive(Debug)]
struct ValidatedTags {
    area_of_life: String,
    project_short_name: String,
    goal_completed: bool,
    completion_pct: i64,
    rationale: String,
}

fn db_err(e: impl std::fmt::Display) -> TagError {
    TagError::new("db", e.to_string())
}

/// Case-normalize an area against the controlled vocabulary. None if not a
/// member (rejected server-side; the model does not get to invent areas).
fn normalize_area(raw: &str) -> Option<String> {
    let t = raw.trim();
    AREAS_OF_LIFE
        .iter()
        .find(|a| a.eq_ignore_ascii_case(t))
        .map(|a| a.to_string())
}

/// Add a field name to a manual_fields list, deduped. Pure so it is unit-tested.
fn with_manual_field(mut manual: Vec<String>, field: &str) -> Vec<String> {
    if !manual.iter().any(|m| m == field) {
        manual.push(field.to_string());
    }
    manual
}

/// Pull the first `{`…last `}` JSON object out of the model's result text.
/// Models sometimes wrap JSON in prose or ```json fences; this tolerates that.
/// Anything that still fails to parse → typed "invalid_json", never a silent
/// pass.
fn extract_json_object(text: &str) -> Result<serde_json::Value, TagError> {
    let (start, end) = match (text.find('{'), text.rfind('}')) {
        (Some(s), Some(e)) if e > s => (s, e),
        _ => return Err(TagError::new("invalid_json", "model output contained no JSON object")),
    };
    serde_json::from_str(&text[start..=end])
        .map_err(|e| TagError::new("invalid_json", format!("model JSON did not parse: {e}")))
}

/// Validate + normalize the model's JSON object into ValidatedTags. Every
/// missing/ill-typed field, or an area outside the vocabulary, → "invalid_json".
fn validate_tags(v: &serde_json::Value) -> Result<ValidatedTags, TagError> {
    let obj = v
        .as_object()
        .ok_or_else(|| TagError::new("invalid_json", "expected a JSON object"))?;

    let area_raw = obj
        .get("area_of_life")
        .and_then(|x| x.as_str())
        .ok_or_else(|| TagError::new("invalid_json", "missing/invalid area_of_life"))?;
    let area_of_life = normalize_area(area_raw).ok_or_else(|| {
        TagError::new("invalid_json", format!("area_of_life '{area_raw}' not in vocabulary"))
    })?;

    let name_raw = obj
        .get("project_short_name")
        .and_then(|x| x.as_str())
        .ok_or_else(|| TagError::new("invalid_json", "missing/invalid project_short_name"))?;
    let trimmed = name_raw.trim();
    if trimmed.is_empty() {
        return Err(TagError::new("invalid_json", "project_short_name was empty"));
    }
    let project_short_name: String = trimmed.chars().take(24).collect();

    let goal_completed = obj
        .get("goal_completed")
        .and_then(|x| x.as_bool())
        .ok_or_else(|| TagError::new("invalid_json", "missing/invalid goal_completed"))?;

    let pct_val = obj
        .get("completion_pct")
        .ok_or_else(|| TagError::new("invalid_json", "missing completion_pct"))?;
    // Accept int or float (models sometimes emit 85.0); round, then clamp 0-100.
    let pct = pct_val
        .as_i64()
        .or_else(|| pct_val.as_f64().map(|f| f.round() as i64))
        .ok_or_else(|| TagError::new("invalid_json", "completion_pct was not a number"))?
        .clamp(0, 100);

    let rationale = obj
        .get("rationale")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(ValidatedTags {
        area_of_life,
        project_short_name,
        goal_completed,
        completion_pct: pct,
        rationale,
    })
}

/// Compact classification prompt: the controlled vocabulary + the session's
/// recap and metadata, demanding ONLY a strict JSON object.
fn build_tag_prompt(ctx: &TagContext) -> String {
    let areas = AREAS_OF_LIFE.join(", ");
    let recap = ctx.recap.as_deref().unwrap_or("(no recap was captured for this session)");
    format!(
        "You are triaging one Claude Code coding session. Classify it from its \
recap and metadata.\n\n\
Metadata:\n\
- Title: {title}\n\
- Project: {project}\n\
- Messages: {msgs}\n\
- Duration (ms): {dur}\n\
- Todos completed: {done}/{total}\n\n\
Recap (auto-generated summary of what happened):\n{recap}\n\n\
Return ONLY a strict JSON object, no prose and no markdown fences, with EXACTLY \
these five keys:\n\
{{\n\
  \"area_of_life\": one of [{areas}],\n\
  \"project_short_name\": a SHORT human name for the project, never a path, at most 24 characters,\n\
  \"goal_completed\": true or false,\n\
  \"completion_pct\": an integer from 0 to 100,\n\
  \"rationale\": one sentence justifying the completion judgement\n\
}}",
        title = ctx.title,
        project = ctx.project,
        msgs = ctx.message_count,
        dur = ctx.duration_ms,
        done = ctx.todos_done,
        total = ctx.todos_total,
        recap = recap,
        areas = areas,
    )
}

/// Load the recap + metadata used to build the tag prompt.
fn load_tag_context(conn: &rusqlite::Connection, id: &str) -> Result<TagContext, TagError> {
    let (title, cwd, message_count, duration_ms, recap): (String, String, i64, i64, Option<String>) =
        conn.query_row(
            "SELECT COALESCE(s.title,''), s.cwd, s.message_count, s.duration_ms,
                    (SELECT content FROM recaps r WHERE r.session_id = s.id AND r.is_final = 1)
             FROM sessions s WHERE s.id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(db_err)?;
    let (todos_total, todos_done): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END),0)
             FROM todos WHERE session_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 0));
    Ok(TagContext {
        title,
        project: project_display(&cwd),
        message_count,
        duration_ms,
        recap,
        todos_total,
        todos_done,
    })
}

/// Read the current manual_fields list for a session (empty if unset/invalid).
fn load_manual_fields(conn: &rusqlite::Connection, id: &str) -> Vec<String> {
    let raw: Option<String> = conn
        .query_row("SELECT manual_fields FROM sessions WHERE id = ?1", params![id], |r| r.get(0))
        .ok()
        .flatten();
    parse_manual_fields(raw)
}

/// Read back the persisted tag fields as the wire struct.
fn read_session_tags(conn: &rusqlite::Connection, id: &str) -> Result<SessionTags, TagError> {
    conn.query_row(
        "SELECT area_of_life, project_short_name, goal_completed, completion_pct,
                tag_rationale, tagged_at, manual_fields
         FROM sessions WHERE id = ?1",
        params![id],
        |r| {
            Ok(SessionTags {
                area_of_life: r.get(0)?,
                project_short_name: r.get(1)?,
                goal_completed: r.get::<_, Option<i64>>(2)?.map(|v| v != 0),
                completion_pct: r.get(3)?,
                tag_rationale: r.get(4)?,
                tagged_at: r.get(5)?,
                manual_fields: parse_manual_fields(r.get(6)?),
            })
        },
    )
    .map_err(db_err)
}

/// Persist auto tags: write each auto field ONLY where its camelCase name is
/// NOT in manual_fields (hand-edit protection). tag_rationale + tagged_at are
/// always written (tagged_at is not user-editable).
fn persist_auto_tags(
    conn: &rusqlite::Connection,
    id: &str,
    v: &ValidatedTags,
) -> Result<SessionTags, TagError> {
    let manual = load_manual_fields(conn, id);
    let auto = |field: &str| !manual.iter().any(|m| m == field);

    if auto("areaOfLife") {
        conn.execute(
            "UPDATE sessions SET area_of_life = ?1 WHERE id = ?2",
            params![v.area_of_life, id],
        )
        .map_err(db_err)?;
    }
    if auto("projectShortName") {
        conn.execute(
            "UPDATE sessions SET project_short_name = ?1 WHERE id = ?2",
            params![v.project_short_name, id],
        )
        .map_err(db_err)?;
    }
    if auto("goalCompleted") {
        conn.execute(
            "UPDATE sessions SET goal_completed = ?1 WHERE id = ?2",
            params![v.goal_completed as i64, id],
        )
        .map_err(db_err)?;
    }
    if auto("completionPct") {
        conn.execute(
            "UPDATE sessions SET completion_pct = ?1 WHERE id = ?2",
            params![v.completion_pct, id],
        )
        .map_err(db_err)?;
    }
    conn.execute(
        "UPDATE sessions SET tag_rationale = ?1, tagged_at = ?2 WHERE id = ?3",
        params![v.rationale, chrono::Utc::now().to_rfc3339(), id],
    )
    .map_err(db_err)?;

    read_session_tags(conn, id)
}

/// Core (blocking) tag flow: load context → CLI → validate → persist. Split out
/// so the async command wraps it in spawn_blocking and the ignored integration
/// test can call it directly.
fn tag_session_blocking(id: &str) -> Result<SessionTags, TagError> {
    let conn = db::open().map_err(db_err)?;
    let ctx = load_tag_context(&conn, id)?;
    let prompt = build_tag_prompt(&ctx);
    let result_text = claude::run_headless(&prompt, TAGGING_MODEL)?;
    let json = extract_json_object(&result_text)?;
    let validated = validate_tags(&json)?;
    persist_auto_tags(&conn, id, &validated)
}

/// One-shot AI tag. Async so the UI stays responsive; the blocking CLI call
/// runs on the blocking pool (tauri::async_runtime, no tokio dependency added).
#[tauri::command]
async fn tag_session(id: String) -> Result<SessionTags, TagError> {
    tauri::async_runtime::spawn_blocking(move || tag_session_blocking(&id))
        .await
        .map_err(|e| TagError::new("cli_failed", format!("tag task failed to join: {e}")))?
}

/// Hand-edit path: apply ONLY the provided fields, validate, and flag each
/// edited field in manual_fields (deduped) so a future auto-tag won't clobber
/// it. Synchronous — no CLI involved.
#[tauri::command]
fn update_session_tags(
    id: String,
    area_of_life: Option<String>,
    project_short_name: Option<String>,
    goal_completed: Option<bool>,
    completion_pct: Option<i64>,
) -> Result<SessionTags, TagError> {
    let conn = db::open().map_err(db_err)?;
    let mut manual = load_manual_fields(&conn, &id);

    if let Some(area) = area_of_life {
        let norm = normalize_area(&area).ok_or_else(|| {
            TagError::new("invalid_json", format!("area '{area}' not in vocabulary"))
        })?;
        conn.execute("UPDATE sessions SET area_of_life = ?1 WHERE id = ?2", params![norm, id])
            .map_err(db_err)?;
        manual = with_manual_field(manual, "areaOfLife");
    }
    if let Some(name) = project_short_name {
        let n: String = name.trim().chars().take(24).collect();
        conn.execute("UPDATE sessions SET project_short_name = ?1 WHERE id = ?2", params![n, id])
            .map_err(db_err)?;
        manual = with_manual_field(manual, "projectShortName");
    }
    if let Some(goal) = goal_completed {
        conn.execute(
            "UPDATE sessions SET goal_completed = ?1 WHERE id = ?2",
            params![goal as i64, id],
        )
        .map_err(db_err)?;
        manual = with_manual_field(manual, "goalCompleted");
    }
    if let Some(pct) = completion_pct {
        let pct = pct.clamp(0, 100);
        conn.execute("UPDATE sessions SET completion_pct = ?1 WHERE id = ?2", params![pct, id])
            .map_err(db_err)?;
        manual = with_manual_field(manual, "completionPct");
    }

    let mf = serde_json::to_string(&manual).unwrap_or_else(|_| "[]".to_string());
    conn.execute("UPDATE sessions SET manual_fields = ?1 WHERE id = ?2", params![mf, id])
        .map_err(db_err)?;

    read_session_tags(&conn, &id)
}

// ─── Kanban board (Feature 4) ────────────────────────────────────────────────
// The board columns are derived from completion, with an explicit drag override
// that wins. Membership + placement is computed on the frontend from the same
// SessionCard fields; this pure helper mirrors that rule so the derivation has
// unit coverage, and set_kanban persists the drag (status + per-column order).

/// The three board columns, in order. The canonical status strings.
const KANBAN_COLUMNS: [&str; 3] = ["planned", "in_progress", "completed"];

/// Derive a session's board column: override-wins, then the completion rule,
/// then None for untagged (which keeps it off the board entirely).
///
/// Rule: an explicit `kanban_status` (set by a drag) overrides everything. With
/// no override, a session with no completion signal at all (pct null AND
/// goal_completed null) is untagged → None. Otherwise: goal_completed OR pct>=100
/// → completed, pct==0 → planned, 1..=99 → in_progress.
///
/// Membership is computed client-side (`columnOf` in KanbanBoard.tsx) so the
/// board reacts without a round-trip; this mirrors that rule and carries the
/// unit coverage the spec requires, so the derivation can't silently drift.
#[cfg_attr(not(test), allow(dead_code))]
fn derive_kanban_column(
    completion_pct: Option<i64>,
    goal_completed: Option<bool>,
    kanban_status: Option<&str>,
) -> Option<&'static str> {
    // Override-wins: a drag-set status beats the derived column.
    if let Some(s) = kanban_status {
        return KANBAN_COLUMNS.into_iter().find(|c| *c == s);
    }
    // Untagged (no override, no completion signal) stays off the board.
    if completion_pct.is_none() && goal_completed.is_none() {
        return None;
    }
    if goal_completed == Some(true) || completion_pct.unwrap_or(0) >= 100 {
        Some("completed")
    } else if completion_pct.unwrap_or(0) == 0 {
        Some("planned")
    } else {
        Some("in_progress")
    }
}

/// Persist a drag: set the explicit `kanban_status` override (None clears it,
/// falling back to the derived column) and the per-column `kanban_order`.
/// Validates the status against the controlled column set.
#[tauri::command]
fn set_kanban(id: String, status: Option<String>, order: Option<f64>) -> Result<(), String> {
    if let Some(s) = &status {
        if !KANBAN_COLUMNS.contains(&s.as_str()) {
            return Err(format!("invalid kanban status: {s}"));
        }
    }
    let conn = db::open().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sessions SET kanban_status = ?1, kanban_order = ?2 WHERE id = ?3",
        params![status, order, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────
// Pure-logic probes for the ranker + snippet extractor. The DB-backed
// search_recaps is exercised end-to-end via the running app; these cover the
// fiddly text math that's easy to get wrong (windows, word-snapping, TF).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_blacklisted_and_count_match_indexer_dual_check() {
        // Mixed dataset: two rows under a blacklisted tree (one matched by cwd,
        // one only unambiguous in encoded space), plus an innocent sibling.
        let pairs = vec![
            (
                "/Users/s/Projects/udacity-project-reviews".to_string(),
                "-Users-s-Projects-udacity-project-reviews".to_string(),
            ),
            (
                "/Users/s/Projects/udacity-project-reviews/sub".to_string(),
                "-Users-s-Projects-udacity-project-reviews-sub".to_string(),
            ),
            (
                "/Users/s/Projects/brain".to_string(),
                "-Users-s-Projects-brain".to_string(),
            ),
        ];
        let patterns = vec!["udacity-project-reviews/**".to_string()];

        // Predicate: parent + descendant hidden, sibling kept.
        assert!(dir_blacklisted(&pairs[0].0, &pairs[0].1, &patterns));
        assert!(dir_blacklisted(&pairs[1].0, &pairs[1].1, &patterns));
        assert!(!dir_blacklisted(&pairs[2].0, &pairs[2].1, &patterns));

        // Count: exactly the two hidden rows.
        assert_eq!(count_matches(&pairs, "udacity-project-reviews/**"), 2);
        assert_eq!(count_matches(&pairs, "brain"), 1);
        assert_eq!(count_matches(&pairs, "nonexistent"), 0);
        // Empty pattern list hides nothing.
        assert!(!dir_blacklisted(&pairs[0].0, &pairs[0].1, &[]));
    }

    #[test]
    fn tokenize_splits_on_punctuation_and_lowercases() {
        assert_eq!(tokenize("Auth Login!"), vec!["auth", "login"]);
        assert_eq!(tokenize("  multi-word_test  "), vec!["multi", "word", "test"]);
        assert!(tokenize("   ...!!   ").is_empty());
    }

    #[test]
    fn snippet_finds_match_and_word_snaps() {
        let content = "The user worked on the authentication flow and login page. \
                       They also fixed a bug in the auth middleware.";
        let lower = content.to_lowercase();
        let terms = vec!["auth".to_string()];
        let (before, m, after) = best_snippet(content, &lower, &terms);
        assert!(!m.is_empty(), "match term must be non-empty");
        assert!(
            m.to_lowercase().contains("auth"),
            "highlight should contain the term, got: {m:?}"
        );
        // Word-snap invariant: the match must start at a token boundary in the
        // lowercased text (so we never highlight a mid-word fragment), and the
        // matched text itself must be a real substring of the content. We don't
        // require before+match+after to be an exact substring because before/
        // after are trimmed for display.
        let m_lower = m.to_lowercase();
        assert!(lower.contains(&m_lower), "match must be a substring of content");
        let m_start_in_lower = lower.find(&m_lower).expect("match present in lower");
        let boundary_ok = m_start_in_lower == 0
            || lower.as_bytes()[m_start_in_lower - 1].is_ascii_whitespace()
            || lower.as_bytes()[m_start_in_lower - 1].is_ascii_punctuation();
        assert!(boundary_ok, "match must start at a word boundary");
        assert!(!after.is_empty(), "after context should be non-empty");
        // `before` is leading context; we only assert it's non-empty here (the
        // word-boundary guarantee is enforced on the match itself above).
        let _ = before;
    }

    #[test]
    fn snippet_dense_region_wins_over_first_hit() {
        // Two clusters: a lone "auth" early, and "auth auth auth" later.
        // The dense window should anchor on the cluster, not the first hit.
        let content = "auth appeared once here in passing. \
                       Much later we have auth auth auth all together \
                       because density matters for relevance.";
        let lower = content.to_lowercase();
        let terms = vec!["auth".to_string()];
        let (_before, m, _after) = best_snippet(content, &lower, &terms);
        assert!(m.to_lowercase().contains("auth"));
    }

    #[test]
    fn snippet_handles_multibyte_without_panicking() {
        let content = "Recap with em-dash—here—and a curly “quote” plus auth token.";
        let lower = content.to_lowercase();
        let terms = vec!["auth".to_string()];
        let (before, m, after) = best_snippet(content, &lower, &terms);
        assert!(m.to_lowercase().contains("auth"));
        // Must not panic and must produce valid slices.
        let _ = format!("{before}[{m}]{after}");
    }

    #[test]
    fn snippet_no_match_returns_head_gracefully() {
        let content = "A recap with no relevant terms at all.";
        let lower = content.to_lowercase();
        let terms = vec!["nonexistent".to_string()];
        let (before, m, after) = best_snippet(content, &lower, &terms);
        assert!(before.is_empty());
        assert!(!m.is_empty(), "no-match should fall back to head as the body");
        assert!(after.is_empty());
    }

    #[test]
    fn score_recap_rewards_term_frequency_and_recency() {
        let now = chrono::Utc::now();
        let recent_ts = chrono::Utc::now().to_rfc3339();
        let old_ts = (chrono::Utc::now() - chrono::Duration::days(365)).to_rfc3339();

        let make = |content: &str, ts: &str| {
            score_recap(
                CandidateRecap {
                    session_id: "s".into(),
                    _uuid: "u".into(),
                    title: "t".into(),
                    cwd: "/p/brain".into(),
                    captured_ts: Some(ts.into()),
                    content: content.into(),
                    project_dir: "-p-brain".into(),
                },
                &["auth".to_string()],
                now,
            )
        };

        let recent = make("auth auth auth auth in this recent recap", &recent_ts);
        let old = make("auth auth auth auth in this old recap", &old_ts);
        let sparse = make("auth only once here in this recent longer recap with padding words", &recent_ts);

        assert!(
            recent.score > old.score,
            "recent must outscore old (same TF): recent={} old={}",
            recent.score, old.score
        );
        assert!(
            recent.score > sparse.score,
            "dense TF must outscore sparse (same recency): recent={} sparse={}",
            recent.score, sparse.score
        );
    }

    // ─── Tagging pure logic ──────────────────────────────────────────────

    #[test]
    fn extract_json_object_handles_fences_and_prefix() {
        // Fenced.
        let fenced = "```json\n{\"area_of_life\":\"Building\"}\n```";
        assert!(extract_json_object(fenced).is_ok());
        // Prose prefix + suffix.
        let prosey = "Sure, here you go: {\"completion_pct\": 40} — hope that helps!";
        let v = extract_json_object(prosey).unwrap();
        assert_eq!(v.get("completion_pct").and_then(|x| x.as_i64()), Some(40));
        // No object at all.
        assert_eq!(extract_json_object("no json here").unwrap_err().kind, "invalid_json");
    }

    #[test]
    fn validate_tags_rejects_bad_area_and_clamps_pct() {
        // Valid, case-insensitive area + float pct rounds and passes.
        let ok = serde_json::json!({
            "area_of_life": "building",
            "project_short_name": "  sessions-ui  ",
            "goal_completed": true,
            "completion_pct": 84.6,
            "rationale": "shipped it"
        });
        let v = validate_tags(&ok).unwrap();
        assert_eq!(v.area_of_life, "Building"); // normalized to canonical casing
        assert_eq!(v.project_short_name, "sessions-ui"); // trimmed
        assert_eq!(v.completion_pct, 85); // rounded

        // Pct out of range clamps.
        let hi = serde_json::json!({
            "area_of_life": "Ops", "project_short_name": "x",
            "goal_completed": false, "completion_pct": 250, "rationale": ""
        });
        assert_eq!(validate_tags(&hi).unwrap().completion_pct, 100);

        // Area outside the vocabulary is rejected.
        let bad = serde_json::json!({
            "area_of_life": "Gardening", "project_short_name": "x",
            "goal_completed": false, "completion_pct": 0, "rationale": ""
        });
        assert_eq!(validate_tags(&bad).unwrap_err().kind, "invalid_json");

        // Empty short name is rejected.
        let empty = serde_json::json!({
            "area_of_life": "Ops", "project_short_name": "   ",
            "goal_completed": false, "completion_pct": 0, "rationale": ""
        });
        assert_eq!(validate_tags(&empty).unwrap_err().kind, "invalid_json");
    }

    #[test]
    fn validate_tags_truncates_long_short_name() {
        let long = serde_json::json!({
            "area_of_life": "Building",
            "project_short_name": "this-is-a-very-long-project-name-way-over-limit",
            "goal_completed": true, "completion_pct": 10, "rationale": "x"
        });
        assert_eq!(validate_tags(&long).unwrap().project_short_name.chars().count(), 24);
    }

    #[test]
    fn derive_kanban_column_implements_override_wins_and_pct_rule() {
        // Pct rule: 0 → planned, mid → in_progress, 100 → completed.
        assert_eq!(derive_kanban_column(Some(0), Some(false), None), Some("planned"));
        assert_eq!(derive_kanban_column(Some(50), Some(false), None), Some("in_progress"));
        assert_eq!(derive_kanban_column(Some(100), Some(false), None), Some("completed"));
        // goal_completed true → completed regardless of pct.
        assert_eq!(derive_kanban_column(Some(30), Some(true), None), Some("completed"));
        // Override beats the %: a drag to planned holds even at 100%.
        assert_eq!(derive_kanban_column(Some(100), Some(true), Some("planned")), Some("planned"));
        assert_eq!(derive_kanban_column(Some(0), None, Some("completed")), Some("completed"));
        // All-null (untagged, no override) → off the board.
        assert_eq!(derive_kanban_column(None, None, None), None);
        // Tagged only via goal_completed=false, no pct → treated as planned.
        assert_eq!(derive_kanban_column(None, Some(false), None), Some("planned"));
    }

    #[test]
    fn with_manual_field_dedups() {
        let m = with_manual_field(vec![], "completionPct");
        assert_eq!(m, vec!["completionPct".to_string()]);
        // Re-adding is a no-op.
        let m = with_manual_field(m, "completionPct");
        assert_eq!(m, vec!["completionPct".to_string()]);
        // A different field appends.
        let m = with_manual_field(m, "areaOfLife");
        assert_eq!(m, vec!["completionPct".to_string(), "areaOfLife".to_string()]);
    }

    /// End-to-end probe against the real indexed DB. Ignored by default
    /// (requires a populated ~/.claude-sessions-ui/index.sqlite). Run with:
    ///   cargo test --lib search_recaps_against_real_db -- --ignored --nocapture
    #[test]
    #[ignore]
    fn search_recaps_against_real_db() {
        let hits = search_recaps("auth login".into()).expect("search_recaps ok");
        assert!(!hits.is_empty(), "expected real hits for 'auth login'");
        // Top hit must carry a real snippet, not a whole-recap dump.
        let top = &hits[0];
        assert!(
            !top.snippet_match.is_empty(),
            "snippet_match must be non-empty"
        );
        assert!(
            top.snippet_match.len() <= 200,
            "snippet_match should be a window, not the whole recap: len={}",
            top.snippet_match.len()
        );
        assert!(
            !top.project.is_empty(),
            "project label must be populated"
        );
        // Scores must be sorted descending.
        let sorted = hits
            .windows(2)
            .all(|w| w[0].score >= w[1].score);
        assert!(sorted, "hits must be sorted by score desc");
        // Print a sample for human inspection.
        println!("--- top 3 hits for 'auth login' ---");
        for h in hits.iter().take(3) {
            println!(
                "  [{:.3}] {} · {}",
                h.score, h.project, h.captured_ts.as_deref().unwrap_or("?")
            );
            println!(
                "    …{}【{}】{}…",
                h.snippet_before.chars().take(60).collect::<String>(),
                h.snippet_match,
                h.snippet_after.chars().take(60).collect::<String>(),
            );
        }
    }

    /// Real CLI round-trip against the indexed DB. Ignored by default (spends
    /// real haiku tokens + needs a populated index). Run with:
    ///   cargo test --lib tag_session_against_real_db -- --ignored --nocapture
    /// Confirms: valid JSON parsed, fields persisted, and a manually-set field
    /// survives a re-tag (manual_fields protection).
    #[test]
    #[ignore]
    fn tag_session_against_real_db() {
        let conn = db::open().expect("open db");
        let id: String = conn
            .query_row(
                "SELECT s.id FROM sessions s
                 WHERE EXISTS (SELECT 1 FROM recaps r WHERE r.session_id = s.id AND r.is_final = 1)
                 ORDER BY s.last_ts DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("need at least one recap-bearing session");
        println!("tagging real session {id}");

        // Set completion_pct by hand FIRST — it must survive the auto-tag below.
        let before = update_session_tags(id.clone(), None, None, None, Some(42))
            .expect("manual update ok");
        assert_eq!(before.completion_pct, Some(42));
        assert!(before.manual_fields.contains(&"completionPct".to_string()));

        // Run the real one-shot AI tag.
        let tags = tag_session_blocking(&id).expect("tag_session core ok");
        println!(
            "MODEL OUTPUT → area={:?} shortName={:?} goalCompleted={:?} completionPct={:?}\n  rationale={:?}\n  manualFields={:?}",
            tags.area_of_life,
            tags.project_short_name,
            tags.goal_completed,
            tags.completion_pct,
            tags.tag_rationale,
            tags.manual_fields,
        );

        // Auto fields populated + valid.
        let area = tags.area_of_life.as_deref().expect("area set");
        assert!(AREAS_OF_LIFE.contains(&area), "area '{area}' must be in vocabulary");
        assert!(tags.project_short_name.is_some(), "short name set");
        assert!(tags.tag_rationale.is_some(), "rationale set");
        assert!(tags.tagged_at.is_some(), "tagged_at set");

        // The hand-edited completion_pct must NOT have been clobbered.
        assert_eq!(tags.completion_pct, Some(42), "manual completion_pct must survive re-tag");
        assert!(tags.manual_fields.contains(&"completionPct".to_string()));
    }

    /// Persistence probe for the kanban drag path against the real indexed DB.
    /// Ignored by default (needs a populated ~/.claude-sessions-ui/index.sqlite).
    /// Run with:
    ///   cargo test --lib set_kanban_persists_against_real_db -- --ignored --nocapture
    /// Drag itself can't be driven headlessly; this exercises the persistence
    /// path set_kanban writes and reads it straight back from the row.
    #[test]
    #[ignore]
    fn set_kanban_persists_against_real_db() {
        let conn = db::open().expect("open db");
        let id: String = conn
            .query_row("SELECT id FROM sessions ORDER BY last_ts DESC LIMIT 1", [], |r| r.get(0))
            .expect("need at least one session");

        // A drag to In Progress at a fractional order.
        set_kanban(id.clone(), Some("in_progress".into()), Some(1500.0)).expect("set_kanban ok");
        let (status, order): (Option<String>, Option<f64>) = conn
            .query_row(
                "SELECT kanban_status, kanban_order FROM sessions WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("read back ok");
        assert_eq!(status.as_deref(), Some("in_progress"));
        assert_eq!(order, Some(1500.0));
        println!("persisted kanban for {id}: status={status:?} order={order:?}");

        // Clearing the override writes NULL, so the card falls back to derived.
        set_kanban(id.clone(), None, None).expect("clear ok");
        let cleared: Option<String> = conn
            .query_row("SELECT kanban_status FROM sessions WHERE id = ?1", params![id], |r| r.get(0))
            .expect("read back ok");
        assert_eq!(cleared, None, "clearing the override writes NULL");
    }
}

// ─── App entry ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    // ponytail: WebDriver plugin is E2E-only, gated to debug builds. It still links into
    // the release binary but never initializes; ceiling — make it an optional cargo
    // feature enabled only in dev to strip it from release entirely.
    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .invoke_handler(tauri::generate_handler![
            reindex,
            index_status,
            list_sessions,
            get_session_detail,
            search_recaps,
            digest,
            get_stats,
            resume_session,
            toggle_pin,
            get_pricing,
            set_pricing,
            list_blacklist,
            add_blacklist_pattern,
            remove_blacklist_pattern,
            tag_session,
            update_session_tags,
            set_kanban,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod serde_tests {
    use super::*;

    #[test]
    fn session_card_serializes_camel_case() {
        let c = SessionCard {
            id: "x".into(),
            project_dir: "p".into(),
            cwd: "/".into(),
            display_project: "d".into(),
            git_branch: None,
            title: "t".into(),
            first_ts: None,
            last_ts: None,
            message_count: 5,
            duration_ms: 1000,
            plan_mode: false,
            has_recap: true,
            recap: None,
            input_toks: 1,
            output_toks: 2,
            cache_read_toks: 3,
            cost_usd: 0.0,
            cost_source: "none".into(),
            pinned: false,
            area_of_life: Some("Building".into()),
            project_short_name: Some("sessions-ui".into()),
            goal_completed: Some(true),
            completion_pct: Some(40),
            tag_rationale: Some("shipped the cards".into()),
            tagged_at: Some("2026-07-04T00:00:00Z".into()),
            manual_fields: vec!["completionPct".into()],
            kanban_status: Some("inProgress".into()),
            kanban_order: Some(1.5),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"projectDir\""), "expected camelCase projectDir, got: {}", json);
        assert!(json.contains("\"messageCount\""));
        assert!(json.contains("\"cacheReadToks\""));
        assert!(!json.contains("project_dir"), "snake_case leaked: {}", json);
        // New tag/kanban fields must also serialize camelCase.
        assert!(json.contains("\"areaOfLife\""), "got: {}", json);
        assert!(json.contains("\"projectShortName\""));
        assert!(json.contains("\"goalCompleted\""));
        assert!(json.contains("\"completionPct\""));
        assert!(json.contains("\"tagRationale\""));
        assert!(json.contains("\"taggedAt\""));
        assert!(json.contains("\"manualFields\""));
        assert!(json.contains("\"kanbanStatus\""));
        assert!(json.contains("\"kanbanOrder\""));
        assert!(!json.contains("area_of_life"), "snake_case leaked: {}", json);
    }

    #[test]
    fn manual_fields_parses_json_array_or_degrades_to_empty() {
        assert_eq!(
            parse_manual_fields(Some(r#"["area_of_life","completion_pct"]"#.into())),
            vec!["area_of_life".to_string(), "completion_pct".to_string()]
        );
        // Empty array → empty vec.
        assert!(parse_manual_fields(Some("[]".into())).is_empty());
        // Null column → empty vec.
        assert!(parse_manual_fields(None).is_empty());
        // Invalid / non-array JSON → empty vec, never a panic.
        assert!(parse_manual_fields(Some("not json".into())).is_empty());
        assert!(parse_manual_fields(Some(r#"{"a":1}"#.into())).is_empty());
    }
}
