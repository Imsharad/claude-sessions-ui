//! Resume a Claude Code session in a new Terminal window.
//!
//! Uses AppleScript via `osascript` to open a fresh Terminal, cd into the
//! session's cwd, and run `claude --resume <id>`. This is the Mac-native way
//! and matches what a user would do by hand. `--fork-session` optionally avoids
//! mutating the original transcript (safer for casual browsing).

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub struct ResumeArgs {
    pub session_id: String,
    pub cwd: String,
    pub fork: bool,
}

pub fn open_in_terminal(args: ResumeArgs) -> Result<(), String> {
    // Escape the cwd for double-quoted shell. cdirs from Claude are absolute
    // paths without quotes/backslashes, but be defensive.
    // ponytail: inline cmd and arg logic
    let cmd = format!(
        "cd {} && claude --resume {}{}",
        shell_escape(&args.cwd),
        args.session_id,
        if args.fork { " --fork-session" } else { "" }
    );

    // AppleScript: tell Terminal to (activate and) do the script in a new window.
    // `do script` opens a new window if Terminal has none, or a new tab/window.
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "{}"
        end tell"#,
        escape_applescript(&cmd)
    );

    let out = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("failed to spawn osascript: {e}"))?;

    // ponytail: combinator for shell escape, inline variable
    if !out.status.success() {
        return Err(format!("osascript failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// Minimal shell escape: wrap in single quotes, escape any embedded single quote.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Escape a string for embedding inside an AppleScript double-quoted string.
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
}

// ─── Headless CLI transport (Feature 3: AI tagging) ──────────────────────────
// A second std::process::Command path beside open_in_terminal — but headless,
// not a Terminal launcher. The osascript path dodges the GUI-PATH problem by
// running inside Terminal (landmine 2); a bare Command::new("claude") from a GUI
// process does NOT inherit the login-shell PATH, so we resolve the binary once
// and cache it. Reuses the app's existing auth (the local claude CLI) — no HTTP
// client, no API key (landmine 1).

/// Headless model to shell out to. Fast + cheap, present in the pricing table.
const HEADLESS_TIMEOUT_SECS: u64 = 30;

/// Typed error the tauri commands return so the frontend can branch on `kind`.
/// Kinds: "cli_not_found" | "timeout" | "cli_failed" | "bad_output"
///        | "invalid_json" | "db" (the last two are raised by lib.rs).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagError {
    pub kind: String,
    pub message: String,
}

impl TagError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self { kind: kind.into(), message: message.into() }
    }
}

impl std::fmt::Display for TagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.kind, self.message)
    }
}

impl std::error::Error for TagError {}

/// Cached resolution of the `claude` binary path. `None` = resolved but not
/// found (so we don't re-probe the shell on every tag call).
static CLAUDE_BIN: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Resolve the `claude` binary once. First a login shell (`zsh -lc`) so we pick
/// up the user's real PATH; then a fallback list of known install locations.
/// Cached in a OnceLock. Missing → a typed "cli_not_found" error that names the
/// fix, which the UI surfaces verbatim.
pub fn resolve_claude_bin() -> Result<PathBuf, TagError> {
    let resolved = CLAUDE_BIN.get_or_init(|| {
        // 1. Ask a login shell where claude lives — inherits the user's PATH.
        if let Ok(out) = Command::new("/bin/zsh")
            .arg("-lc")
            .arg("command -v claude")
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() && Path::new(&p).exists() {
                    return Some(PathBuf::from(p));
                }
            }
        }
        // 2. Fallback: probe known install paths directly.
        let home = dirs::home_dir();
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(h) = &home {
            candidates.push(h.join(".claude/local/claude"));
            candidates.push(h.join(".local/bin/claude"));
            candidates.push(h.join(".bun/bin/claude"));
        }
        candidates.push(PathBuf::from("/opt/homebrew/bin/claude"));
        candidates.push(PathBuf::from("/usr/local/bin/claude"));
        candidates.into_iter().find(|p| p.exists())
    });

    resolved.clone().ok_or_else(|| {
        TagError::new(
            "cli_not_found",
            "Claude CLI not found. Install it (npm i -g @anthropic-ai/claude-code) \
             or ensure `claude` is on your PATH — checked your login shell plus \
             ~/.claude/local, ~/.local/bin, ~/.bun/bin, /opt/homebrew/bin, /usr/local/bin.",
        )
    })
}

/// Run the resolved `claude` CLI headlessly and return the `result` string from
/// its `--output-format json` envelope. Enforces a bounded timeout by polling
/// `try_wait()` and killing on expiry — never a silent hang. `current_dir` is
/// the user's home so we don't pick up a project's CLAUDE.md context. Output is
/// a small JSON envelope (~1-2KB), well under the pipe buffer, so polling
/// without draining stdout can't deadlock here.
pub fn run_headless(prompt: &str, model: &str) -> Result<String, TagError> {
    let bin = resolve_claude_bin()?;
    let home = dirs::home_dir()
        .ok_or_else(|| TagError::new("cli_failed", "no home directory to run claude in"))?;

    let mut child = Command::new(&bin)
        .arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--model")
        .arg(model)
        .current_dir(&home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| TagError::new("cli_failed", format!("failed to spawn claude: {e}")))?;

    let deadline = Instant::now() + Duration::from_secs(HEADLESS_TIMEOUT_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(TagError::new(
                        "timeout",
                        format!("claude CLI timed out after {HEADLESS_TIMEOUT_SECS}s"),
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(TagError::new("cli_failed", format!("waiting on claude failed: {e}")))
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| TagError::new("cli_failed", format!("collecting claude output failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TagError::new(
            "cli_failed",
            format!("claude exited with {}: {}", output.status, stderr.trim()),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    extract_result_field(&stdout)
}

/// Extract the `result` string from the CLI's `--output-format json` envelope.
/// Missing / non-string / unparseable → typed "bad_output" error.
fn extract_result_field(stdout: &str) -> Result<String, TagError> {
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| TagError::new("bad_output", format!("claude stdout was not JSON: {e}")))?;
    v.get("result")
        .and_then(|r| r.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            TagError::new("bad_output", "claude JSON envelope had no string `result` field")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_escape_handles_simple_path() {
        assert_eq!(shell_escape("/Users/sharad/brain"), "'/Users/sharad/brain'");
    }

    #[test]
    fn shell_escape_handles_quote() {
        assert_eq!(shell_escape("/path/with'quote"), "'/path/with'\\''quote'");
    }

    #[test]
    fn applescript_escape_handles_backslash_and_quote() {
        assert_eq!(
            escape_applescript(r#"cd "x" && echo \"y\""#),
            r#"cd \"x\" && echo \\\"y\\\""#
        );
    }

    #[test]
    fn extract_result_field_pulls_result_from_envelope() {
        let envelope = r#"{"type":"result","is_error":false,"result":"pong","session_id":"x"}"#;
        assert_eq!(extract_result_field(envelope).unwrap(), "pong");
    }

    #[test]
    fn extract_result_field_rejects_missing_result() {
        let envelope = r#"{"type":"result","is_error":false}"#;
        let err = extract_result_field(envelope).unwrap_err();
        assert_eq!(err.kind, "bad_output");
    }

    #[test]
    fn extract_result_field_rejects_non_json() {
        let err = extract_result_field("not json at all").unwrap_err();
        assert_eq!(err.kind, "bad_output");
    }

    #[test]
    fn tag_error_serializes_camel_case() {
        let e = TagError::new("cli_not_found", "install it");
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"kind\""));
        assert!(json.contains("\"message\""));
        assert!(json.contains("cli_not_found"));
    }
}
