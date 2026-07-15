//! Home screen: clusters indexed sessions into work threads, scores them, and
//! returns the top N with a templated why-sentence. Backs the `list_threads`
//! command (thin wrapper in lib.rs).
//!
//! The whole ranking pipeline is a single pure function (`build_home`) over a
//! Vec of loaded session rows, so clustering / scoring / dominance / why-copy are
//! all unit-testable without a DB. `load_session_rows` + `load_open_todos` are the
//! only DB-touching helpers; the command wires them together.
//!
//! Design (see design/first-screen-spec.md):
//!   1. thread_key = project_short_name (if set) else the cwd's display project.
//!      Tagged + untagged rows merge when the short name equals the display
//!      project case-insensitively; otherwise the tag wins for tagged rows.
//!   2. Branch split: a non-modal branch with >=2 sessions in the last 14 days
//!      splits into a `{key} · {branch}` sub-thread.
//!   Scoring: score = S * (100R + 40D + 30U + 15P + 25L). L is open-loop
//!   pressure from digest open_loops (+ open desired_vs_real). Dominance guard
//!   caps a project at 2 slots. If every thread is a tiny one-off (all S=0) the
//!   guard relaxes and everything ranks by recency alone.

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

// ─── IPC types (camelCase over the wire, matching lib.rs conventions) ────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HomeData {
    pub threads: Vec<HomeThread>,
    pub total_sessions: i64,
    pub active_threads_this_week: i64,
    pub stale: bool,
    pub last_activity_ts: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HomeThread {
    pub key: String,
    pub display_name: String,
    pub area_of_life: Option<String>,
    pub git_branch: Option<String>,
    pub session_count: i64,
    pub active_days_14: i64,
    pub last_ts: Option<String>,
    pub latest_session_id: String,
    pub latest_title: String,
    pub latest_recap: Option<String>,
    pub open_todos: Vec<String>,
    /// Stated unfinished intent (digest open_loops + open desired_vs_real), max 3.
    pub open_loops: Vec<String>,
    /// Digest worked_on for the latest session, when present.
    pub worked_on: Option<String>,
    /// Digest outcome for the latest session, when present.
    pub outcome: Option<String>,
    pub completion_pct: Option<i64>,
    pub why_sentence: String,
    pub score: f64,
}

// ─── Loaded session row (internal) ───────────────────────────────────────────

/// One indexed session, projected for the ranker. Opaque to lib.rs (which only
/// pipes it from `load_session_rows` into `build_home`), so fields stay private.
pub struct SessionRow {
    id: String,
    cwd: String,
    project_dir: String,
    display_project: String,
    git_branch: Option<String>,
    title: String,
    last_ts: Option<String>,
    last_dt: Option<DateTime<Utc>>,
    message_count: i64,
    plan_mode: bool,
    recap: Option<String>,
    pinned: bool,
    project_status: Option<String>, // derived (override-wins), for archived-filtering
    area_of_life: Option<String>,
    short_name: Option<String>, // normalized: None when null/blank
    goal_completed: Option<bool>,
    completion_pct: Option<i64>,
    kanban_status: Option<String>,
    open_todo_count: i64,
    /// Digest open_loops for this session (empty if no digest row).
    digest_open_loops: Vec<String>,
    digest_worked_on: Option<String>,
    digest_outcome: Option<String>,
    /// Open desired_vs_real "desired" strings from the newest report for this
    /// session's ontology project key (project-level, same on every session of
    /// the project — only the latest session's copy feeds L).
    report_open_desired: Vec<String>,
}

/// Last path component of a cwd (the human project name). Mirrors
/// `project_display` in lib.rs.
fn display_project(cwd: &str) -> String {
    cwd.split('/').next_back().unwrap_or(cwd).to_string()
}

fn parse_dt(ts: Option<&str>) -> Option<DateTime<Utc>> {
    ts.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

/// Normalize a short name: trim, and treat empty as absent.
fn norm_short_name(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Parse a JSON array-of-strings column; null/invalid → empty.
fn parse_str_array(raw: Option<String>) -> Vec<String> {
    raw.as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

/// Distinct open-loop strings for ranking/UI: digest loops + open dvr desired,
/// case-insensitive dedupe, order preserved (digest first).
fn merge_open_loops(digest: &[String], report_desired: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for s in digest.iter().chain(report_desired.iter()) {
        let t = s.trim();
        if t.is_empty() {
            continue;
        }
        let key = t.to_lowercase();
        if seen.insert(key) {
            out.push(t.to_string());
        }
    }
    out
}

/// Open desired strings from the newest project_reports row per project_key.
fn load_open_desired_by_project(conn: &rusqlite::Connection) -> HashMap<String, Vec<String>> {
    #[derive(Deserialize)]
    struct DvrRow {
        desired: String,
        status: String,
    }
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    // Newest report per key: order by generated_at desc; first write wins per key.
    let Ok(mut stmt) = conn.prepare(
        "SELECT project_key, desired_vs_real FROM project_reports
         ORDER BY generated_at DESC NULLS LAST",
    ) else {
        return map;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
        ))
    }) else {
        return map;
    };
    for row in rows.flatten() {
        let (key, raw) = row;
        if map.contains_key(&key) {
            continue; // already have newer report for this key
        }
        let opens: Vec<String> = raw
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<DvrRow>>(s).ok())
            .map(|rows| {
                rows.into_iter()
                    .filter(|r| r.status == "open")
                    .map(|r| r.desired)
                    .filter(|d| !d.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if !opens.is_empty() {
            map.insert(key, opens);
        }
    }
    map
}

// ─── DB loading (the only impure helpers) ────────────────────────────────────

/// Same dual blacklist check the indexer / list_sessions / digest apply. Kept
/// local so home.rs stays self-contained (mirrors digest.rs).
fn dir_blacklisted(cwd: &str, project_dir: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        crate::indexer::is_blacklisted(cwd, p)
            || crate::indexer::is_blacklisted_encoded(project_dir, p)
    })
}

