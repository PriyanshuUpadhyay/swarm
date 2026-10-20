//! Provider payload normalization and hook reply rendering.

use super::*;

/// The plain JSON every rule reads: the same keys for every CLI. AGY sends `toolCall` and
/// `conversationId` where Claude and Codex send `tool_name`, `tool_input`, and `session_id`.
pub fn normalize(
    provider: Provider,
    event: &str,
    payload: &serde_json::Value,
) -> serde_json::Value {
    let (tool, input, session) = match provider {
        Provider::Agy => (
            &payload["toolCall"]["name"],
            &payload["toolCall"]["args"],
            &payload["conversationId"],
        ),
        // pi has no swarm guard (ADR 0086); a caller that names pi is read with Claude's keys.
        Provider::Claude | Provider::Codex | Provider::Pi => (
            &payload["tool_name"],
            &payload["tool_input"],
            &payload["session_id"],
        ),
    };
    let cwd = payload["cwd"].as_str().map(str::to_string).or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|dir| dir.to_string_lossy().into_owned())
    });
    serde_json::json!({
        "provider": provider,
        "event": event,
        "tool": tool.as_str().unwrap_or_default(),
        "input": input,
        "cwd": cwd,
        "session_id": session.as_str(),
    })
}

/// The reply each CLI reads from a PreToolUse hook: stdout, stderr, and the exit code. AGY must
/// always get a decision, because `{}` makes it refuse every tool.
pub fn render(provider: Provider, verdict: &Verdict) -> (String, String, i32) {
    match (provider, verdict) {
        (Provider::Claude | Provider::Codex | Provider::Pi, Verdict::Allow) => {
            (String::new(), String::new(), 0)
        }
        (Provider::Claude, Verdict::Deny(reason)) => (
            serde_json::json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason}}).to_string() + "\n",
            String::new(),
            0,
        ),
        (Provider::Codex | Provider::Pi, Verdict::Deny(reason)) => {
            (String::new(), format!("{reason}\n"), 2)
        }
        (Provider::Agy, Verdict::Allow) => ("{\"decision\":\"allow\"}\n".into(), String::new(), 0),
        (Provider::Agy, Verdict::Deny(reason)) => (
            serde_json::json!({"decision": "deny", "reason": reason}).to_string() + "\n",
            String::new(),
            0,
        ),
    }
}
