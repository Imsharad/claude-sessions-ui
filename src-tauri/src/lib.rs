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
    pub captured_ts: Option<String>,
    pub snippet: String, // recap content (or a window around the match)
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
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let conn = db::open().map_err(|e| e.to_string())?;
    let like = format!("%{}%", query);
    let mut stmt = conn
        .prepare(
            "SELECT r.session_id, s.title, s.cwd, r.captured_ts, r.content
             FROM recaps r
             JOIN sessions s ON s.id = r.session_id
             WHERE r.content LIKE ?1
             ORDER BY r.captured_ts DESC NULLS LAST
             LIMIT 100",
        )
        .map_err(|e| e.to_string())?;
    let hits = stmt
        .query_map(params![like], |r| {
            Ok(RecapHit {
                session_id: r.get(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                cwd: r.get(2)?,
                captured_ts: r.get(3)?,
                snippet: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(hits)
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