/// Load every non-blacklisted indexed session, projected for the ranker. One
/// query: the final recap, pinned flag, tag fields, open-todo count, and digest
/// fields all arrive as correlated subqueries (cheap against a ~700-row corpus).
pub fn load_session_rows(conn: &rusqlite::Connection) -> Result<Vec<SessionRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.cwd, s.project_dir, s.git_branch, s.title, s.last_ts,
                    s.message_count, s.plan_mode,
                    (SELECT content FROM recaps r WHERE r.session_id = s.id AND r.is_final = 1),
                    COALESCE((SELECT pinned FROM projects p WHERE p.encoded_dir = s.project_dir), 0),
                    (SELECT status FROM projects p WHERE p.encoded_dir = s.project_dir),
                    COALESCE((SELECT status_manual FROM projects p WHERE p.encoded_dir = s.project_dir), 0),
                    (SELECT last_modified FROM projects p WHERE p.encoded_dir = s.project_dir),
                    s.area_of_life, s.project_short_name, s.goal_completed, s.completion_pct,
                    s.kanban_status,
                    (SELECT COUNT(*) FROM todos t WHERE t.session_id = s.id AND t.status != 'completed'),
                    (SELECT open_loops FROM session_digests d WHERE d.session_id = s.id),
                    (SELECT worked_on FROM session_digests d WHERE d.session_id = s.id),
                    (SELECT outcome FROM session_digests d WHERE d.session_id = s.id)
             FROM sessions s",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<SessionRow> = stmt
        .query_map([], |r| {
            let cwd: String = r.get(1)?;
            let last_ts: Option<String> = r.get(5)?;
            Ok(SessionRow {
                id: r.get(0)?,
                display_project: display_project(&cwd),
                cwd,
                project_dir: r.get(2)?,
                git_branch: r.get(3)?,
                title: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                last_dt: parse_dt(last_ts.as_deref()),
                last_ts,
                message_count: r.get(6)?,
                plan_mode: r.get::<_, i64>(7)? != 0,
                recap: r.get(8)?,
                pinned: r.get::<_, i64>(9)? != 0,
                project_status: crate::derive_project_status(
                    r.get::<_, Option<String>>(10)?.as_deref(),
                    r.get::<_, i64>(11)? != 0,
                    r.get::<_, Option<String>>(12)?.as_deref(),
                )
                .map(|s| s.to_string()),
                area_of_life: r.get(13)?,
                short_name: norm_short_name(r.get(14)?),
                goal_completed: r.get::<_, Option<i64>>(15)?.map(|v| v != 0),
                completion_pct: r.get(16)?,
                kanban_status: r.get(17)?,
                open_todo_count: r.get(18)?,
                digest_open_loops: parse_str_array(r.get(19)?),
                digest_worked_on: r
                    .get::<_, Option<String>>(20)?
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                digest_outcome: r
                    .get::<_, Option<String>>(21)?
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                report_open_desired: Vec::new(), // filled below
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();

    // Defensive blacklist filter (same as list_sessions): a pattern added after
    // indexing drops its rows this query cycle, no re-index needed. Archived
    // projects are hidden from Home threads + counts by the same post-filter.
    let patterns = crate::db::load_blacklist_patterns(conn);
    let archived = crate::db::load_archived_project_dirs(conn);
    let open_by_key = load_open_desired_by_project(conn);
    let out = rows
        .into_iter()
        .filter(|r| !dir_blacklisted(&r.cwd, &r.project_dir, &patterns))
        .filter(|r| r.project_status.as_deref() != Some("archived"))
        .filter(|r| !archived.contains(&r.project_dir))
        .map(|mut r| {
            let key = crate::ontology::derive_identity_with(&r.cwd, None).key;
            r.report_open_desired = open_by_key.get(&key).cloned().unwrap_or_default();
            r
        })
        .collect();
    Ok(out)
}

/// Up to 3 open (status != completed) todo contents for one session, in list
/// order. Filled for the winning threads only, after ranking.
pub fn load_open_todos(conn: &rusqlite::Connection, session_id: &str) -> Vec<String> {
    conn.prepare(
        "SELECT content FROM todos WHERE session_id = ?1 AND status != 'completed'
         ORDER BY seq LIMIT 3",
    )
    .and_then(|mut s| {
        s.query_map(rusqlite::params![session_id], |r| r.get::<_, String>(0))
            .map(|rows| rows.flatten().collect())
    })
    .unwrap_or_default()
}

// ─── Clustering ──────────────────────────────────────────────────────────────

/// A cluster of session indices that renders as one thread. `project_key` is the
/// base thread key shared by a project and its branch sub-threads (the unit the
/// dominance guard caps at 2).
#[derive(Clone)]
struct Cluster {
    project_key: String,
    key: String,
    branch: Option<String>,
    is_sub: bool,
    sessions: Vec<usize>,
}

/// thread_key = short_name (if set) else display_project. A tagged row merges
/// with untagged rows of the same project when its short name equals the display
/// project case-insensitively; otherwise the tag wins.
fn resolve_thread_key(short_name: Option<&str>, display_project: &str) -> String {
    match short_name {
        Some(s) if !s.trim().is_empty() => {
            if s.eq_ignore_ascii_case(display_project) {
                display_project.to_string()
            } else {
                s.to_string()
            }
        }
        _ => display_project.to_string(),
    }
}

fn cmp_dt(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

/// Most frequent branch across a session set; ties broken by recency. None when
/// no session carries a branch.
fn modal_branch(sessions: &[usize], rows: &[SessionRow]) -> Option<String> {
    // branch -> (count, max last_dt)
    let mut agg: HashMap<&str, (usize, Option<DateTime<Utc>>)> = HashMap::new();
    for &i in sessions {
        if let Some(b) = rows[i].git_branch.as_deref() {
            let e = agg.entry(b).or_insert((0, None));
            e.0 += 1;
            if cmp_dt(rows[i].last_dt, e.1) == Ordering::Greater {
                e.1 = rows[i].last_dt;
            }
        }
    }
    agg.into_iter()
        .max_by(|a, b| a.1 .0.cmp(&b.1 .0).then_with(|| cmp_dt(a.1 .1, b.1 .1)))
        .map(|(b, _)| b.to_string())
}

/// Group sessions into threads, then split off qualifying non-modal branches.
fn cluster_sessions(rows: &[SessionRow], now: DateTime<Utc>) -> Vec<Cluster> {
    // 1. Group by resolved thread key. Preserve first-seen order for determinism.
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let key = resolve_thread_key(row.short_name.as_deref(), &row.display_project);
        groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        groups.get_mut(&key).unwrap().push(i);
    }

    let cutoff = now - Duration::days(14);
    let mut out: Vec<Cluster> = Vec::new();

    for base_key in order {
        let idxs = groups.remove(&base_key).unwrap();
        let modal = modal_branch(&idxs, rows);

        // 2. Which non-modal branches have >=2 sessions in the last 14 days.
        let mut recent_counts: HashMap<&str, usize> = HashMap::new();
        for &i in &idxs {
            if let Some(b) = rows[i].git_branch.as_deref() {
                if Some(b) != modal.as_deref()
                    && rows[i].last_dt.map(|d| d >= cutoff).unwrap_or(false)
                {
                    *recent_counts.entry(b).or_insert(0) += 1;
                }
            }
        }
        let qualifying: HashSet<String> = recent_counts
            .into_iter()
            .filter(|(_, n)| *n >= 2)
            .map(|(b, _)| b.to_string())
            .collect();

        // Partition: qualifying-branch sessions form sub-threads; the rest
        // (modal, one-off branches, no-branch) stay in the base.
        let mut base_sessions: Vec<usize> = Vec::new();
        let mut sub_order: Vec<String> = Vec::new();
        let mut subs: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in &idxs {
            match rows[i].git_branch.as_deref() {
                Some(b) if qualifying.contains(b) => {
                    subs.entry(b.to_string()).or_insert_with(|| {
                        sub_order.push(b.to_string());
                        Vec::new()
                    });
                    subs.get_mut(b).unwrap().push(i);
                }
                _ => base_sessions.push(i),
            }
        }

        // Base always retains at least the modal-branch sessions.
        out.push(Cluster {
            project_key: base_key.clone(),
            key: base_key.clone(),
            branch: modal.clone(),
            is_sub: false,
            sessions: base_sessions,
        });
        for b in sub_order {
            let sessions = subs.remove(&b).unwrap();
            out.push(Cluster {
                project_key: base_key.clone(),
                key: format!("{base_key} · {b}"),
                branch: Some(b),
                is_sub: true,
                sessions,
            });
        }
    }
    out
}

// ─── Scoring ─────────────────────────────────────────────────────────────────

struct Metrics {
    r: f64,
    d: f64,
    u: i64,
    p: i64,
    /// Open-loop pressure 0..1 (min(count,4)/4).
    l: f64,
    /// Distinct open-loop count used for L (for why-copy).
    open_loop_count: i64,
    s: i64,
    active_days: i64,
    score: f64,
    max_dt: Option<DateTime<Utc>>,
    latest_idx: usize,
}

/// Pick the latest session (max last_ts; smaller id breaks ties for stability).
fn latest_session(sessions: &[usize], rows: &[SessionRow]) -> usize {
    *sessions
        .iter()
        .max_by(|&&a, &&b| {
            cmp_dt(rows[a].last_dt, rows[b].last_dt)
                .then_with(|| rows[b].id.cmp(&rows[a].id))
        })
        .expect("cluster is never empty")
}

/// Does the latest session read as "left mid-task"? (the U signal)
fn latest_unfinished(latest: &SessionRow) -> bool {
    latest.open_todo_count > 0
        || latest.completion_pct.map(|p| p < 100).unwrap_or(false)
        || latest.goal_completed == Some(false)
        || latest.kanban_status.as_deref() == Some("in_progress")
        || latest.plan_mode
}

/// Full scoring for a session set. `ref_dt` anchors the 14-day density window
/// (now normally, the global max last_ts in stale mode); `d`/R always decay from
/// `now`.
fn compute_metrics(
    sessions: &[usize],
    rows: &[SessionRow],
    now: DateTime<Utc>,
    ref_dt: DateTime<Utc>,
) -> Metrics {
    let latest_idx = latest_session(sessions, rows);
    let max_dt = rows[latest_idx].last_dt;

    // Recency: 3-day half-life, decaying from now.
    let d_days = max_dt
        .map(|dt| ((now - dt).num_seconds().max(0) as f64) / 86_400.0)
        .unwrap_or(f64::INFINITY);
    let r = if d_days.is_finite() {
        2f64.powf(-d_days / 3.0)
    } else {
        0.0
    };

    // Density: distinct active calendar days in the 14-day window ending at ref.
    let ref_date: NaiveDate = ref_dt.with_timezone(&Local).date_naive();
    let oldest: NaiveDate = ref_date - Duration::days(13);
    let mut days: HashSet<NaiveDate> = HashSet::new();
    for &i in sessions {
        if let Some(dt) = rows[i].last_dt {
            let day = dt.with_timezone(&Local).date_naive();
            if day >= oldest && day <= ref_date {
                days.insert(day);
            }
        }
    }
    let active_days = days.len() as i64;
    let d = active_days as f64 / 14.0;

    let u = if latest_unfinished(&rows[latest_idx]) { 1 } else { 0 };
    let p = if sessions.iter().any(|&i| rows[i].pinned) { 1 } else { 0 };

    let latest = &rows[latest_idx];
    let loops = merge_open_loops(&latest.digest_open_loops, &latest.report_open_desired);
    let open_loop_count = loops.len() as i64;
    let l = (open_loop_count.min(4) as f64) / 4.0;

    // Substance guard: kill drive-by one-offs (single short session, nothing left
    // open). Everything else clears the bar. Open loops count as substance.
    let s = if sessions.len() == 1
        && rows[latest_idx].message_count < 10
        && u == 0
        && open_loop_count == 0
    {
        0
    } else {
        1
    };

    let score =
        s as f64 * (100.0 * r + 40.0 * d + 30.0 * u as f64 + 15.0 * p as f64 + 25.0 * l);

    Metrics {
        r,
        d,
        u,
        p,
        l,
        open_loop_count,
        s,
        active_days,
        score,
        max_dt,
        latest_idx,
    }
}

// ─── Why-sentence ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Comp {
    R,
    D,
    U,
    P,
    L,
}

/// Humanize a relative time, e.g. "2 hours ago", "yesterday", "just now".
fn humanize_relative(dt: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(dt) = dt else {
        return "some time ago".to_string();
    };
    let secs = (now - dt).num_seconds().max(0);
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    if secs < 60 {
        "just now".to_string()
    } else if mins < 60 {
        plural(mins, "minute")
    } else if hours < 24 {
        plural(hours, "hour")
    } else if days == 1 {
        "yesterday".to_string()
    } else if days < 7 {
        plural(days, "day")
    } else if days < 30 {
        plural(days / 7, "week")
    } else if days < 365 {
        plural(days / 30, "month")
    } else {
        plural(days / 365, "year")
    }
}

/// The U fragment, by trigger priority (todos > pct > plan > in-progress),
/// falling back to a goal-unfinished line for the bare goal_completed=false case.
fn unfinished_fragment(latest: &SessionRow) -> String {
    if latest.open_todo_count > 0 {
        let n = latest.open_todo_count;
        format!(
            "Last session ended with {n} open todo{}.",
            if n == 1 { "" } else { "s" }
        )
    } else if let Some(pct) = latest.completion_pct.filter(|p| *p < 100) {
        format!("Last session at {pct}% complete.")
    } else if latest.plan_mode {
        "Last session ended with an unexecuted plan.".to_string()
    } else if latest.kanban_status.as_deref() == Some("in_progress") {
        "Marked in progress.".to_string()
    } else {
        "Last session left its goal unfinished.".to_string()
    }
}

fn fragment_for(comp: Comp, m: &Metrics, latest: &SessionRow, now: DateTime<Utc>) -> String {
    match comp {
        Comp::R => format!("Last active {}.", humanize_relative(latest.last_dt, now)),
        Comp::D => format!("Active {} of the last 14 days.", m.active_days),
        Comp::U => unfinished_fragment(latest),
        Comp::P => "Pinned.".to_string(),
        Comp::L => {
            let n = m.open_loop_count;
            format!(
                "{n} open loop{}.",
                if n == 1 { "" } else { "s" }
            )
        }
    }
}

/// Lowercase only the first character (for the second fragment after "; ").
fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Templated why-sentence from the two largest weighted score components, joined
/// with "; " (the second fragment starts lowercase). Relaxed mode says only the
/// recency line. A component contributes only when its weighted value is > 0.
fn why_sentence(m: &Metrics, latest: &SessionRow, now: DateTime<Utc>, relaxed: bool) -> String {
    if relaxed {
        return fragment_for(Comp::R, m, latest, now);
    }
    // (weighted value, component) in fixed priority order for stable tie-breaks.
    let weighted = [
        (100.0 * m.r, Comp::R),
        (40.0 * m.d, Comp::D),
        (30.0 * m.u as f64, Comp::U),
        (25.0 * m.l, Comp::L),
        (15.0 * m.p as f64, Comp::P),
    ];
    let mut chosen: Vec<(f64, Comp)> = weighted.iter().copied().filter(|(v, _)| *v > 0.0).collect();
    chosen.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
    chosen.truncate(2);

    // A substantive thread with no positive component (e.g. old, nothing open)
    // still states its recency honestly.
    if chosen.is_empty() {
        return fragment_for(Comp::R, m, latest, now);
    }

    let frags: Vec<String> = chosen
        .iter()
        .map(|(_, c)| fragment_for(*c, m, latest, now))
        .collect();

    if frags.len() == 1 {
        return frags.into_iter().next().unwrap();
    }
    let first = frags[0].strip_suffix('.').unwrap_or(&frags[0]);
    format!("{}; {}", first, lower_first(&frags[1]))
}

// ─── Assembly ────────────────────────────────────────────────────────────────

/// Human name for a cluster: the most-recently-tagged short name, else the
/// latest session's display project; a sub-thread appends " · {branch}".
fn display_name(cluster: &Cluster, rows: &[SessionRow]) -> String {
    // Latest session carrying a short name (most recent by last_ts).
    let short = cluster
        .sessions
        .iter()
        .filter(|&&i| rows[i].short_name.is_some())
        .max_by(|&&a, &&b| cmp_dt(rows[a].last_dt, rows[b].last_dt))
        .and_then(|&i| rows[i].short_name.clone());
    let latest_idx = latest_session(&cluster.sessions, rows);
    let base = short.unwrap_or_else(|| rows[latest_idx].display_project.clone());
    if cluster.is_sub {
        if let Some(b) = &cluster.branch {
            return format!("{base} · {b}");
        }
    }
    base
}

/// Most recent session with a non-null area_of_life, if any.
fn thread_area(cluster: &Cluster, rows: &[SessionRow]) -> Option<String> {
    cluster
        .sessions
        .iter()
        .filter(|&&i| rows[i].area_of_life.is_some())
        .max_by(|&&a, &&b| cmp_dt(rows[a].last_dt, rows[b].last_dt))
        .and_then(|&i| rows[i].area_of_life.clone())
}

fn finalize_thread(
    cluster: &Cluster,
    rows: &[SessionRow],
    now: DateTime<Utc>,
    ref_dt: DateTime<Utc>,
    relaxed: bool,
) -> HomeThread {
    let m = compute_metrics(&cluster.sessions, rows, now, ref_dt);
    let latest = &rows[m.latest_idx];
    let score = if relaxed { 100.0 * m.r } else { m.score };
    let mut open_loops =
        merge_open_loops(&latest.digest_open_loops, &latest.report_open_desired);
    open_loops.truncate(3);
    HomeThread {
        key: cluster.key.clone(),
        display_name: display_name(cluster, rows),
        area_of_life: thread_area(cluster, rows),
        git_branch: cluster.branch.clone(),
        session_count: cluster.sessions.len() as i64,
        active_days_14: m.active_days,
        last_ts: latest.last_ts.clone(),
        latest_session_id: latest.id.clone(),
        latest_title: latest.title.clone(),
        latest_recap: latest.recap.clone(),
        open_todos: Vec::new(), // filled by the command for winners
        open_loops,
        worked_on: latest.digest_worked_on.clone(),
        outcome: latest.digest_outcome.clone(),
        completion_pct: latest.completion_pct,
        why_sentence: why_sentence(&m, latest, now, relaxed),
        score,
    }
}

/// The core ranking pipeline: cluster → score → (substance filter | relax) →
/// order → dominance guard + fold → finalize. Pure and fully unit-testable.
pub fn build_home(rows: Vec<SessionRow>, now: DateTime<Utc>, limit: usize) -> HomeData {
    let total_sessions = rows.len() as i64;

    // Global activity anchors.
    let latest_row = rows
        .iter()
        .filter(|r| r.last_dt.is_some())
        .max_by(|a, b| cmp_dt(a.last_dt, b.last_dt));
    let last_activity_ts = latest_row.and_then(|r| r.last_ts.clone());
    let global_max = latest_row.and_then(|r| r.last_dt);
    let stale = global_max
        .map(|d| (now - d) > Duration::days(14))
        .unwrap_or(false);
    let ref_dt = if stale { global_max.unwrap() } else { now };

    let clusters = cluster_sessions(&rows, now);

    // Threads active this week (post-clustering, over the full cluster set).
    let week_ago = now - Duration::days(7);
    let metrics: Vec<Metrics> = clusters
        .iter()
        .map(|c| compute_metrics(&c.sessions, &rows, now, ref_dt))
        .collect();
    let active_threads_this_week = metrics
        .iter()
        .filter(|m| m.max_dt.map(|d| d >= week_ago).unwrap_or(false))
        .count() as i64;

    // Substance guard: keep only S=1 clusters. If EVERY cluster is a tiny one-off
    // (all S=0), relax — rank whatever exists by recency alone.
    let any_substance = metrics.iter().any(|m| m.s == 1);
    let relaxed = !any_substance && !clusters.is_empty();

    let mut order: Vec<usize> = (0..clusters.len())
        .filter(|&i| relaxed || metrics[i].s == 1)
        .collect();
    order.sort_by(|&a, &b| {
        let (ka, kb) = if relaxed {
            (metrics[a].r, metrics[b].r)
        } else {
            (metrics[a].score, metrics[b].score)
        };
        kb.partial_cmp(&ka)
            .unwrap_or(Ordering::Equal)
            .then_with(|| cmp_dt(metrics[b].max_dt, metrics[a].max_dt))
            .then_with(|| clusters[a].key.cmp(&clusters[b].key))
    });

    // Dominance guard: at most 2 threads per project prefix. Overflow sub-threads
    // fold their sessions into the project's top selected thread; distinct
    // projects take the freed slots.
    let mut selected: Vec<Cluster> = Vec::new();
    let mut proj_count: HashMap<String, usize> = HashMap::new();
    let mut proj_top: HashMap<String, usize> = HashMap::new();
    for i in order {
        let pk = clusters[i].project_key.clone();
        let fold_into = |selected: &mut Vec<Cluster>, top: Option<&usize>| {
            if let Some(&ti) = top {
                let mut extra = clusters[i].sessions.clone();
                selected[ti].sessions.append(&mut extra);
            }
        };
        if proj_count.get(&pk).copied().unwrap_or(0) >= 2 {
            fold_into(&mut selected, proj_top.get(&pk));
        } else if selected.len() < limit {
            selected.push(clusters[i].clone());
            let idx = selected.len() - 1;
            *proj_count.entry(pk.clone()).or_insert(0) += 1;
            proj_top.entry(pk).or_insert(idx);
        } else {
            // No slot left: fold into this project's top thread if it is shown,
            // else drop (a distinct project with no room).
            fold_into(&mut selected, proj_top.get(&pk));
        }
    }

    let threads = selected
        .iter()
        .map(|c| finalize_thread(c, &rows, now, ref_dt, relaxed))
        .collect();

    HomeData {
        threads,
        total_sessions,
        active_threads_this_week,
        stale,
        last_activity_ts,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal builder for a session row at `days_ago` (fractional) days back.
    #[allow(clippy::too_many_arguments)]
    fn row(
        id: &str,
        cwd: &str,
        short_name: Option<&str>,
        branch: Option<&str>,
        days_ago: f64,
        now: DateTime<Utc>,
        message_count: i64,
    ) -> SessionRow {
        let dt = now - Duration::milliseconds((days_ago * 86_400_000.0) as i64);
        SessionRow {
            id: id.to_string(),
            display_project: display_project(cwd),
            cwd: cwd.to_string(),
            project_dir: format!("-{}", cwd.replace('/', "-")),
            git_branch: branch.map(|b| b.to_string()),
            title: format!("title {id}"),
            last_ts: Some(dt.to_rfc3339()),
            last_dt: Some(dt),
            message_count,
            plan_mode: false,
            recap: Some(format!("recap {id}")),
            pinned: false,
            project_status: None,
            area_of_life: None,
            short_name: short_name.map(|s| s.to_string()),
            goal_completed: None,
            completion_pct: None,
            kanban_status: None,
            open_todo_count: 0,
            digest_open_loops: Vec::new(),
            digest_worked_on: None,
            digest_outcome: None,
            report_open_desired: Vec::new(),
        }
    }

    fn row_with_loops(
        id: &str,
        cwd: &str,
        days_ago: f64,
        now: DateTime<Utc>,
        loops: &[&str],
    ) -> SessionRow {
        let mut r = row(id, cwd, None, None, days_ago, now, 20);
        r.digest_open_loops = loops.iter().map(|s| s.to_string()).collect();
        r
    }

    fn now_fixed() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    // ─── Clustering ──────────────────────────────────────────────────────────

    #[test]
    fn thread_key_merges_on_case_insensitive_match_else_tag_wins() {
        assert_eq!(resolve_thread_key(None, "brain"), "brain");
        assert_eq!(resolve_thread_key(Some("brain"), "brain"), "brain");
        assert_eq!(resolve_thread_key(Some("BRAIN"), "brain"), "brain"); // merge
        assert_eq!(
            resolve_thread_key(Some("Sleep Consolidation"), "brain"),
            "Sleep Consolidation" // tag wins
        );
        // Blank tag degrades to display project.
        assert_eq!(resolve_thread_key(Some("  "), "brain"), "brain");
    }

    #[test]
    fn clustering_merges_tagged_and_untagged_but_splits_a_distinct_tag() {
        let now = now_fixed();
        let rows = vec![
            row("a", "/x/brain", None, None, 0.0, now, 20), // untagged
            row("b", "/x/brain", Some("brain"), None, 1.0, now, 20), // tag == display → merge
            row("c", "/x/brain", Some("Sleep Consolidation"), None, 2.0, now, 20), // tag wins
        ];
        let clusters = cluster_sessions(&rows, now);
        let mut by_key: HashMap<String, usize> = HashMap::new();
        for c in &clusters {
            by_key.insert(c.key.clone(), c.sessions.len());
        }
        assert_eq!(by_key.get("brain"), Some(&2), "a+b merge into brain");
        assert_eq!(
            by_key.get("Sleep Consolidation"),
            Some(&1),
            "c keys by its distinct tag"
        );
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn branch_split_only_for_recent_multi_session_branches() {
        let now = now_fixed();
        let rows = vec![
            // modal branch "main": 2 sessions
            row("m1", "/x/app", None, Some("main"), 0.0, now, 20),
            row("m2", "/x/app", None, Some("main"), 1.0, now, 20),
            // "feat-x": 2 recent sessions → splits
            row("x1", "/x/app", None, Some("feat-x"), 0.5, now, 20),
            row("x2", "/x/app", None, Some("feat-x"), 1.5, now, 20),
            // "feat-y": 1 session → stays in base
            row("y1", "/x/app", None, Some("feat-y"), 0.2, now, 20),
        ];
        let clusters = cluster_sessions(&rows, now);
        let base = clusters.iter().find(|c| c.key == "app").unwrap();
        let sub = clusters.iter().find(|c| c.key == "app · feat-x").unwrap();
        assert_eq!(sub.sessions.len(), 2, "feat-x splits with its 2 recent sessions");
        assert_eq!(base.sessions.len(), 3, "main (2) + one-off feat-y (1) stay in base");
        assert!(base.branch.as_deref() == Some("main"));
        assert!(!clusters.iter().any(|c| c.key == "app · feat-y"));
    }

    #[test]
    fn branch_split_ignores_stale_branches() {
        let now = now_fixed();
        let rows = vec![
            row("m1", "/x/app", None, Some("main"), 0.0, now, 20),
            row("m2", "/x/app", None, Some("main"), 1.0, now, 20),
            // 2 sessions on old-branch but both older than 14 days → no split.
            row("o1", "/x/app", None, Some("old-branch"), 20.0, now, 20),
            row("o2", "/x/app", None, Some("old-branch"), 21.0, now, 20),
        ];
        let clusters = cluster_sessions(&rows, now);
        assert!(!clusters.iter().any(|c| c.is_sub), "stale branch must not split");
    }

    // ─── Scoring ─────────────────────────────────────────────────────────────

    #[test]
    fn recency_has_three_day_half_life() {
        let now = now_fixed();
        let ref_dt = now;
        let rows = vec![row("a", "/x/app", None, None, 3.0, now, 20)];
        let m = compute_metrics(&[0], &rows, now, ref_dt);
        assert!((m.r - 0.5).abs() < 1e-9, "d=3 → R=0.5, got {}", m.r);
        // 9 days back → ~1/8.
        let rows2 = vec![row("a", "/x/app", None, None, 9.0, now, 20)];
        let m2 = compute_metrics(&[0], &rows2, now, ref_dt);
        assert!((m2.r - 0.125).abs() < 1e-9, "d=9 → R=0.125, got {}", m2.r);
    }

    #[test]
    fn unfinished_triggers_each_signal() {
        let now = now_fixed();
        let mut base = row("a", "/x/app", None, None, 0.0, now, 20);
        assert!(!latest_unfinished(&base));

        base.open_todo_count = 2;
        assert!(latest_unfinished(&base));
        base.open_todo_count = 0;

        base.completion_pct = Some(60);
        assert!(latest_unfinished(&base));
        base.completion_pct = Some(100);
        assert!(!latest_unfinished(&base), "100% is finished");
        base.completion_pct = None;

        base.goal_completed = Some(false);
        assert!(latest_unfinished(&base));
        base.goal_completed = None;

        base.kanban_status = Some("in_progress".into());
        assert!(latest_unfinished(&base));
        base.kanban_status = None;

        base.plan_mode = true;
        assert!(latest_unfinished(&base), "unexecuted plan counts");
    }

    #[test]
    fn substance_guard_kills_tiny_one_off_but_spares_unfinished() {
        let now = now_fixed();
        let ref_dt = now;
        // Single short session, nothing open → S=0.
        let tiny = vec![row("a", "/x/app", None, None, 0.0, now, 5)];
        assert_eq!(compute_metrics(&[0], &tiny, now, ref_dt).s, 0);
        // Same, but with an open todo → S=1 (unfinished spares it).
        let mut r = row("a", "/x/app", None, None, 0.0, now, 5);
        r.open_todo_count = 1;
        assert_eq!(compute_metrics(&[0], &[r], now, ref_dt).s, 1);
        // A chunky single session (>=10 msgs) also clears the guard.
        let chunky = vec![row("a", "/x/app", None, None, 0.0, now, 40)];
        assert_eq!(compute_metrics(&[0], &chunky, now, ref_dt).s, 1);
    }

    #[test]
    fn substantive_thread_excludes_tiny_one_offs_from_results() {
        let now = now_fixed();
        let rows = vec![
            row("big", "/x/app", None, None, 0.0, now, 40), // substantive
            row("tiny", "/x/scratch", None, None, 0.0, now, 3), // one-off, S=0
        ];
        let data = build_home(rows, now, 5);
        assert_eq!(data.threads.len(), 1, "the tiny one-off is filtered out");
        assert_eq!(data.threads[0].key, "app");
    }

    #[test]
    fn all_tiny_relaxes_guard_and_ranks_by_recency() {
        let now = now_fixed();
        let rows = vec![
            row("a", "/x/one", None, None, 5.0, now, 3),
            row("b", "/x/two", None, None, 0.5, now, 4), // most recent
            row("c", "/x/three", None, None, 10.0, now, 2),
        ];
        let data = build_home(rows, now, 5);
        assert_eq!(data.threads.len(), 3, "relaxed guard surfaces all one-offs");
        assert_eq!(data.threads[0].key, "two", "ranked by recency (R) alone");
        assert!(
            data.threads[0].why_sentence.starts_with("Last active"),
            "relaxed why is the recency line, got: {}",
            data.threads[0].why_sentence
        );
    }

    // ─── Dominance guard ─────────────────────────────────────────────────────

    #[test]
    fn dominance_guard_caps_project_at_two_and_folds_the_rest() {
        let now = now_fixed();
        let rows = vec![
            // base "app" on modal "main": 3 sessions
            row("m1", "/x/app", None, Some("main"), 0.0, now, 20),
            row("m2", "/x/app", None, Some("main"), 1.0, now, 20),
            row("m3", "/x/app", None, Some("main"), 2.0, now, 20),
            // feat-a: 2 recent → sub
            row("a1", "/x/app", None, Some("feat-a"), 0.3, now, 20),
            row("a2", "/x/app", None, Some("feat-a"), 1.3, now, 20),
            // feat-b: 2 recent → sub
            row("b1", "/x/app", None, Some("feat-b"), 0.4, now, 20),
            row("b2", "/x/app", None, Some("feat-b"), 1.4, now, 20),
        ];
        let data = build_home(rows, now, 5);
        assert_eq!(data.threads.len(), 2, "project capped at 2 slots");
        let total: i64 = data.threads.iter().map(|t| t.session_count).sum();
        assert_eq!(total, 7, "the folded sub's sessions count toward a shown thread");
    }

    #[test]
    fn distinct_projects_take_the_freed_slots() {
        let now = now_fixed();
        let mut rows = vec![
            row("m1", "/x/app", None, Some("main"), 0.0, now, 20),
            row("m2", "/x/app", None, Some("main"), 1.0, now, 20),
            row("a1", "/x/app", None, Some("feat-a"), 0.3, now, 20),
            row("a2", "/x/app", None, Some("feat-a"), 1.3, now, 20),
            row("b1", "/x/app", None, Some("feat-b"), 0.4, now, 20),
            row("b2", "/x/app", None, Some("feat-b"), 1.4, now, 20),
        ];
        // A different project.
        rows.push(row("z1", "/x/brain", None, None, 0.1, now, 40));
        let data = build_home(rows, now, 5);
        // app capped at 2, brain gets a slot.
        assert!(data.threads.iter().any(|t| t.key == "brain"));
        let app_slots = data.threads.iter().filter(|t| t.key.starts_with("app")).count();
        assert_eq!(app_slots, 2, "app never exceeds 2 slots");
    }

    // ─── Why-sentence ────────────────────────────────────────────────────────

    fn metrics(r: f64, d: f64, u: i64, p: i64, active_days: i64) -> Metrics {
        Metrics {
            r,
            d,
            u,
            p,
            l: 0.0,
            open_loop_count: 0,
            s: 1,
            active_days,
            score: 0.0,
            max_dt: None,
            latest_idx: 0,
        }
    }

    #[test]
    fn why_recency_dominant_single_sentence() {
        let now = now_fixed();
        let latest = row("a", "/x/app", None, None, 0.0, now, 20); // now → "just now"
        let m = metrics(1.0, 0.0, 0, 0, 0);
        assert_eq!(why_sentence(&m, &latest, now, false), "Last active just now.");
    }

    #[test]
    fn why_density_dominant_single_sentence() {
        let now = now_fixed();
        let latest = row("a", "/x/app", None, None, 30.0, now, 20);
        let m = metrics(0.0, 5.0 / 14.0, 0, 0, 5);
        assert_eq!(
            why_sentence(&m, &latest, now, false),
            "Active 5 of the last 14 days."
        );
    }

    #[test]
    fn why_unfinished_templates_by_priority() {
        let now = now_fixed();
        let m = metrics(0.0, 0.0, 1, 0, 0);

        let mut todos = row("a", "/x/app", None, None, 0.0, now, 20);
        todos.open_todo_count = 2;
        assert_eq!(
            why_sentence(&m, &todos, now, false),
            "Last session ended with 2 open todos."
        );

        let mut pct = row("a", "/x/app", None, None, 0.0, now, 20);
        pct.completion_pct = Some(60);
        assert_eq!(
            why_sentence(&m, &pct, now, false),
            "Last session at 60% complete."
        );

        let mut plan = row("a", "/x/app", None, None, 0.0, now, 20);
        plan.plan_mode = true;
        assert_eq!(
            why_sentence(&m, &plan, now, false),
            "Last session ended with an unexecuted plan."
        );

        let mut kanban = row("a", "/x/app", None, None, 0.0, now, 20);
        kanban.kanban_status = Some("in_progress".into());
        assert_eq!(why_sentence(&m, &kanban, now, false), "Marked in progress.");
    }

    #[test]
    fn why_pinned_template() {
        let now = now_fixed();
        let latest = row("a", "/x/app", None, None, 30.0, now, 20);
        let m = metrics(0.0, 0.0, 0, 1, 0);
        assert_eq!(why_sentence(&m, &latest, now, false), "Pinned.");
    }

    #[test]
    fn why_two_components_join_with_lowercased_second() {
        let now = now_fixed();
        let mut latest = row("a", "/x/app", None, None, 30.0, now, 20);
        latest.open_todo_count = 2;
        // D weight (40*1.0=40) > U weight (30) → D first, U second lowercased.
        let m = metrics(0.0, 1.0, 1, 0, 14);
        assert_eq!(
            why_sentence(&m, &latest, now, false),
            "Active 14 of the last 14 days; last session ended with 2 open todos."
        );
    }

    #[test]
    fn humanize_relative_buckets() {
        let now = now_fixed();
        assert_eq!(humanize_relative(Some(now), now), "just now");
        assert_eq!(
            humanize_relative(Some(now - Duration::minutes(1)), now),
            "1 minute ago"
        );
        assert_eq!(
            humanize_relative(Some(now - Duration::hours(2)), now),
            "2 hours ago"
        );
        assert_eq!(
            humanize_relative(Some(now - Duration::days(1)), now),
            "yesterday"
        );
        assert_eq!(
            humanize_relative(Some(now - Duration::days(3)), now),
            "3 days ago"
        );
        assert_eq!(
            humanize_relative(Some(now - Duration::days(10)), now),
            "1 week ago"
        );
    }

    // ─── Global aggregates + stale mode ──────────────────────────────────────

    #[test]
    fn stale_mode_flags_and_anchors_density_to_last_activity() {
        let now = now_fixed();
        // Everything is >14 days old → stale; the two sessions are on consecutive
        // days within the window ending at the global max.
        let rows = vec![
            row("a", "/x/app", None, None, 30.0, now, 20),
            row("b", "/x/app", None, None, 31.0, now, 20),
        ];
        let data = build_home(rows, now, 5);
        assert!(data.stale, "global max older than 14 days is stale");
        assert_eq!(data.threads.len(), 1);
        assert_eq!(
            data.threads[0].active_days_14, 2,
            "density anchors to the last-activity window, not now"
        );
        assert_eq!(data.active_threads_this_week, 0);
    }

    #[test]
    fn totals_and_weekly_activity_counted() {
        let now = now_fixed();
        let rows = vec![
            row("a", "/x/app", None, None, 0.0, now, 20),
            row("b", "/x/brain", None, None, 2.0, now, 20),
            row("c", "/x/old", None, None, 20.0, now, 20),
        ];
        let data = build_home(rows, now, 5);
        assert_eq!(data.total_sessions, 3);
        assert_eq!(data.active_threads_this_week, 2, "app + brain within 7 days");
        assert!(!data.stale, "global max (app, today) is recent");
    }

    // ─── L term (open loops) ─────────────────────────────────────────────────

    #[test]
    fn open_loops_raise_rank_over_equal_peer() {
        let now = now_fixed();
        // Same recency/density; only loops differ.
        let with = row_with_loops("a", "/x/alpha", 0.0, now, &["wire e2e", "fix CI"]);
        let without = row("b", "/x/beta", None, None, 0.0, now, 20);
        let data = build_home(vec![without, with], now, 5);
        assert_eq!(data.threads[0].key, "alpha", "open loops should win hero slot");
        assert!(data.threads[0].score > data.threads[1].score);
    }

    #[test]
    fn open_loops_on_thread_capped_at_three() {
        let now = now_fixed();
        let r = row_with_loops(
            "a",
            "/x/app",
            0.0,
            now,
            &["one", "two", "three", "four", "five"],
        );
        let data = build_home(vec![r], now, 5);
        assert_eq!(data.threads[0].open_loops.len(), 3);
        assert_eq!(
            data.threads[0].open_loops,
            vec!["one".to_string(), "two".to_string(), "three".to_string()]
        );
    }

    #[test]
    fn score_includes_25l_term() {
        let now = now_fixed();
        // Four distinct loops → L = 1.0 → +25 vs an identical peer with no loops.
        let with = row_with_loops("a", "/x/app", 0.0, now, &["a", "b", "c", "d"]);
        let without = row("b", "/x/app2", None, None, 0.0, now, 20);
        let m_with = compute_metrics(&[0], &[with], now, now);
        let m_without = compute_metrics(&[0], &[without], now, now);
        assert!((m_with.l - 1.0).abs() < 1e-9);
        assert_eq!(m_with.open_loop_count, 4);
        assert!((m_without.l).abs() < 1e-9);
        let delta = m_with.score - m_without.score;
        assert!(
            (delta - 25.0).abs() < 0.01,
            "L term should add exactly 25; delta was {delta}"
        );
    }

    #[test]
    fn merge_open_loops_dedupes_case_insensitively() {
        let merged = merge_open_loops(
            &["Wire E2E".into(), "fix CI".into()],
            &["wire e2e".into(), "Land PR".into()],
        );
        assert_eq!(merged, vec!["Wire E2E", "fix CI", "Land PR"]);
    }

    #[test]
    fn why_includes_open_loops_when_l_dominates() {
        let now = now_fixed();
        let latest = row_with_loops("a", "/x/app", 30.0, now, &["x", "y"]);
        let mut m = metrics(0.0, 0.0, 0, 0, 0);
        m.l = 0.5;
        m.open_loop_count = 2;
        assert_eq!(why_sentence(&m, &latest, now, false), "2 open loops.");
    }
}
