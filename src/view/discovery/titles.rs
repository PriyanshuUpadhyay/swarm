// Read chat titles from agent logs and thread names.
use super::tree;
use serde_json::Value;
use std::{io::Read, path::Path};
/// A chat's title: the agent's own (a Claude rename or AI title, a pi `/name`, a Codex thread
/// name), else the owner's first message.
pub(super) fn read_title(path: &str) -> Option<String> {
    agent_title(path)
        .or_else(|| codex_thread_name(path))
        .or_else(|| first_message(path))
        .map(|title| short(&title))
}

fn short(title: &str) -> String {
    if title.chars().count() > 80 {
        title
            .chars()
            .take(79)
            .collect::<String>()
            .trim_end()
            .to_owned()
            + "…"
    } else {
        title.into()
    }
}

/// The newest title in the last 256 KiB of the log. Claude's `-n` gives a chair the custom title
/// "orchestrator", which is no name for the owner.
fn agent_title(path: &str) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(256 * 1024)))
        .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let (mut custom, mut ai, mut pi) = (None, None, None);
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let text = |key: &str| value[key].as_str().map(str::trim).map(str::to_owned);
        match value["type"].as_str() {
            Some("custom-title") => {
                custom = text("customTitle").filter(|t| !t.is_empty() && t != "orchestrator")
            }
            Some("ai-title") => ai = text("aiTitle").filter(|t| !t.is_empty()),
            // An empty name clears pi's title.
            Some("session_info") if value.get("name").is_some() => {
                pi = text("name").filter(|t| !t.is_empty())
            }
            _ => {}
        }
    }
    custom.or(ai).or(pi)
}

/// Codex keeps thread names in `<codex home>/session_index.jsonl`; a rollout log sits at
/// `<codex home>/sessions/YYYY/MM/DD/rollout-…-<id>.jsonl`.
fn codex_thread_name(path: &str) -> Option<String> {
    let path = Path::new(path);
    let stem = path.file_stem()?.to_str()?;
    if !stem.starts_with("rollout-") || stem.len() < 36 {
        return None;
    }
    let id = &stem[stem.len() - 36..];
    let index = path.ancestors().nth(5)?.join("session_index.jsonl");
    let text = std::fs::read_to_string(index).ok()?;
    text.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|v| v["id"] == id)
        .filter_map(|v| tree::nonblank(v["thread_name"].as_str()))
        .next()
}

fn first_message(path: &str) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(256 * 1024).read_to_end(&mut bytes).ok()?;
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let text = if value["type"] == "user"
            || (value["type"] == "message" && value["message"]["role"] == "user")
        {
            content_text(&value["message"]["content"])
        } else if value["type"] == "response_item" && value["payload"]["role"] == "user" {
            content_text(&value["payload"]["content"])
        } else if value["type"] == "event_msg" && value["payload"]["type"] == "user_message" {
            value["payload"]["message"].as_str().map(str::to_owned)
        } else {
            None
        };
        let Some(text) = text.and_then(|s| tree::nonblank(Some(&s))) else {
            continue;
        };
        if text.starts_with('<') || text.starts_with("# AGENTS.md") {
            continue;
        }
        // A pasted image is a path to a file in uploads (images.ts), which says nothing as a title.
        let title = text.lines().find_map(|line| {
            let words: Vec<_> = line
                .split_whitespace()
                .filter(|w| !(w.starts_with('/') && w.contains("/uploads/")))
                .collect();
            (!words.is_empty()).then(|| words.join(" "))
        });
        if let Some(title) = title {
            return Some(title);
        }
    }
    None
}
fn content_text(content: &Value) -> Option<String> {
    content.as_str().map(str::to_owned).or_else(|| {
        content.as_array().map(|a| {
            a.iter()
                .filter(|b| b["type"] == "text" || b["type"] == "input_text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::read_title;

    fn log(name: &str, lines: &[&str]) -> String {
        let dir = std::env::temp_dir().join(format!("swarm-title-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_title_is_the_agents_own_else_the_first_message() {
        let pi_user = r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"Explain the talk\nmore"}]}}"#;
        assert_eq!(
            read_title(&log("pi.jsonl", &[pi_user])).as_deref(),
            Some("Explain the talk")
        );
        let named = log(
            "pi-named.jsonl",
            &[pi_user, r#"{"type":"session_info","name":"Talk notes"}"#],
        );
        assert_eq!(read_title(&named).as_deref(), Some("Talk notes"));
        let pasted = log(
            "pi-image.jsonl",
            &[
                r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"/h/.swarm/uploads/01a1.png"}]}}"#,
                r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"/h/.swarm/uploads/01a2.png \n\nFix the diff colors"}]}}"#,
            ],
        );
        assert_eq!(read_title(&pasted).as_deref(), Some("Fix the diff colors"));
        let cleared = log(
            "pi-cleared.jsonl",
            &[
                pi_user,
                r#"{"type":"session_info","name":"Talk notes"}"#,
                r#"{"type":"session_info","name":""}"#,
            ],
        );
        assert_eq!(read_title(&cleared).as_deref(), Some("Explain the talk"));
        let claude = log(
            "claude.jsonl",
            &[
                r#"{"type":"custom-title","customTitle":"orchestrator"}"#,
                r#"{"type":"user","message":{"content":"fix it"}}"#,
                r#"{"type":"ai-title","aiTitle":"Fix the build"}"#,
            ],
        );
        assert_eq!(read_title(&claude).as_deref(), Some("Fix the build"));
        let renamed = log(
            "claude-renamed.jsonl",
            &[
                r#"{"type":"ai-title","aiTitle":"Fix the build"}"#,
                r#"{"type":"custom-title","customTitle":"Build"}"#,
            ],
        );
        assert_eq!(read_title(&renamed).as_deref(), Some("Build"));
    }

    #[test]
    fn a_codex_chat_takes_its_thread_name_from_the_session_index() {
        let home = std::env::temp_dir().join(format!("swarm-title-codex-{}", std::process::id()));
        let day = home.join("sessions/2026/10/16");
        std::fs::create_dir_all(&day).unwrap();
        let id = "01a13e6c-b745-7752-bdb0-0a04931eb858";
        let rollout = day.join(format!("rollout-2026-10-16T10-00-00-{id}.jsonl"));
        std::fs::write(
            &rollout,
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"hi there"}}"#
                .to_owned()
                + "\n",
        )
        .unwrap();
        let rollout = rollout.to_string_lossy().into_owned();
        assert_eq!(read_title(&rollout).as_deref(), Some("hi there"));
        std::fs::write(
            home.join("session_index.jsonl"),
            format!("{{\"id\":\"{id}\",\"thread_name\":\"Old\"}}\n{{\"id\":\"{id}\",\"thread_name\":\"Greet\"}}\n"),
        )
        .unwrap();
        assert_eq!(read_title(&rollout).as_deref(), Some("Greet"));
    }
}
