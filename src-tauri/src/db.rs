//! SQLite schema + connection helpers.
//!
//! Analytic-grade schema: one row per session, plus child tables for the
//! multi-row signals (per-model usage, recaps, files touched, todos, turns,
//! errors). See PLAN — every field traces to a measured source.

use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

/// Where the index lives. Co-located with brain's own logs so it's discoverable
/// and benefits from the same backup story, but namespaced under this app.
pub fn db_path() -> PathBuf {
    let home = dirs::home_dir().expect("no home directory");
    home.join(".claude-sessions-ui").join("index.sqlite")
}

/// Open a connection, creating the parent dir + schema if needed.
/// `read_only` is for query paths that never write (future).
pub fn open() -> rusqlite::Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(&path)?;
    // Perf: WAL gives concurrent reads during a write (the incremental re-index),
    // and a much larger cache makes the 830-row + child-table joins snappy.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "cache_size", -64 * 1024)?; // 64MB
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Idempotent schema creation. All `CREATE IF NOT EXISTS` so re-running is safe.
/// pub(crate) so integration-style tests can land the real schema on a temp DB.
pub(crate) fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    // NOTE: keep field names stable — frontend & indexer depend on them.
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id            TEXT PRIMARY KEY,        -- Claude sessionId (UUID)
            project_dir   TEXT,                    -- encoded dir name (e.g. -Users-sharad-...)
            cwd           TEXT,                    -- resolved, human-readable
            git_branch    TEXT,
            title         TEXT,                    -- aiTitle > lastPrompt > first user msg
            first_ts      TEXT,                    -- ISO 8601 UTC
            last_ts       TEXT,
            message_count INTEGER DEFAULT 0,
            duration_ms   INTEGER DEFAULT 0,
            plan_mode     INTEGER DEFAULT 0,       -- bool: permissionMode == "plan"
            has_recap     INTEGER DEFAULT 0,       -- bool: ≥1 away_summary line
            file_size     INTEGER DEFAULT 0,
            file_path     TEXT UNIQUE NOT NULL,    -- absolute path to the .jsonl
            file_mtime    INTEGER NOT NULL,        -- epoch seconds; drives incremental re-scan
            indexed_at    TEXT
        );

        CREATE TABLE IF NOT EXISTS session_usage (
            session_id          TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            model               TEXT NOT NULL,
            input_toks          INTEGER DEFAULT 0,
            output_toks         INTEGER DEFAULT 0,
            cache_create_toks   INTEGER DEFAULT 0,
            cache_read_toks     INTEGER DEFAULT 0,
            cost_usd            REAL    DEFAULT 0, -- measured from config where available, else pricing×usage
            cost_source         TEXT,              -- 'config' | 'estimate' | null
            api_duration_ms     INTEGER DEFAULT 0,
            PRIMARY KEY (session_id, model)
        );

        -- One row per `away_summary` system line. seq orders them within a session;
        -- the LAST is the end-of-session recap, all rows give the evolution timeline.
        CREATE TABLE IF NOT EXISTS recaps (
            session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            uuid         TEXT PRIMARY KEY,
            captured_ts  TEXT,
            content      TEXT,
            seq          INTEGER,
            is_final     INTEGER DEFAULT 0         -- 1 for the last recap of the session
        );

        CREATE TABLE IF NOT EXISTS files_touched (
            session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            file_path   TEXT NOT NULL,
            snapshots   INTEGER DEFAULT 0,
            PRIMARY KEY (session_id, file_path)
        );

        -- Last-state todos per session (the final TodoWrite call wins; history
        -- is discarded — only the final list is shown).
        CREATE TABLE IF NOT EXISTS todos (
            session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            seq         INTEGER NOT NULL,          -- position in the final list
            content     TEXT,
            status      TEXT,                       -- 'pending' | 'in_progress' | 'completed'
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
            encoded_dir        TEXT PRIMARY KEY,   -- the projects/<name>/ dir
            cwd                TEXT,
            display_name       TEXT,
            session_count      INTEGER DEFAULT 0,
            last_cost_usd      REAL,
            last_lines_added   INTEGER,
            last_lines_removed INTEGER,
            last_modified      TEXT,
            pinned             INTEGER DEFAULT 0
        );

        -- Editable model→$ price table (per million tokens). Seeded with defaults;
        -- user can correct in settings. Drives the 'estimate' cost path.
        CREATE TABLE IF NOT EXISTS pricing (
            model                TEXT PRIMARY KEY,
            input_per_mtok       REAL,
            output_per_mtok      REAL,
            cache_write_per_mtok REAL,
            cache_read_per_mtok  REAL
        );

        -- Metadata for the indexer itself (last full scan, version, etc.).
        CREATE TABLE IF NOT EXISTS index_meta (
            key   TEXT PRIMARY KEY,
            value TEXT
        );
        "#,
    )?;

    // Seed pricing defaults once. UPDATE OR INSERT pattern via temp absence check.
    seed_pricing_if_empty(conn)?;
    migrate_v1(conn)?;
    migrate_v2(conn)?;
    migrate_v3(conn)?;
    migrate_v4(conn)?;
    migrate_v5(conn)?;
    migrate_v6(conn)?;
    Ok(())
}

