// Provider hook states and terminal program-status reports.

#[derive(Debug, PartialEq)]
pub struct HookState {
    pub state: &'static str,
    pub detail: Option<String>,
    pub kind: Option<&'static str>,
}

/// The agent state one provider hook event reports, with its detail and wait kind, or None when
/// the event says nothing about state. `swarm hook` calls this; see ADR 0021 for the table. AGY puts no event
/// name in its payload, so the caller passes it; Claude and Codex send `hook_event_name`.
pub fn hook_state(provider: &str, event: &str, payload: &serde_json::Value) -> Option<HookState> {
    let text = |name: &str| payload.get(name).and_then(serde_json::Value::as_str);
    let error = text("error").filter(|error| !error.is_empty());
    let tool = text("tool_name");
    let (state, kind) = match (provider, event) {
        // An in-process Claude subagent's events are not the pane's state.
        ("claude", _) if text("agent_id").is_some_and(|id| !id.is_empty()) => return None,
        // Codex asks with a tool that returns at once, then waits in `clocksleep` calls until the
        // owner's reply arrives as a UserPromptSubmit, so a sleep keeps the question's state.
        ("codex", "PreToolUse" | "PostToolUse") if tool == Some("request_user_input_async") => {
            ("waiting", Some("question"))
        }
        ("codex", "PreToolUse" | "PostToolUse") if tool == Some("clocksleep") => return None,
        ("claude" | "codex", "UserPromptSubmit" | "PreToolUse" | "PostToolUse") => {
            ("working", None)
        }
        ("claude" | "codex", "PermissionRequest") => ("waiting", Some("permission")),
        ("claude", "Elicitation") => ("waiting", Some("question")),
        ("claude" | "codex", "Stop") | ("codex", "Interrupt") => ("done", None),
        ("claude", "StopFailure") => ("failed", None),
        ("claude", "Notification") => match text("notification_type")? {
            "permission_prompt" => ("waiting", Some("permission")),
            "elicitation_dialog" => ("waiting", Some("question")),
            "agent_needs_input" => ("waiting", None),
            "idle_prompt" => ("done", None),
            _ => return None,
        },
        ("agy", "PreInvocation" | "PostToolUse") => ("working", None),
        ("agy", "Stop") => match error {
            Some(_) => ("failed", None),
            None => ("done", None),
        },
        _ => return None,
    };
    Some(HookState {
        state,
        detail: if state == "failed" {
            error.map(str::to_string)
        } else {
            None
        },
        kind,
    })
}

/// The state in one OSC 7501 report body, the Program Status Protocol 0.4
/// (<https://www.superlogical.com/rex/docs/build/program-status>). Only the root record is the
/// agent's own state. `seen_work` keeps the `idle` an agent reports at start from reading as a
/// finished turn; after work, `idle` means the owner cancelled it.
pub fn program_status(body: &str, seen_work: bool) -> Option<HookState> {
    let mut pairs = std::collections::HashMap::new();
    for pair in body.split(':') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let value_ok = value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.,+/=-".contains(&b));
        if !key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase()) && value_ok {
            pairs.insert(key, value);
        }
    }
    if pairs.get("id").is_some_and(|id| !id.is_empty()) {
        return None;
    }
    // The spec discards a whole report whose message is bad base64 or holds a control character.
    let message = match pairs.get("msg") {
        Some(msg) => Some(base64_text(msg).filter(|text| !text.chars().any(char::is_control))?),
        None => None,
    };
    let (state, kind) = match *pairs.get("state")? {
        "working" => ("working", None),
        "blocked" => match pairs.get("kind").copied() {
            Some("permission") => ("waiting", Some("permission")),
            Some("question") => ("waiting", Some("question")),
            _ => ("waiting", None),
        },
        "done" => ("done", None),
        "idle" if seen_work => ("done", None),
        "error" => ("failed", None),
        _ => return None,
    };
    Some(HookState {
        state,
        detail: if state == "failed" { message } else { None },
        kind,
    })
}

/// The state and time of a host record, the adapter `status` verb's output: an OSC 7501 body
/// with `:at=<unix seconds>`. A record without its time is None, so an old record never reads as
/// a turn that started after a ring.
pub fn host_record(body: &str, seen_work: bool) -> Option<(HookState, i64)> {
    let at = body
        .split(':')
        .find_map(|pair| pair.trim().strip_prefix("at="))
        .filter(|at| !at.is_empty() && at.bytes().all(|b| b.is_ascii_digit()))?
        .parse()
        .ok()?;
    Some((program_status(body, seen_work)?, at))
}

/// The OSC 7501 report body for a hook state, the inverse of `program_status`, so a terminal that
/// speaks the protocol shows a hooked agent's state. A report replaces its whole record, so each
/// one names the provider as `app`. A failure's detail is left out: it needs base64.
pub fn program_status_report(provider: &str, state: &HookState) -> Option<String> {
    let body = match (state.state, state.kind) {
        ("working", _) => "state=working".to_string(),
        ("waiting", Some(kind)) => format!("state=blocked:kind={kind}"),
        ("waiting", None) => "state=blocked".to_string(),
        ("done", _) => "state=done".to_string(),
        ("failed", _) => "state=error".to_string(),
        _ => return None,
    };
    Some(format!("{body}:app={provider}"))
}

/// Standard base64 with optional padding, decoded as UTF-8.
fn base64_text(text: &str) -> Option<String> {
    let text = text.trim_end_matches('=');
    if text.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::new();
    let (mut bits, mut count) = (0u32, 0);
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    String::from_utf8(bytes).ok()
}

/// Finds OSC 7501 report bodies in a pane's raw output. A report can arrive split across reads.
#[derive(Default)]
pub struct StatusScanner {
    pending: Vec<u8>,
}

impl StatusScanner {
    const START: &[u8] = b"\x1b]7501;";
    /// The spec's cap on one whole sequence.
    const MAX: usize = 4096;

    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut bodies = Vec::new();
        loop {
            let Some(start) = find(&self.pending, Self::START) else {
                // Keep a tail that may hold the start of a split sequence.
                let keep = self.pending.len().min(Self::START.len() - 1);
                self.pending.drain(..self.pending.len() - keep);
                return bodies;
            };
            let body = start + Self::START.len();
            let end = self.pending[body..]
                .iter()
                .enumerate()
                .find_map(|(i, &b)| match b {
                    0x07 => Some((i, 1)),
                    0x1b if self.pending.get(body + i + 1) == Some(&b'\\') => Some((i, 2)),
                    _ => None,
                });
            match end {
                Some((len, terminator)) => {
                    if body + len + terminator - start <= Self::MAX
                        && let Ok(text) = std::str::from_utf8(&self.pending[body..body + len])
                    {
                        bodies.push(text.to_string());
                    }
                    self.pending.drain(..body + len + terminator);
                }
                None if self.pending.len() - start > Self::MAX => {
                    self.pending.drain(..body);
                }
                None => {
                    self.pending.drain(..start);
                    return bodies;
                }
            }
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
