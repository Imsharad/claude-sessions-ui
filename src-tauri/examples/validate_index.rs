//! Standalone validator: runs the indexer against real ~/.claude data and
//! prints gate numbers. Run with: `cargo run --example validate_index`.
//!
//! This bypasses Tauri entirely — it talks to the indexer + db modules directly.

use claude_sessions_ui_lib::{db, indexer};
use rusqlite::params;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp_secs()
        .init();

    println!("Opening index DB at {:?} …", db::db_path());
    let conn = db::open().expect("open db");

    println!("Running FULL scan…");
    let stats = indexer::run(&conn, true);
    println!(
        "  mode={}  seen={}  reindexed={}  skipped_uptodate={}  upserted={}  in {}ms",
        stats.mode, stats.files_seen, stats.files_reindexed, stats.files_skipped_uptodate,
        stats.sessions_upserted, stats.duration_ms
    );
    if let Some(e) = &stats.error {
        println!("  ⚠ last error: {}", e);
    }

    println!("\nNow run INCREMENTAL (should re-parse ~0 files)…");
    let stats2 = indexer::run(&conn, false);
    println!(
        "  mode={}  seen={}  reindexed={}  skipped_uptodate={}  in {}ms",
        stats2.mode, stats2.files_seen, stats2.files_reindexed, stats2.files_skipped_uptodate, stats2.duration_ms
    );

    // ---- Gate checks vs recon numbers ----
    println!("\n=== GATE CHECKS (vs recon: 830 sessions, 543 recaps / 224 sessions, 45.3M input tokens) ===");
    let n_sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    let n_recap_lines: i64 = conn
        .query_row("SELECT COUNT(*) FROM recaps", [], |r| r.get(0))
        .unwrap();
    let n_sessions_with_recap: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions WHERE has_recap=1", [], |r| r.get(0))
        .unwrap();
    let (input_toks, output_toks, cache_read): (i64, i64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(input_toks),0), COALESCE(SUM(output_toks),0),
                    COALESCE(SUM(cache_read_toks),0) FROM session_usage",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or((0, 0, 0));
    let n_projects: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
        .unwrap();
    let n_files_touched: i64 = conn
        .query_row("SELECT COUNT(*) FROM files_touched", [], |r| r.get(0))
        .unwrap();
    let n_todos: i64 = conn
        .query_row("SELECT COUNT(*) FROM todos", [], |r| r.get(0))
        .unwrap();
    let n_errors: i64 = conn
        .query_row("SELECT COUNT(*) FROM errors", [], |r| r.get(0))
        .unwrap();
    let n_turns: i64 = conn
        .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))
        .unwrap();

    println!("  sessions               : {:>6}   (recon: 830)", n_sessions);
    println!("  recap lines            : {:>6}   (recon: 543)", n_recap_lines);
    println!("  sessions w/ recap      : {:>6}   (recon: 224)", n_sessions_with_recap);
    println!("  projects               : {:>6}   (recon: 444)", n_projects);
    println!("  input_tokens (sum)     : {:>6} (recon: 45.3M)", fmt_millions(input_toks));
    println!("  output_tokens (sum)    : {:>6}", fmt_millions(output_toks));
    println!("  cache_read_tokens (sum): {:>6} (recon: 5.3B)", fmt_millions(cache_read));
    println!("  files_touched rows     : {:>6}   (recon: 8,131 source-file entries tracked)", n_files_touched);
    println!("  todos                  : {:>6}", n_todos);
    println!("  api errors             : {:>6}   (recon: 36)", n_errors);
    println!("  turns (turn_duration)  : {:>6}   (recon: 1,529)", n_turns);

    // Cost summary
    let (total_cost, measured, estimated): (f64, f64, f64) = conn
        .query_row(
            "SELECT COALESCE(SUM(cost_usd),0),
                    COALESCE(SUM(CASE WHEN cost_source='config' THEN cost_usd ELSE 0 END),0),
                    COALESCE(SUM(CASE WHEN cost_source='estimate' THEN cost_usd ELSE 0 END),0)
             FROM session_usage",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or((0.0, 0.0, 0.0));
    println!(
        "\n  cost: total ${:.2}   measured ${:.2}   estimated ${:.2}",
        total_cost, measured, estimated
    );

    // Top 5 projects by session count
    println!("\n  Top 5 projects by session count:");
    let mut stmt = conn
        .prepare("SELECT display_name, session_count, cwd FROM projects ORDER BY session_count DESC LIMIT 5")
        .unwrap();
    let top: Vec<(String, i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    for (name, count, cwd) in top {
        println!("    {:>3}  {:<24}  {}", count, name, cwd);
    }

    // Sample a recap to eyeball content quality
    println!("\n  Sample recap (most recent final):");
    let sample: Option<String> = conn
        .query_row(
            "SELECT content FROM recaps WHERE is_final=1 ORDER BY captured_ts DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(s) = sample {
        let preview: String = s.chars().take(200).collect();
        println!("    {}", preview);
    } else {
        println!("    (none)");
    }

    // Verify a couple of known recon facts: the 752fb0d0 session is in the index.
    let known: Option<String> = conn
        .query_row(
            "SELECT title FROM sessions WHERE id = ?1",
            params!["752fb0d0-15ad-4e60-8d45-9ca8d7121e86"],
            |r| r.get(0),
        )
        .ok();
    println!("\n  Known session (752fb0d0 qmd-speedup) title: {:?}", known);

    println!("\nDone. Index DB is now populated and ready for the frontend.");
}

fn fmt_millions(n: i64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}
