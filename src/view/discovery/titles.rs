// Read chat titles and the last reply from agent logs and thread names.
use super::tree;
use serde_json::Value;
use std::{io::Read, path::Path};
/// The most chars of a chat title.
const TITLE_CHARS: usize = 80;

/// A chat's title: the agent's own (a Claude rename or AI title, a pi `/name`, a Codex thread
/// name), else the owner's first message.
pub fn read_title(path: &str) -> Option<String> {
    agent_title(path)
        .or_else(|| codex_thread_name(path))
        .or_else(|| first_message(path))
        .map(|title| tree::shorten(&tree::one_line(&title), TITLE_CHARS))
}

/// The newest title in the log's tail (`screen::log_tail`). Claude's `-n` gives a chair the custom title
/// "orchestrator", which is no name for the owner.
fn agent_title(path: &str) -> Option<String> {
    let bytes = crate::screen::log_tail(Path::new(path))?;
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
        // The lines stay, so the title is the first line with words.
        let Some(text) = text.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()) else {
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

/// ADR 0147: the agent's last reply with words in its log's tail, for the done
/// notice of a CLI whose turn end does not give it. It reads Claude, pi, and Codex logs.
pub fn last_reply(path: &str) -> Option<String> {
    let bytes = crate::screen::log_tail(Path::new(path))?;
    let mut last = None;
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };

        // Claude and pi keep a reply under `message`, Codex under `payload`.
        let content = if value["type"] == "assistant"
            || (value["type"] == "message" && value["message"]["role"] == "assistant")
        {
            &value["message"]["content"]
        } else if value["type"] == "response_item"
            && value["payload"]["type"] == "message"
            && value["payload"]["role"] == "assistant"
        {
            &value["payload"]["content"]
        } else {
            continue;
        };

        if let Some(text) = content_text(content).filter(|text| !text.trim().is_empty()) {
            last = Some(text.trim().to_owned());
        }
    }
    last
}

/// The text blocks of a message: an owner's (`text`, `input_text`) or a reply's (`output_text`).
fn content_text(content: &Value) -> Option<String> {
    content.as_str().map(str::to_owned).or_else(|| {
        content.as_array().map(|a| {
            a.iter()
                .filter(|b| {
                    matches!(
                        b["type"].as_str(),
                        Some("text" | "input_text" | "output_text")
                    )
                })
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{last_reply, read_title};

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
        // A line break in a name breaks the sidebar row in WebKit, U+2028 too.
        let broken = log(
            "claude-broken.jsonl",
            &[r#"{"type":"custom-title","customTitle":"Build\u2028the\n  app"}"#],
        );
        assert_eq!(read_title(&broken).as_deref(), Some("Build the app"));
    }

    /// ADR 0147: a done notice of a CLI whose turn end gives no reply quotes the last reply in
    /// its log, so a newer owner message does not count.
    #[test]
    fn the_last_reply_is_the_newest_assistant_text_of_the_log() {
        let codex = log(
            "codex-reply.jsonl",
            &[
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Old reply."}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Tests pass."}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"thanks"}]}}"#,
            ],
        );
        assert_eq!(last_reply(&codex).as_deref(), Some("Tests pass."));
    }

    #[test]
    fn a_codex_chat_takes_its_thread_name_from_the_session_index() {
        let home = std::env::temp_dir().join(format!("swarm-title-codex-{}", std::process::id()));
        // A process id comes back, so a folder left by an old run can hold a session index.
        let _ = std::fs::remove_dir_all(&home);
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
        std::fs::remove_dir_all(home).unwrap();
    }
}
