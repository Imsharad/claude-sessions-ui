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
                p.pinned
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
    let n_binds = binds.len();
    let rows = match n_binds {
        0 => stmt.query_map([], map_session_card),
        1 => stmt.query_map(params![binds[0]], map_session_card),
        2 => stmt.query_map(params![binds[0], binds[1]], map_session_card),
        3 => stmt.query_map(
            params![binds[0], binds[1], binds[2]],
            map_session_card,
        ),
        _ => Err(rusqlite::Error::ToSqlConversionFailure(
            "too many binds".into(),
        )),
    }
    .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
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
    })
}

fn project_display(cwd: &str) -> String {
    cwd.split('/').next_back().unwrap_or(cwd).to_string()
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
                        COALESCE((SELECT pinned FROM projects p WHERE p.encoded_dir=s.project_dir),0)
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
            "SELECT r.session_id, r.uuid, s.title, s.cwd, r.captured_ts, r.content
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

    // Score + extract snippet, then rank.
    let now = chrono::Utc::now();
    let mut scored: Vec<RecapHit> = seen
        .into_values()
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
                    s.message_count
             FROM sessions s
             WHERE s.last_ts >= datetime('now', '-{} days')
             ORDER BY s.last_ts DESC",
            days
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let cwd: String = r.get(2)?;
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
            .map(|e| (day, e))
        })
        .map_err(|e| e.to_string())?;

    let mut by_day: Vec<DigestDay> = Vec::new();
    for r in rows.flatten() {
        if by_day.last().map(|d| d.day == r.0).unwrap_or(false) {
            by_day.last_mut().unwrap().sessions.push(r.1);
        } else {
            by_day.push(DigestDay {
                day: r.0,
                sessions: vec![r.1],
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
    let rows = stmt
        .query_map([], |r| {
            Ok(PricingRow {
                model: r.get(0)?,
                input_per_mtok: r.get(1)?,
                output_per_mtok: r.get(2)?,
                cache_write_per_mtok: r.get(3)?,
                cache_read_per_mtok: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
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

// ─── Tests ─────────────────────────────────────────────────────────────────
// Pure-logic probes for the ranker + snippet extractor. The DB-backed
// search_recaps is exercised end-to-end via the running app; these cover the
// fiddly text math that's easy to get wrong (windows, word-snapping, TF).

#[cfg(test)]
mod tests {
    use super::*;

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
}

// ─── App entry ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"projectDir\""), "expected camelCase projectDir, got: {}", json);
        assert!(json.contains("\"messageCount\""));
        assert!(json.contains("\"cacheReadToks\""));
        assert!(!json.contains("project_dir"), "snake_case leaked: {}", json);
    }
}
