//! Resume a Claude Code session in a new Terminal window.
//!
//! Uses AppleScript via `osascript` to open a fresh Terminal, cd into the
//! session's cwd, and run `claude --resume <id>`. This is the Mac-native way
//! and matches what a user would do by hand. `--fork-session` optionally avoids
//! mutating the original transcript (safer for casual browsing).

use std::process::Command;

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
}
