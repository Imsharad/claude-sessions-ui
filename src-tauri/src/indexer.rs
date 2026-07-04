//! Indexes Claude Code session JSONL files into SQLite.
//!
//! Sources (all measured, see PLAN):
//!   ~/.claude/projects/*/*.jsonl   — sessions, usage, recaps, files, todos, turns, errors
//!   ~/.claude.json                 — per-project last-session cost (authoritative)
//!   ~/.brain/logs/usage.jsonl      — brain telemetry (joined later, not here)
//!
//! Two scan modes:
//!   - full:    parse every file, rebuild child tables
//!   - incremental: stat every file; only re-parse files whose mtime advanced
//!                  since last index. Cheap (stat ~830 inodes).

use crate::db::set_meta;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Root of Claude Code's per-project session storage.
fn projects_root() -> PathBuf {
    dirs::home_dir()
        .expect("no home")
        .join(".claude")
        .join("projects")
}

/// Top-level ~/.claude.json (parsed once per scan; carries last-session cost).
fn claude_json_path() -> PathBuf {
    dirs::home_dir().expect("no home").join(".claude.json")
}

pub struct ScanStats {
    pub files_seen: usize,
    pub files_reindexed: usize,
    pub files_skipped_uptodate: usize,
    pub sessions_upserted: usize,
    pub duration_ms: u128,
    pub mode: &'static str, // "full" | "incremental"
    pub error: Option<String>,
}

/// Run an index pass. If `force_full`, re-parse everything; else do an mtime
/// check and only re-parse changed files. Safe to call repeatedly.
pub fn run(conn: &Connection, force_full: bool) -> ScanStats {
    let t0 = std::time::Instant::now();
    let mode = if force_full { "full" } else { "incremental" };

    // For incremental: map file_path -> known mtime so we can skip unchanged.
    let known_mtimes: HashMap<String, i64> = if force_full {
        HashMap::new()
    } else {
        load_known_mtimes(conn)
    };

    let root = projects_root();
    if !root.exists() {
        return ScanStats {
            files_seen: 0,
            files_reindexed: 0,
            files_skipped_uptodate: 0,
            sessions_upserted: 0,
            duration_ms: t0.elapsed().as_millis(),
            mode,
            error: Some(format!("projects root not found: {}", root.display())),
        };
    }

    // Load config once: gives us per-project cost/lines data for the projects table.
    let config = load_claude_json();

    let mut files_seen = 0usize;
    let mut files_reindexed = 0usize;
    let mut files_skipped = 0usize;
    let mut sessions_upserted = 0usize;
    let mut last_err: Option<String> = None;

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("jsonl"))
        // Only index top-level session files: <project-dir>/<sessionId>.jsonl.
        // Nested transcripts (<sessionId>/subagents/agent-*.jsonl) are subagent
        // exhaust, not user sessions — they have their own sessionId that doesn't
        // map back to a parent, and including them would double-count work.
        // This means our token totals are a slight undercount vs raw grep, which
        // is honest: "what the user drove" vs "total inference including subagents."
        .filter(|e| {
            // depth from projects root: project-dir/session.jsonl => 2 components
            e.path()
                .strip_prefix(&root)
                .map(|rel| rel.components().count() == 2)
                .unwrap_or(false)
        })
    {
        files_seen += 1;
        let path = entry.path();
        let mtime = match file_mtime_secs(path) {
            Ok(m) => m,
            Err(e) => {
                last_err = Some(format!("stat {}: {}", path.display(), e));
                continue;
            }
        };
        let path_str = path.to_string_lossy().to_string();
        if !force_full {
            if let Some(&known) = known_mtimes.get(&path_str) {
                if known == mtime {
                    files_skipped += 1;
                    continue;
                }
            }
        }
        // Parse + upsert. A failure on ONE file must not abort the whole scan.
        match parse_and_upsert(conn, path, mtime, &config) {
            Ok(true) => {
                files_reindexed += 1;
                sessions_upserted += 1;
            }
            Ok(false) => {
                // file parsed but yielded no usable session (e.g. empty) — count as reindexed effort
                files_reindexed += 1;
            }
            Err(e) => {
                log::warn!("index {} failed: {}", path.display(), e);
                last_err = Some(format!("parse {}: {}", path.display(), e));
            }
        }
    }

    // Rebuild the projects table from the now-current sessions.
    rebuild_projects(conn, &config);

    let dur = t0.elapsed().as_millis();
    set_meta(
        conn,
        "last_scan_ts",
        &Utc::now().to_rfc3339(),
    )
    .ok();
    set_meta(conn, "last_scan_mode", mode).ok();

    ScanStats {
        files_seen,
        files_reindexed,
        files_skipped_uptodate: files_skipped,
        sessions_upserted,
        duration_ms: dur,
        mode,
        error: last_err,
    }
}

