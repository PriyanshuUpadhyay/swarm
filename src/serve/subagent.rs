//! A subagent's own log, opened as a read-only chat (ADR 0140).
//!
//! The page names a child chat `<agent>~sub~<tool call id>`. The parent's log tells where the
//! child's log is: Claude writes `<session>/subagents/agent-<id>.jsonl` with a `.meta.json` that
//! holds `toolUseId`; Codex writes the child as its own rollout file, named by the id in the
//! `spawn_agent` output. AGY keeps no log path in its public docs, and pi has no subagent.

use super::*;
use std::io::{BufRead, BufReader};

/// Between the parent's agent id and the tool call id in a child chat's agent id.
pub(super) const MARK: &str = "~sub~";

/// No CLI writes a tool call id longer than this; a longer one is not a call id.
const CALL_ID_MAX: usize = 128;
/// How a Claude subagent's files are named: `agent-<id>` plus one of these.
const CLAUDE_PREFIX: &str = "agent-";
const CLAUDE_META: &str = ".meta.json";
const CLAUDE_LOG: &str = ".jsonl";
/// Codex keeps rollouts in `sessions/<year>/<month>/<day>/`, so a file sits this far below it.
const CODEX_DEPTH: u8 = 4;
/// The length of a hyphenated UUID.
const UUID_LEN: usize = 36;

/// A tool call id as any of the CLIs writes it; it never holds a path part.
fn plain_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= CALL_ID_MAX
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// The parent agent and the call id of a child chat's agent id.
pub(super) fn split(agent: &str) -> Option<(&str, &str)> {
    agent.rsplit_once(MARK)
}

pub(super) fn child_log(parent: &Log, call_id: &str) -> Result<Log, ApiError> {
    if !plain_id(call_id) {
        return Err(error(StatusCode::BAD_REQUEST, "bad subagent call id"));
    }

    let path = match parent.format.as_str() {
        "claude" => claude_child(&parent.path, call_id),
        "codex" => codex_child(&parent.path, call_id),
        _ => None,
    };

    Ok(Log {
        path: path.ok_or_else(|| error(StatusCode::NOT_FOUND, "no subagent log"))?,
        format: parent.format.clone(),
    })
}

fn claude_child(parent: &Path, call_id: &str) -> Option<PathBuf> {
    let folder = parent.with_extension("").join("subagents");
    let root = folder.canonicalize().ok()?;

    for entry in std::fs::read_dir(&folder).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name
            .strip_prefix(CLAUDE_PREFIX)
            .and_then(|rest| rest.strip_suffix(CLAUDE_META))
        else {
            continue;
        };

        // A file that Claude is still writing is not this call's; the next one may be.
        let Some(meta) = std::fs::read(entry.path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        else {
            continue;
        };
        if meta["toolUseId"] != call_id {
            continue;
        }

        // A link out of the folder is not this session's subagent log.
        let log = folder
            .join(format!("{CLAUDE_PREFIX}{stem}{CLAUDE_LOG}"))
            .canonicalize()
            .ok()?;
        return (log.starts_with(&root) && log.is_file()).then_some(log);
    }
    None
}

/// The Codex child's id is the first UUID in the output of the `spawn_agent` call.
fn codex_child(parent: &Path, call_id: &str) -> Option<PathBuf> {
    let id = spawn_output_id(parent, call_id)?;
    let sessions = parent
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "sessions"))?;
    // ponytail: each read walks the day folders; keep the found path if a long history lags.
    let log = find_rollout(sessions, &format!("-{id}.jsonl"), CODEX_DEPTH)?;

    // Only the first line, the `session_meta`, says whose child this is.
    let mut first = String::new();
    BufReader::new(std::fs::File::open(&log).ok()?)
        .read_line(&mut first)
        .ok()?;
    let meta: Value = serde_json::from_str(&first).ok()?;
    meta["payload"]["source"]["subagent"]["thread_spawn"]["parent_thread_id"]
        .is_string()
        .then_some(log)
}