/// v1: tag-and-triage schema. Establishes the PRAGMA user_version pattern —
/// additive steps run once, guarded by the version, then bump it. Columns are
/// double-guarded by a presence check so an interrupted run stays idempotent.
///
/// Lands the FULL cross-feature schema in one step: blacklist (F1), tag fields
/// (F3, read by F2's chips), kanban placement (F4). All tag/kanban columns are
/// nullable — null means "untagged", which the UI renders as clean absence.
fn migrate_v1(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 1 {
        return Ok(());
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_blacklist (
            pattern    TEXT PRIMARY KEY,   -- dir name or path, optional /** suffix
            created_at TEXT
        );",
    )?;
    // Seed inside the version gate: runs once ever, so deleting it sticks.
    conn.execute(
        "INSERT OR IGNORE INTO project_blacklist (pattern, created_at) VALUES (?1, ?2)",
        rusqlite::params!["udacity-project-reviews/**", chrono::Utc::now().to_rfc3339()],
    )?;

    for (name, decl) in [
        // F3 tag fields (F2 chips read these; absent = null = renders nothing)
        ("area_of_life", "area_of_life TEXT"),
        ("project_short_name", "project_short_name TEXT"),
        ("goal_completed", "goal_completed INTEGER"),
        ("completion_pct", "completion_pct INTEGER"),
        ("tag_rationale", "tag_rationale TEXT"),
        ("tagged_at", "tagged_at TEXT"),
        ("manual_fields", "manual_fields TEXT"), // JSON array of hand-edited field names
        // F4 board placement (override-wins vs %-derived column)
        ("kanban_status", "kanban_status TEXT"),
        ("kanban_order", "kanban_order REAL"),
    ] {
        add_column_if_missing(conn, "sessions", name, decl)?;
    }

    conn.pragma_update(None, "user_version", 1)?;
    Ok(())
}

/// v2: heal titles mangled by the pre-fix `sanitize_title` (which blanket-stripped
/// underscores, so "TAG_AND_TRIAGE_PROMPT.md" was persisted as "TAGANDTRIAGEPROMPT.md").
/// The corrupted title lives in the `sessions` row, so fixing the function alone
/// leaves stored rows wrong. Force a one-time re-derivation by zeroing every
/// session's stored `file_mtime`: the indexer's incremental scan skips a file
/// only when its stored mtime equals the file's real mtime, so a zeroed value
/// guarantees a re-parse (and thus a re-sanitize) on the next pass without a
/// wipe. The upsert rewrites only parsed fields (title, timestamps, counts…),
/// never the tag/kanban columns, so user curation survives untouched.
fn migrate_v2(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 2 {
        return Ok(());
    }
    conn.execute("UPDATE sessions SET file_mtime = 0", [])?;
    conn.pragma_update(None, "user_version", 2)?;
    Ok(())
}

/// v3: honest blacklist counts. Adds `skipped_count` to `project_blacklist` — the
/// per-pattern tally of session files the indexer skips at scan time (files that
/// never enter `sessions`). `blacklist_entries` sums this with the table-match
/// count so a seeded pattern (whose files were never indexed) still reports a
/// live count. The indexer overwrites the tally each pass (see indexer::run), so
/// on-disk deletions are reflected after the next refresh.
fn migrate_v3(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 3 {
        return Ok(());
    }
    add_column_if_missing(
        conn,
        "project_blacklist",
        "skipped_count",
        "skipped_count INTEGER DEFAULT 0",
    )?;
    conn.pragma_update(None, "user_version", 3)?;
    Ok(())
}

/// v4: Timeline digest schema (P6). Three additive tables, all `CREATE IF NOT
/// EXISTS` so an interrupted run re-runs clean, then bump user_version to 4. The
/// LLM only ever fills `session_digests` slots + a `threads`/`thread_members`
/// linking pass; deterministic facts (day grouping, counts) stay in SQL. Every
/// row references `sessions(id)` ON DELETE CASCADE, so a re-index that drops a
/// session takes its digest + thread membership with it — no orphan provenance.
fn migrate_v4(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 4 {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        -- One per-session digest slot. LLM output is untrusted: schema-validated
        -- + referentially checked in Rust before it lands here. content_hash
        -- (FNV-1a over the final recap uuid + content + prompt_version + model)
        -- gates regeneration; manual_fields (JSON array of hand-edited field
        -- names) protects hand-edits from auto-regeneration.
        CREATE TABLE IF NOT EXISTS session_digests (
            session_id     TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
            content_hash   TEXT NOT NULL,
            prompt_version INTEGER NOT NULL,
            model          TEXT,
            worked_on      TEXT,
            outcome        TEXT,
            open_loops     TEXT,        -- JSON array of strings, max 3
            citations      TEXT,        -- JSON array of strings, may be empty
            verified       INTEGER DEFAULT 1,
            confidence     REAL,
            generated_at   TEXT,
            manual_fields  TEXT         -- JSON array of hand-edited field names
        );

        -- A narrative thread linking sessions across days. Members live in
        -- thread_members; a session may reference only existing rows (the
        -- linking pass validates every member id against the input set).
        CREATE TABLE IF NOT EXISTS threads (
            id             TEXT PRIMARY KEY,    -- uuid
            arc            TEXT,                -- one-line narrative
            prompt_version INTEGER NOT NULL,
            generated_at   TEXT
        );

        CREATE TABLE IF NOT EXISTS thread_members (
            thread_id  TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            PRIMARY KEY (thread_id, session_id)
        );
        "#,
    )?;
    conn.pragma_update(None, "user_version", 4)?;
    Ok(())
}

/// v5: Project ontology — a lifecycle status per project. Mirrors the kanban
/// override-wins pattern (db.rs v1): `status` is the user override (null = unset,
/// derived at read time), `status_manual` flags a hand-set value so the indexer's
/// `rebuild_projects` upsert knows never to overwrite it. Vocabulary is validated
/// server-side (PROJECT_STATUSES in lib.rs): `active` | `labs` | `archived` |
/// `inbox`. Archived projects are hidden from Launcher / Home / Digest by default.
fn migrate_v5(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 5 {
        return Ok(());
    }
    add_column_if_missing(conn, "projects", "status", "status TEXT")?;
    add_column_if_missing(
        conn,
        "projects",
        "status_manual",
        "status_manual INTEGER DEFAULT 0",
    )?;
    conn.pragma_update(None, "user_version", 5)?;
    Ok(())
}

/// v6: Review tab — one project report card per (project_key, window_days,
/// window_end). LLM output is untrusted: schema-validated + referentially
/// checked in Rust before it lands here. content_hash (FNV-1a over the input
/// digest hashes + prompt_version + model) gates regeneration so an unchanged
/// week never regenerates; manual_fields protects a hand-edited headline.
fn migrate_v6(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 6 {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS project_reports (
            project_key     TEXT NOT NULL,        -- ontology key, e.g. "NOW/brain"
            window_days     INTEGER NOT NULL,     -- 7 | 14 | 30
            window_end      TEXT NOT NULL,        -- YYYY-MM-DD (local today)
            content_hash    TEXT NOT NULL,        -- FNV-1a over input digest hashes + prompt_version + model
            prompt_version  INTEGER NOT NULL,
            model           TEXT,
            headline        TEXT,                 -- one calm sentence, ≤120 chars
            built           TEXT,                 -- JSON: [{claim, evidence:[sessionId,...]}]
            how             TEXT,                 -- JSON: [string]
            why             TEXT,                 -- JSON: [string]
            desired_vs_real TEXT,                 -- JSON: [{desired, real, status}]
            manual_fields   TEXT,                 -- JSON array of hand-edited field names
            generated_at    TEXT,
            PRIMARY KEY (project_key, window_days, window_end)
        );
        "#,
    )?;
    conn.pragma_update(None, "user_version", 6)?;
    Ok(())
}

/// SQLite has no ALTER TABLE ADD COLUMN IF NOT EXISTS — check table_info first.
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .flatten()
        .any(|c| c == column);
    drop(stmt);
    if !exists {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {decl}"))?;
    }
    Ok(())
}

/// Default per-million-token pricing (USD). Public Anthropic list prices as of
/// mid-2026; user can override. Conservative: if a model isn't found, the
/// indexer falls back to zero cost (and marks it estimate with a null price).
fn seed_pricing_if_empty(conn: &Connection) -> rusqlite::Result<()> {
    // ponytail: inline query
    if conn.query_row("SELECT COUNT(*) FROM pricing", [], |r| r.get::<_, i64>(0))? > 0 {
        return Ok(());
    }
    // (model, in, out, cache_write, cache_read) — $/Mtok
    let rows: &[(&str, f64, f64, f64, f64)] = &[
        // Claude 4 family
        ("claude-opus-4-8", 15.0, 75.0, 18.75, 1.50),
        ("claude-opus-4-7", 15.0, 75.0, 18.75, 1.50),
        ("claude-sonnet-5", 3.0, 15.0, 3.75, 0.30),
        ("claude-haiku-4-5", 1.0, 5.0, 1.25, 0.10),
        ("claude-haiku-4-5-20251001", 1.0, 5.0, 1.25, 0.10),
        ("claude-fable-5", 5.0, 25.0, 6.25, 0.50),
    ];
    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO pricing (model, input_per_mtok, output_per_mtok, cache_write_per_mtok, cache_read_per_mtok) VALUES (?1,?2,?3,?4,?5)",
    )?;
    for (m, i, o, cw, cr) in rows {
        stmt.execute(rusqlite::params![m, i, o, cw, cr])?;
    }
    Ok(())
}

/// All blacklist patterns, oldest first. Missing table (pre-v1 test DBs)
/// degrades to "no blacklist" rather than an error.
pub fn load_blacklist_patterns(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT pattern FROM project_blacklist ORDER BY created_at")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default()
}

/// Encoded dirs of projects currently hidden because they're archived. Mirrors
/// `load_blacklist_patterns`: a one-shot read feeding the per-query post-filter
/// at the three session-load sites. "Archived" = explicit override OR derived
/// (last activity older than ARCHIVE_DAYS). Pre-v5 DBs (no `status` column)
/// degrade to "nothing archived." See lib::archived_predicate.
pub fn load_archived_project_dirs(conn: &Connection) -> std::collections::HashSet<String> {
    conn.prepare(
        "SELECT encoded_dir FROM projects
         WHERE status = 'archived'
            OR (status IS NULL AND status_manual = 0
                AND last_modified IS NOT NULL
                AND last_modified < datetime('now','-90 days'))",
    )
    .and_then(|mut s| {
        s.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| rows.flatten().collect())
    })
    .unwrap_or_default()
}

/// Read a meta value (e.g. "last_full_scan_ts").
pub fn get_meta(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM index_meta WHERE key = ?1",
        rusqlite::params![key],
        |r| r.get(0),
    )
    .ok()
}

/// Write a meta value.
pub fn set_meta(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO index_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creation_and_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        // 1. Verify schema created and pricing seeded
        let num_pricing: i64 = conn.query_row("SELECT COUNT(*) FROM pricing", [], |r| r.get(0)).unwrap();
        assert!(num_pricing > 0, "Pricing table should be seeded");

        // 2. Insert into sessions and query back
        conn.execute(
            "INSERT INTO sessions (id, project_dir, cwd, file_path, file_mtime, title) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params!["session-123", "-home-user-project", "/home/user/project", "/tmp/session.jsonl", 1000, "My Title"],
        ).unwrap();

        let title: String = conn.query_row(
            "SELECT title FROM sessions WHERE id = ?1", 
            rusqlite::params!["session-123"], 
            |r| r.get(0)
        ).unwrap();
        assert_eq!(title, "My Title");

        // 3. Test set_meta / get_meta
        assert_eq!(get_meta(&conn, "some_key"), None);
        set_meta(&conn, "some_key", "some_value").unwrap();
        assert_eq!(get_meta(&conn, "some_key"), Some("some_value".to_string()));

        // update existing meta
        set_meta(&conn, "some_key", "new_value").unwrap();
        assert_eq!(get_meta(&conn, "some_key"), Some("new_value".to_string()));

        // 4. Test child tables (FK checks if any)
        conn.execute(
            "INSERT INTO recaps (session_id, uuid, content) VALUES (?1, ?2, ?3)",
            rusqlite::params!["session-123", "recap-uuid", "recap body"],
        ).unwrap();

        let recap_content: String = conn.query_row(
            "SELECT content FROM recaps WHERE uuid = ?1",
            rusqlite::params!["recap-uuid"],
            |r| r.get(0)
        ).unwrap();
        assert_eq!(recap_content, "recap body");
    }

    #[test]
    fn migrate_v1_lands_schema_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        // user_version bumped to the latest applied migration (v1 schema + v2 heal
        // + v3 blacklist skip-tally column + v4 digest tables + v5 project status + v6 project_reports).
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 6);

        // Blacklist table exists and is seeded exactly once
        let patterns = load_blacklist_patterns(&conn);
        assert_eq!(patterns, vec!["udacity-project-reviews/**".to_string()]);

        // All new sessions columns exist and are nullable (insert without them)
        conn.execute(
            "INSERT INTO sessions (id, file_path, file_mtime) VALUES ('s1', '/tmp/x.jsonl', 1)",
            [],
        ).unwrap();
        let (area, pct, kanban): (Option<String>, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT area_of_life, completion_pct, kanban_status FROM sessions WHERE id='s1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(area, None);
        assert_eq!(pct, None);
        assert_eq!(kanban, None);

        // Re-running migrate is a no-op (idempotent) and does NOT re-seed:
        // deleting the seed must stick.
        conn.execute("DELETE FROM project_blacklist", []).unwrap();
        migrate(&conn).unwrap();
        assert!(load_blacklist_patterns(&conn).is_empty(), "seed must not reappear");
    }

    #[test]
    fn migrate_v2_zeroes_mtime_to_force_title_reheal_without_wiping_tags() {
        // Simulate a pre-v2 DB: schema present but user_version pinned below 2,
        // holding a session with a real mtime and user-set tag/kanban fields.
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, file_path, file_mtime, title, area_of_life, completion_pct, kanban_status)
             VALUES ('s1', '/tmp/x.jsonl', 1700000000, 'TAGANDTRIAGEPROMPT.md', 'Building', 42, 'in_progress')",
            [],
        ).unwrap();

        // Re-run migrations: v2 should zero the mtime (forcing a re-parse) but
        // leave every user-curated field intact.
        migrate(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 6);

        let (mtime, area, pct, kanban): (i64, Option<String>, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT file_mtime, area_of_life, completion_pct, kanban_status FROM sessions WHERE id='s1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(mtime, 0, "mtime must be zeroed so the indexer re-derives the title");
        assert_eq!(area.as_deref(), Some("Building"), "tags must survive the heal");
        assert_eq!(pct, Some(42));
        assert_eq!(kanban.as_deref(), Some("in_progress"));

        // Idempotent: a second run does not re-zero (mtime a later index restored).
        conn.execute("UPDATE sessions SET file_mtime = 1700000001 WHERE id='s1'", []).unwrap();
        migrate(&conn).unwrap();
        let mtime2: i64 = conn.query_row("SELECT file_mtime FROM sessions WHERE id='s1'", [], |r| r.get(0)).unwrap();
        assert_eq!(mtime2, 1700000001, "v2 must not re-run once user_version >= 2");
    }

    #[test]
    fn migrate_v4_lands_digest_tables_and_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        // Cascade only fires with FK enforcement on — db::open sets this pragma,
        // but a bare in-memory connection does not, so enable it here.
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&conn).unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 6);

        // A session to hang a digest + thread off of (FK targets must exist).
        conn.execute(
            "INSERT INTO sessions (id, file_path, file_mtime) VALUES ('s1', '/tmp/x.jsonl', 1)",
            [],
        )
        .unwrap();

        // session_digests round-trips; `verified` defaults to 1.
        conn.execute(
            "INSERT INTO session_digests (session_id, content_hash, prompt_version, worked_on)
             VALUES ('s1', 'deadbeef', 1, 'refactored the indexer')",
            [],
        )
        .unwrap();
        let (worked, verified): (String, i64) = conn
            .query_row(
                "SELECT worked_on, verified FROM session_digests WHERE session_id='s1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(worked, "refactored the indexer");
        assert_eq!(verified, 1, "verified defaults to 1");

        // threads + thread_members, with cascade from threads.
        conn.execute(
            "INSERT INTO threads (id, arc, prompt_version) VALUES ('t1', 'a workstream', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO thread_members (thread_id, session_id) VALUES ('t1', 's1')",
            [],
        )
        .unwrap();
        let n_members: i64 = conn
            .query_row("SELECT COUNT(*) FROM thread_members", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_members, 1);

        // Deleting the thread cascades its membership away.
        conn.execute("DELETE FROM threads WHERE id='t1'", []).unwrap();
        let n_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM thread_members", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_after, 0, "thread_members cascades on thread delete");

        // Re-running migrate is a no-op: version holds, rows survive.
        migrate(&conn).unwrap();
        let v2: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v2, 6);
        let n_digests: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_digests", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_digests, 1, "idempotent migrate must not drop rows");

        // v6: project_reports table landed (one row round-trips).
        conn.execute(
            "INSERT INTO project_reports (project_key, window_days, window_end, content_hash, prompt_version, headline)
             VALUES ('NOW/brain', 7, '2026-07-08', 'deadbeef', 1, 'calm headline')",
            [],
        )
        .unwrap();
        let headline: String = conn
            .query_row(
                "SELECT headline FROM project_reports WHERE project_key='NOW/brain' AND window_days=7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(headline, "calm headline");
    }
}