fn file_mtime_secs(p: &Path) -> std::io::Result<i64> {
    let md = fs::metadata(p)?;
    Ok(md
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0))
}

fn load_known_mtimes(conn: &Connection) -> HashMap<String, i64> {
    // ponytail: functional row mapping
    conn.prepare("SELECT file_path, file_mtime FROM sessions")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
             .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default()
}

/// Parsed view of ~/.claude.json — only the parts we use (projects dict).
struct ClaudeConfig {
    projects: HashMap<String, ProjectConfig>,
}
struct ProjectConfig {
    last_cost_usd: Option<f64>,
    last_model_usage: Option<Value>, // {model: {costUSD, inputTokens, ...}}
    last_lines_added: Option<i64>,
    last_lines_removed: Option<i64>,
    last_session_id: Option<String>,
}

fn load_claude_json() -> ClaudeConfig {
    // ponytail: combinators and early returns
    let Ok(bytes) = fs::read(claude_json_path()) else { return ClaudeConfig { projects: HashMap::new() } };
    let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { return ClaudeConfig { projects: HashMap::new() } };
    let Some(projs) = v.get("projects").and_then(|p| p.as_object()) else {
        return ClaudeConfig { projects: HashMap::new() };
    };

    let projects = projs.iter().map(|(cwd, cfg)| {
        (cwd.clone(),
            ProjectConfig {
                last_cost_usd: cfg.get("lastCost").and_then(|c| c.as_f64()),
                last_model_usage: cfg.get("lastModelUsage").cloned(),
                last_lines_added: cfg.get("lastLinesAdded").and_then(|c| c.as_i64()),
                last_lines_removed: cfg.get("lastLinesRemoved").and_then(|c| c.as_i64()),
                last_session_id: cfg.get("lastSessionId")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string()),
            }
        )
    }).collect();
    ClaudeConfig { projects }
}

/// The encoded dir name (e.g. "-Users-sharad-...") → decoded cwd path.
fn decode_project_dir(encoded: &str) -> String {
    // Claude Code encodes cwd by replacing "/" with "-". A leading "/" becomes
    // a leading "-", so we replace all "-" with "/" and we get the path back.
    // (Caveat: paths containing "-" are ambiguous, but in practice this is how
    // CC encodes and it round-trips for typical paths.)
    encoded.replace('-', "/")
}