/// The child id in the output of the call, reading the parent rollout only up to that line.
fn spawn_output_id(parent: &Path, call_id: &str) -> Option<String> {
    let lines = BufReader::new(std::fs::File::open(parent).ok()?).lines();
    lines.map_while(Result::ok).find_map(|line| {
        let record: Value = serde_json::from_str(&line).ok()?;
        let payload = &record["payload"];
        (payload["type"] == "function_call_output" && payload["call_id"] == call_id)
            .then(|| first_uuid(payload["output"].as_str()?))
            .flatten()
    })
}

fn find_rollout(dir: &Path, suffix: &str, depth: u8) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if path.is_dir() && depth > 0 {
            if let Some(found) = find_rollout(&path, suffix, depth - 1) {
                return Some(found);
            }
        } else if name.starts_with("rollout-") && name.ends_with(suffix) {
            return Some(path);
        }
    }
    None
}

/// The first UUID in the text.
fn first_uuid(text: &str) -> Option<String> {
    (0..=text.len().checked_sub(UUID_LEN)?).find_map(|start| {
        let window = text.get(start..start + UUID_LEN)?;
        uuid::Uuid::parse_str(window).ok().map(|id| id.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swarm-sub-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_claude_call_finds_its_child_log_by_tool_use_id() {
        let dir = scratch("claude");
        let session = dir.join("s1");
        let subagents = session.join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        std::fs::write(dir.join("s1.jsonl"), "{}\n").unwrap();

        // A meta file that Claude is still writing must not hide the others.
        std::fs::write(subagents.join("agent-a0.meta.json"), "{").unwrap();
        for (id, call) in [("a1", "toolu_one"), ("a2", "toolu_two")] {
            std::fs::write(subagents.join(format!("agent-{id}.jsonl")), "{}\n").unwrap();
            std::fs::write(
                subagents.join(format!("agent-{id}.meta.json")),
                format!(r#"{{"agentType":"Explore","description":"d","toolUseId":"{call}"}}"#),
            )
            .unwrap();
        }

        let parent = Log {
            path: dir.join("s1.jsonl"),
            format: "claude".into(),
        };
        let child = child_log(&parent, "toolu_two").unwrap();
        assert!(child.path.ends_with("subagents/agent-a2.jsonl"));
        assert_eq!(child.format, "claude");

        assert!(child_log(&parent, "toolu_none").is_err());
        assert!(child_log(&parent, "../x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_codex_call_finds_the_rollout_of_the_agent_it_spawned() {
        let dir = scratch("codex");
        let day = dir.join("sessions/2026/11/11");
        std::fs::create_dir_all(&day).unwrap();

        let child_id = "0199aaaa-bbbb-cccc-dddd-eeeeffff0001";
        let parent =
            day.join("rollout-2026-11-11T10-00-00-0199aaaa-bbbb-cccc-dddd-eeeeffff0000.jsonl");
        let output = format!(r#"{{"agent_id":"{child_id}","nickname":"Ada"}}"#);
        std::fs::write(
            &parent,
            format!(
                "{}\n",
                json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":output}})
            ),
        )
        .unwrap();

        std::fs::write(
            day.join(format!("rollout-2026-11-11T10-01-00-{child_id}.jsonl")),
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":child_id,"source":{"subagent":{"thread_spawn":{"parent_thread_id":"0199aaaa-bbbb-cccc-dddd-eeeeffff0000","depth":1}}}}})
            ),
        )
        .unwrap();

        let log = Log {
            path: parent,
            format: "codex".into(),
        };
        assert!(
            child_log(&log, "call_1")
                .unwrap()
                .path
                .to_string_lossy()
                .ends_with(&format!("{child_id}.jsonl"))
        );
        assert!(child_log(&log, "call_2").is_err());

        let agy = Log {
            path: log.path.clone(),
            format: "agy".into(),
        };
        assert!(child_log(&agy, "call_1").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
