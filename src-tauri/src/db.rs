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
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
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
}