/// Parse one JSONL file and write all its signals. Returns Ok(true) if a
/// session row was upserted.
fn parse_and_upsert(
    conn: &Connection,
    path: &Path,
    mtime: i64,
    config: &ClaudeConfig,
) -> Result<bool, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(false);
    }

    // Project dir = parent dir name; cwd decoded from it.
    let project_dir = path
        .parent()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()))
        .unwrap_or("")
        .to_string();
    let project_cwd = decode_project_dir(&project_dir);

    // First pass: gather everything in one scan of the lines.
    let mut session_id: Option<String> = None;
    let mut title: Option<String> = None;
    let mut last_prompt: Option<String> = None;
    let mut first_user_msg: Option<String> = None;
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut message_count: i64 = 0;
    let mut plan_mode = false;
    let mut final_todos: Option<Vec<Value>> = None;
    let mut turn_idx = 0i64;
    let mut turns: Vec<(i64, Option<i64>, Option<i64>)> = Vec::new();
    let mut errors: Vec<(String, Option<i64>, Option<i64>)> = Vec::new();
    let mut recaps: Vec<(String, Option<String>, String)> = Vec::new(); // (uuid, ts, content)
    let mut files: HashMap<String, i64> = HashMap::new();

    // per-(session,model) usage accumulator
    let mut usage: HashMap<String, UsageAcc> = HashMap::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Cheap pre-filter: skip lines that can't be useful metadata and aren't
        // messages. Avoids parsing multi-MB files line-by-line through serde
        // for the bulk that are file-history-snapshot noise etc.
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue, // torn line; tolerate
        };

        // ponytail: lazy assignment
        if session_id.is_none() { session_id = v.get("sessionId").and_then(|s| s.as_str()).map(String::from); }
        if cwd.is_none() { cwd = v.get("cwd").and_then(|s| s.as_str()).map(String::from); }
        if git_branch.is_none() { git_branch = v.get("gitBranch").and_then(|s| s.as_str()).map(String::from); }

        let ts = v.get("timestamp").and_then(|s| s.as_str()).map(|s| s.to_string());
        if let Some(ref t) = ts {
            if first_ts.is_none() {
                first_ts = Some(t.clone());
            }
            last_ts = Some(t.clone());
        }

        let ltype = v.get("type").and_then(|s| s.as_str()).unwrap_or("");
        match ltype {
            "user" | "assistant" => {
                message_count += 1;
                if first_user_msg.is_none() && ltype == "user" {
                    // Skip meta user messages (isMeta=true) — they're tool echoes.
                    let is_meta = v.get("isMeta").and_then(|b| b.as_bool()).unwrap_or(false);
                    if !is_meta {
                        first_user_msg = extract_message_text(&v);
                    }
                }
                // assistant carries usage + model + (sometimes) TodoWrite tool_use
                if ltype == "assistant" {
                    if let Some(model) = v
                        .get("message")
                        .and_then(|m| m.get("model"))
                        .and_then(|s| s.as_str())
                    {
                        let acc = usage.entry(model.to_string()).or_default();
                        if let Some(u) = v.get("message").and_then(|m| m.get("usage")) {
                            acc.input += u.get("input_tokens").and_then(|n| n.as_i64()).unwrap_or(0);
                            acc.output += u.get("output_tokens").and_then(|n| n.as_i64()).unwrap_or(0);
                            acc.cache_create += u
                                .get("cache_creation_input_tokens")
                                .and_then(|n| n.as_i64())
                                .unwrap_or(0);
                            acc.cache_read += u
                                .get("cache_read_input_tokens")
                                .and_then(|n| n.as_i64())
                                .unwrap_or(0);
                        }
                    }
                    // TodoWrite tool_use: the full list is re-persisted on every
                    // call, so the last call in the file wins (= final todo state).
                    if let Some(tool) = v
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_array())
                        .and_then(|arr| {
                            arr.iter().find(|b| {
                                b.get("type").and_then(|s| s.as_str()) == Some("tool_use")
                                    && b.get("name").and_then(|s| s.as_str()) == Some("TodoWrite")
                            })
                        })
                    {
                        if let Some(todos) = tool
                            .get("input")
                            .and_then(|i| i.get("todos"))
                            .and_then(|t| t.as_array())
                        {
                            final_todos = Some(todos.clone());
                        }
                    }
                }
            }
            "ai-title" => {
                if title.is_none() { title = v.get("aiTitle").and_then(|s| s.as_str()).map(String::from); }
            }
            "last-prompt" => {
                // ponytail: last occurrence wins, no need for is_none check
                if let Some(s) = v.get("lastPrompt").and_then(|s| s.as_str()) {
                    last_prompt = Some(s.to_string());
                }
            }
            "permission-mode" => {
                if v.get("permissionMode").and_then(|s| s.as_str()) == Some("plan") {
                    plan_mode = true;
                }
            }
            "system" => {
                let sub = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
                match sub {
                    "away_summary" => {
                        // THE RECAP. Strip the trailing config hint if present.
                        let mut content = v
                            .get("content")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let hint = " (disable recaps in /config)";
                        if content.ends_with(hint) {
                            content.truncate(content.len() - hint.len());
                        }
                        let uuid = v.get("uuid").and_then(|s| s.as_str()).unwrap_or("").to_string();
                        recaps.push((uuid, ts.clone(), content));
                    }
                    "turn_duration" => {
                        let dur_ms = v.get("durationMs").and_then(|n| n.as_i64());
                        let mc = v.get("messageCount").and_then(|n| n.as_i64());
                        turns.push((turn_idx, dur_ms, mc));
                        turn_idx += 1;
                    }
                    "api_error" => {
                        let kind = v
                            .get("error")
                            .and_then(|e| e.as_str())
                            .unwrap_or("api_error")
                            .to_string();
                        let attempt = v.get("retryAttempt").and_then(|n| n.as_i64());
                        let retry_in = v.get("retryInMs").and_then(|n| n.as_i64());
                        errors.push((kind, attempt, retry_in));
                    }
                    _ => {}
                }
            }
            "file-history-snapshot" => {
                if let Some(snap) = v.get("snapshot") {
                    if let Some(backups) = snap.get("trackedFileBackups").and_then(|t| t.as_object())
                    {
                        for file_path in backups.keys() {
                            *files.entry(file_path.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let Some(sid) = session_id else {
        // No sessionId on any line — not a real session file. Skip.
        return Ok(false);
    };

    // Resolve title priority: aiTitle > lastPrompt > first user msg.
    // Sanitize each candidate so prompt-content garbage (XML tags, markdown,
    // file refs) never leaks as the display title. Empty after cleaning → skip.
    let candidates = [title.as_deref(), last_prompt.as_deref(), first_user_msg.as_deref()];
    let resolved_title = candidates
        .iter()
        .find_map(|c| c.and_then(sanitize_title).filter(|s| s.chars().count() >= 3))
        .unwrap_or_default();
    let resolved_cwd = cwd.unwrap_or(project_cwd.clone());
    let file_size = raw.len() as i64;

    // Per-session wall duration: last_ts - first_ts (ms). Best-effort parse.
    // Clamp at 0 — sub-ms timestamp jitter can otherwise yield -1ms, and a
    // negative duration has no meaningful interpretation in the UI.
    let duration_ms = match (&first_ts, &last_ts) {
        (Some(a), Some(b)) => iso_diff_ms(a, b).unwrap_or(0).max(0),
        _ => 0,
    };

    // ---- Write session row (replace child rows) ----
    let indexed_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, project_dir, cwd, git_branch, title, first_ts, last_ts,
            message_count, duration_ms, plan_mode, has_recap, file_size, file_path, file_mtime, indexed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
         ON CONFLICT(id) DO UPDATE SET
            project_dir=excluded.project_dir, cwd=excluded.cwd,
            git_branch=excluded.git_branch, title=excluded.title,
            first_ts=excluded.first_ts, last_ts=excluded.last_ts,
            message_count=excluded.message_count, duration_ms=excluded.duration_ms,
            plan_mode=excluded.plan_mode, has_recap=excluded.has_recap,
            file_size=excluded.file_size, file_mtime=excluded.file_mtime,
            indexed_at=excluded.indexed_at",
        params![
            sid,
            project_dir,
            resolved_cwd,
            git_branch,
            truncate_str(&resolved_title, 500),
            first_ts,
            last_ts,
            message_count,
            duration_ms,
            plan_mode as i64,
            !recaps.is_empty() as i64,
            file_size,
            path.to_string_lossy(),
            mtime,
            indexed_at,
        ],
    )
    .map_err(|e| e.to_string())?;

    // ---- Child rows: clear + reinsert (cleanest on re-index) ----
    for table in ["session_usage", "recaps", "files_touched", "todos", "turns", "errors"] {
        conn.execute(
            &format!("DELETE FROM {} WHERE session_id = ?1", table),
            params![sid],
        )
        .map_err(|e| e.to_string())?;
    }

    // Usage + cost. Prefer config's per-model costUSD for the project's last
    // session; else estimate from per-message tokens × pricing.
    let is_last_session_for_project = config
        .projects
        .get(&resolved_cwd)
        .and_then(|p| p.last_session_id.as_deref())
        .map(|id| id == sid)
        .unwrap_or(false);

    for (model, acc) in &usage {
        let (cost_usd, cost_source) = if is_last_session_for_project {
            // Look up costUSD for this model in config.lastModelUsage
            let cfg_cost = config
                .projects
                .get(&resolved_cwd)
                .and_then(|p| p.last_model_usage.as_ref())
                .and_then(|lmu| lmu.get(model))
                .and_then(|m| m.get("costUSD"))
                .and_then(|c| c.as_f64());
            if let Some(c) = cfg_cost {
                (c, "config")
            } else {
                (estimate_cost(conn, model, acc), "estimate")
            }
        } else {
            (estimate_cost(conn, model, acc), "estimate")
        };
        conn.execute(
            "INSERT INTO session_usage
             (session_id, model, input_toks, output_toks, cache_create_toks, cache_read_toks, cost_usd, cost_source)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                sid,
                model,
                acc.input,
                acc.output,
                acc.cache_create,
                acc.cache_read,
                cost_usd,
                cost_source,
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    // Recaps — mark the last one as final.
    let n_recaps = recaps.len();
    for (i, (uuid, ts, content)) in recaps.iter().enumerate() {
        let is_final = (i + 1 == n_recaps) as i64;
        conn.execute(
            "INSERT OR REPLACE INTO recaps (session_id, uuid, captured_ts, content, seq, is_final)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![sid, uuid, ts, content, i as i64, is_final],
        )
        .map_err(|e| e.to_string())?;
    }

    // Files touched
    for (fp, count) in &files {
        conn.execute(
            "INSERT OR REPLACE INTO files_touched (session_id, file_path, snapshots)
             VALUES (?1,?2,?3)",
            params![sid, fp, count],
        )
        .map_err(|e| e.to_string())?;
    }

    // Todos (final state only)
    if let Some(todos) = &final_todos {
        for (seq, t) in todos.iter().enumerate() {
            let content = t.get("content").and_then(|s| s.as_str()).unwrap_or("");
            let status = t.get("status").and_then(|s| s.as_str()).unwrap_or("");
            conn.execute(
                "INSERT OR REPLACE INTO todos (session_id, seq, content, status)
                 VALUES (?1,?2,?3,?4)",
                params![sid, seq as i64, content, status],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    // Turns
    for (idx, dur, mc) in &turns {
        conn.execute(
            "INSERT OR REPLACE INTO turns (session_id, turn_idx, duration_ms, message_count)
             VALUES (?1,?2,?3,?4)",
            params![sid, idx, dur, mc],
        )
        .map_err(|e| e.to_string())?;
    }

    // Errors
    for (i, (kind, attempt, retry_in)) in errors.iter().enumerate() {
        conn.execute(
            "INSERT OR REPLACE INTO errors (session_id, seq, kind, retry_attempt, retry_in_ms)
             VALUES (?1,?2,?3,?4,?5)",
            params![sid, i as i64, kind, attempt, retry_in],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(true)
}

#[derive(Default)]
struct UsageAcc {
    input: i64,
    output: i64,
    cache_create: i64,
    cache_read: i64,
}

/// Estimate cost from per-message tokens × the pricing table.
fn estimate_cost(conn: &Connection, model: &str, acc: &UsageAcc) -> f64 {
    let row = conn.query_row(
        "SELECT input_per_mtok, output_per_mtok, cache_write_per_mtok, cache_read_per_mtok
         FROM pricing WHERE model = ?1",
        params![model],
        |r| {
            Ok((
                r.get::<_, f64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, f64>(3)?,
            ))
        },
    );
    match row {
        Ok((ipm, opm, cwm, crm)) => {
            (acc.input as f64 / 1_000_000.0) * ipm
                + (acc.output as f64 / 1_000_000.0) * opm
                + (acc.cache_create as f64 / 1_000_000.0) * cwm
                + (acc.cache_read as f64 / 1_000_000.0) * crm
        }
        Err(_) => 0.0, // unknown model → unknown price; surface as estimate:0 in UI
    }
}

fn rebuild_projects(conn: &Connection, config: &ClaudeConfig) {
    // Aggregate sessions → projects; layer config's last-cost/lines.
    conn.execute("DELETE FROM projects", []).ok();
    let mut stmt = conn
        .prepare(
            "SELECT project_dir, cwd, COUNT(*), MAX(last_ts)
             FROM sessions GROUP BY project_dir, cwd",
        )
        .unwrap();
    let rows: Vec<(String, String, i64, Option<String>)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    drop(stmt);
    for (encoded, cwd, count, last_mod) in rows {
        let cfg = config.projects.get(&cwd);
        let last_cost = cfg.and_then(|c| c.last_cost_usd);
        let lines_added = cfg.and_then(|c| c.last_lines_added);
        let lines_removed = cfg.and_then(|c| c.last_lines_removed);
        let display = cwd.split('/').next_back().unwrap_or(&cwd).to_string();
        conn.execute(
            "INSERT OR REPLACE INTO projects
             (encoded_dir, cwd, display_name, session_count, last_cost_usd,
              last_lines_added, last_lines_removed, last_modified, pinned)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8, 0)",
            params![
                encoded,
                cwd,
                display,
                count,
                last_cost,
                lines_added,
                lines_removed,
                last_mod,
            ],
        )
        .ok();
    }
}

fn extract_message_text(v: &Value) -> Option<String> {
    let content = v.get("message").and_then(|m| m.get("content"))?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut out = String::new();
        for block in arr {
            if let Some(t) = block.get("text").and_then(|s| s.as_str()) {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(t);
            }
        }
        if !out.is_empty() {
            return Some(out);
        }
    }
    None
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

/// Clean a candidate title so prompt-content garbage doesn't leak as the
/// display title. Mirrors the frontend sanitizeTitle logic so both stay
/// consistent. Returns None if nothing usable remains.
///
///   "<instructions><references>"  →  "instructions references"
///   "## Task: do X"               →  "Task: do X"
fn sanitize_title(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }
    // Strip XML/HTML tags, keep inner text.
    while let (Some(start), _) = (s.find('<'), s.find('>')) {
        if let Some(end) = s.find('>') {
            if end > start {
                s.replace_range(start..=end, " ");
            } else {
                break;
            }
        } else {
            break;
        }
    }
    // Strip markdown headers / emphasis.
    let stripped = s.trim_start_matches('#').trim_start();
    let stripped = stripped.replace(['*', '_', '`'], "");
    s = stripped;
    // Collapse whitespace.
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Drop leading @ / / file-ref noise.
    s = s.trim_start_matches(|c: char| c == '@' || c == '/' || c == '\\').to_string();
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Difference between two ISO timestamps in milliseconds (best-effort).
fn iso_diff_ms(a: &str, b: &str) -> Option<i64> {
    let pa = parse_iso(a)?;
    let pb = parse_iso(b)?;
    Some((pb - pa).num_milliseconds())
}

fn parse_iso(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn setup_mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // create tables
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id            TEXT PRIMARY KEY,
                project_dir   TEXT,
                cwd           TEXT,
                git_branch    TEXT,
                title         TEXT,
                first_ts      TEXT,
                last_ts       TEXT,
                message_count INTEGER DEFAULT 0,
                duration_ms   INTEGER DEFAULT 0,
                plan_mode     INTEGER DEFAULT 0,
                has_recap     INTEGER DEFAULT 0,
                file_size     INTEGER DEFAULT 0,
                file_path     TEXT UNIQUE NOT NULL,
                file_mtime    INTEGER NOT NULL,
                indexed_at    TEXT
            );

            CREATE TABLE IF NOT EXISTS session_usage (
                session_id          TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                model               TEXT NOT NULL,
                input_toks          INTEGER DEFAULT 0,
                output_toks         INTEGER DEFAULT 0,
                cache_create_toks   INTEGER DEFAULT 0,
                cache_read_toks     INTEGER DEFAULT 0,
                cost_usd            REAL    DEFAULT 0,
                cost_source         TEXT,
                api_duration_ms     INTEGER DEFAULT 0,
                PRIMARY KEY (session_id, model)
            );

            CREATE TABLE IF NOT EXISTS recaps (
                session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                uuid         TEXT PRIMARY KEY,
                captured_ts  TEXT,
                content      TEXT,
                seq          INTEGER,
                is_final     INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS files_touched (
                session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                file_path   TEXT NOT NULL,
                snapshots   INTEGER DEFAULT 0,
                PRIMARY KEY (session_id, file_path)
            );

            CREATE TABLE IF NOT EXISTS todos (
                session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                seq         INTEGER NOT NULL,
                content     TEXT,
                status      TEXT,
                PRIMARY KEY (session_id, seq)
            );

            CREATE TABLE IF NOT EXISTS turns (
                session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                turn_idx     INTEGER NOT NULL,
                duration_ms  INTEGER,
                message_count INTEGER,
                PRIMARY KEY (session_id, turn_idx)
            );

            CREATE TABLE IF NOT EXISTS errors (
                session_id     TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                seq            INTEGER NOT NULL,
                kind           TEXT,
                retry_attempt  INTEGER,
                retry_in_ms    INTEGER,
                PRIMARY KEY (session_id, seq)
            );

            CREATE TABLE IF NOT EXISTS projects (
                encoded_dir        TEXT PRIMARY KEY,
                cwd                TEXT,
                display_name       TEXT,
                session_count      INTEGER DEFAULT 0,
                last_cost_usd      REAL,
                last_lines_added   INTEGER,
                last_lines_removed INTEGER,
                last_modified      TEXT,
                pinned             INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS pricing (
                model                TEXT PRIMARY KEY,
                input_per_mtok       REAL,
                output_per_mtok      REAL,
                cache_write_per_mtok REAL,
                cache_read_per_mtok  REAL
            );
            "#,
        ).unwrap();
        // Insert a dummy price for tests
        conn.execute(
            "INSERT INTO pricing (model, input_per_mtok, output_per_mtok, cache_write_per_mtok, cache_read_per_mtok) VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params!["test-model", 10.0, 20.0, 0.0, 0.0],
        ).unwrap();
        conn
    }

    #[test]
    fn decode_dir() {
        assert_eq!(decode_project_dir("-home-test"), "/home/test");
        assert_eq!(decode_project_dir("a-b-c"), "a/b/c");
    }

    #[test]
    fn test_sanitize_title() {
        assert_eq!(sanitize_title("## Task: do X").unwrap(), "Task: do X");
        assert_eq!(sanitize_title("<instructions><references>Hello</references>").unwrap(), "Hello");
        assert_eq!(sanitize_title("@/src/foo.rs").unwrap(), "src/foo.rs");
        assert_eq!(sanitize_title("    _test_  ").unwrap(), "test");
        assert!(sanitize_title("<tag></tag>").is_none());
    }

    #[test]
    fn parse_and_upsert_empty_file() {
        let conn = setup_mem_db();
        let f = NamedTempFile::new().unwrap();
        let cfg = ClaudeConfig { projects: HashMap::new() };
        let res = parse_and_upsert(&conn, f.path(), 1234, &cfg).unwrap();
        assert_eq!(res, false, "empty file should return false");
    }

    #[test]
    fn parse_and_upsert_no_session_id() {
        let conn = setup_mem_db();
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"type":"user","message":{{"content":"hi"}}}}"#).unwrap();
        let cfg = ClaudeConfig { projects: HashMap::new() };
        let res = parse_and_upsert(&conn, f.path(), 1234, &cfg).unwrap();
        assert_eq!(res, false, "file with no sessionId anywhere should return false");
    }

    #[test]
    fn parse_and_upsert_basic_session() {
        let conn = setup_mem_db();
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"sessionId":"s1","cwd":"/path/basic","type":"user","message":{{"content":"first user message"}}}}"#).unwrap();
        writeln!(f, r#"{{"sessionId":"s1","type":"assistant","message":{{"model":"test-model","usage":{{"input_tokens":100,"output_tokens":50}}}}}}"#).unwrap();
        writeln!(f, r#"{{"sessionId":"s1","type":"system","subtype":"away_summary","uuid":"uuid1","content":"This is a recap. (disable recaps in /config)"}}"#).unwrap();
        writeln!(f, r#"{{"sessionId":"s1","type":"file-history-snapshot","snapshot":{{"trackedFileBackups":{{"/path/basic/foo.txt":{{}}}}}}}}"#).unwrap();
        
        let cfg = ClaudeConfig { projects: HashMap::new() };
        let res = parse_and_upsert(&conn, f.path(), 1234, &cfg).unwrap();
        assert!(res, "should have parsed correctly");

        // Verify session
        let (cwd, title, mc, has_recap, plan_mode): (String, String, i64, i64, i64) = conn.query_row(
            "SELECT cwd, title, message_count, has_recap, plan_mode FROM sessions WHERE id='s1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        ).unwrap();
        assert_eq!(cwd, "/path/basic");
        assert_eq!(title, "first user message");
        assert_eq!(mc, 2);
        assert_eq!(has_recap, 1);
        assert_eq!(plan_mode, 0);

        // Verify recap
        let (content, is_final): (String, i64) = conn.query_row(
            "SELECT content, is_final FROM recaps WHERE uuid='uuid1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert_eq!(content, "This is a recap.");
        assert_eq!(is_final, 1);

        // Verify usage
        let (model, itoks, otoks, usd): (String, i64, i64, f64) = conn.query_row(
            "SELECT model, input_toks, output_toks, cost_usd FROM session_usage WHERE session_id='s1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        ).unwrap();
        assert_eq!(model, "test-model");
        assert_eq!(itoks, 100);
        assert_eq!(otoks, 50);
        assert_eq!(usd, (100.0/1_000_000.0)*10.0 + (50.0/1_000_000.0)*20.0);

        // Verify files touched
        let nfiles: i64 = conn.query_row("SELECT COUNT(*) FROM files_touched WHERE session_id='s1' AND file_path='/path/basic/foo.txt'", [], |r| r.get(0)).unwrap();
        assert_eq!(nfiles, 1);
    }

    #[test]
    fn parse_and_upsert_multibyte_title() {
        let conn = setup_mem_db();
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"sessionId":"s-multi","cwd":"/p","type":"user","message":{{"content":"em-dash—here—and a curly “quote” emoji 🔥"}}}}"#).unwrap();
        let cfg = ClaudeConfig { projects: HashMap::new() };
        let res = parse_and_upsert(&conn, f.path(), 1234, &cfg).unwrap();
        assert!(res);
        let title: String = conn.query_row("SELECT title FROM sessions WHERE id='s-multi'", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "em-dash—here—and a curly “quote” emoji 🔥");
    }

    #[test]
    fn parse_and_upsert_torn_line() {
        let conn = setup_mem_db();
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"sessionId":"s2","type":"user","message":{{"content":"ok"}}}}"#).unwrap();
        writeln!(f, r#"{{"sessionId":"s2","type":"user","message"#).unwrap(); // TORN LINE
        let cfg = ClaudeConfig { projects: HashMap::new() };
        let res = parse_and_upsert(&conn, f.path(), 1234, &cfg).unwrap();
        assert!(res);
        let mc: i64 = conn.query_row("SELECT message_count FROM sessions WHERE id='s2'", [], |r| r.get(0)).unwrap();
        assert_eq!(mc, 1);
    }
}
