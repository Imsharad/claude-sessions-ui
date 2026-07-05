//! Direct Anthropic Messages API transport for tagging — the fast path beside the
//! headless CLI (claude.rs). One HTTPS POST per classification; no subprocess, no
//! 29k-token agent harness. Falls back to the CLI (caller's job) when no key.

use crate::claude::TagError;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

const API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Pooled agent = keep-alive across calls (saves a TLS handshake on every tag).
static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
fn agent() -> &'static ureq::Agent {
    AGENT.get_or_init(|| ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build())
}

/// API key: env first, then the login shell. A GUI Tauri process does NOT inherit
/// the user's shell env (same landmine as PATH in claude.rs::resolve_claude_bin),
/// so we probe `zsh -lc`. Cached — probe once. None => caller uses the CLI path.
static API_KEY: OnceLock<Option<String>> = OnceLock::new();
fn api_key() -> Option<&'static str> {
    API_KEY
        .get_or_init(|| {
            if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
                if !k.is_empty() {
                    return Some(k);
                }
            }
            if let Ok(out) = Command::new("/bin/zsh")
                .arg("-lc")
                .arg("printf %s \"$ANTHROPIC_API_KEY\"")
                .output()
            {
                if out.status.success() {
                    let k = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !k.is_empty() {
                        return Some(k);
                    }
                }
            }
            None
        })
        .as_deref()
}

pub fn has_api_key() -> bool {
    api_key().is_some()
}

/// One Haiku classification. Returns the assistant text (the JSON the model emitted);
/// the caller reuses extract_json_object + validate_tags, so tag quality is identical
/// to the CLI path. Error kinds: "no_api_key" | "rate_limited" | "api_http" |
/// "timeout" | "bad_output" — all consumed by the same TagError branching in ipc.ts.
///
/// The 200-token classification specialization of [`call_api`]. The digest and
/// thread-linking passes (digest.rs) emit larger JSON, so they call `call_api`
/// directly with a wider token budget over the same transport + error kinds.
pub fn tag_via_api(prompt: &str, model: &str) -> Result<String, TagError> {
    call_api(prompt, model, 200)
}

/// One Messages API completion with a caller-chosen output ceiling. Same pooled
/// agent, same auth probe, same typed error kinds as `tag_via_api`; the only
/// knob is `max_tokens` (tagging wants 200; a multi-field digest or a list of
/// threads needs more headroom, else the JSON truncates and fails validation).
pub fn call_api(prompt: &str, model: &str, max_tokens: u32) -> Result<String, TagError> {
    let key = api_key().ok_or_else(|| {
        TagError::new("no_api_key", "ANTHROPIC_API_KEY not set (checked env + login shell)")
    })?;
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [{ "role": "user", "content": prompt }],
    });
    let resp = match agent()
        .post(API_URL)
        .set("x-api-key", key)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(body)
    {
        Ok(r) => r,
        Err(ureq::Error::Status(429, _)) => {
            return Err(TagError::new("rate_limited", "Anthropic API returned 429"))
        }
        Err(ureq::Error::Status(code, r)) => {
            let msg: String = r.into_string().unwrap_or_default().chars().take(200).collect();
            return Err(TagError::new("api_http", format!("Anthropic API {code}: {msg}")));
        }
        Err(e) => return Err(TagError::new("timeout", format!("Anthropic API transport error: {e}"))),
    };
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| TagError::new("bad_output", format!("API response was not JSON: {e}")))?;
    v.get("content")
        .and_then(|c| c.get(0))
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| TagError::new("bad_output", "API response had no content[0].text"))
}

#[cfg(test)]
mod tests {
    use super::*;
    // Runnable check (offline): with no key configured, tag_via_api returns a typed
    // no_api_key error, never a panic. Proves the graceful-degradation boundary the
    // CLI fallback relies on. Skips silently if a key happens to be present.
    #[test]
    fn missing_key_is_typed_not_panic() {
        if super::api_key().is_none() {
            let err = tag_via_api("hi", "claude-haiku-4-5").unwrap_err();
            assert_eq!(err.kind, "no_api_key");
        }
    }
}
