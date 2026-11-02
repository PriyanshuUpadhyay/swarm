//! Swarm's own walls: checks that `swarm guard` runs before the owner's rule list.

use super::*;

/// ADR 0124: a worker reads the board through `swarm board read`, which withholds the checker's
/// posts, so a worker's tool call that names the board's files is denied. Best effort: a path
/// that the call builds at run time passes.
pub fn board_wall(provider: Provider, event: &str, payload: &str) -> Option<Verdict> {
    let payload: serde_json::Value = serde_json::from_str(payload).ok()?;
    let input = normalize(provider, event, &payload)["input"].to_string();
    input.contains(".swarm/boards").then(|| {
        Verdict::Deny(
            "swarm guard: board files are read with `swarm board read`, so the call is blocked"
                .into(),
        )
    })
}

/// A swarm agent learns of a message when the ring starts its next turn, and a ring that comes
/// during a tool call waits until the call ends. So a shell call that sleeps and then reads the
/// inbox, the agent list, or the board only holds the ring back, and it is denied.
pub fn inbox_wait(provider: Provider, event: &str, payload: &str) -> Option<Verdict> {
    fn text(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::String(text) => {
                out.push_str(text);
                out.push('\n');
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| text(item, out)),
            serde_json::Value::Object(map) => map.values().for_each(|item| text(item, out)),
            _ => {}
        }
    }
    let payload: serde_json::Value = serde_json::from_str(payload).ok()?;
    let mut command = String::new();
    text(&normalize(provider, event, &payload)["input"], &mut command);
    let words: Vec<&str> = command
        .split(|c: char| c.is_whitespace() || ";&|()`$".contains(c))
        .filter(|word| !word.is_empty())
        .collect();
    let sleeps = words.windows(2).any(|pair| {
        pair[0] == "sleep" && pair[1].starts_with(|c: char| c.is_ascii_digit() || c == '.')
    });
    let reads = words.windows(2).any(|pair| {
        pair[0].rsplit('/').next() == Some("swarm")
            && ["inbox", "agents", "board"].contains(&pair[1])
    });
    (sleeps && reads).then(|| Verdict::Deny(INBOX_WAIT.into()))
}

/// Swarm's pi extension ends the turn on a reason that starts with "swarm guard: end your turn".
pub const INBOX_WAIT: &str = "swarm guard: end your turn now. Each reply starts your next turn as \
     the prompt `swarm: new message`, so the call is blocked";
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sleep_before_an_inbox_read_is_denied_and_other_sleeps_pass() {
        let call = |command: &str| {
            let payload =
                serde_json::json!({"tool_name": "Bash", "tool_input": {"command": command}});
            inbox_wait(Provider::Claude, "PreToolUse", &payload.to_string())
        };
        // Commands that swarm chairs ran (pi logs, 2026-10-30 to 2026-11-02).
        for polls in [
            "sleep 60; swarm inbox 2>&1 | head -c 300",
            "swarm ack 10; sleep 270; swarm inbox | tail -n +2 | rg -v '^swarm:'",
            "sleep 5; swarm agents --json | jq -c '[.agents[] | {id,alive}]'",
            "swarm inbox; sleep 5; swarm agents --json",
            "while true; do /opt/homebrew/bin/swarm inbox && break; sleep 30; done",
            "sleep 2 && swarm board read",
        ] {
            assert!(matches!(call(polls), Some(Verdict::Deny(_))), "{polls}");
        }
        for passes in [
            "kill -TERM 70097; for i in 1 2 3; do kill -0 70097 || break; sleep 1; done",
            "swarm inbox | tail -n +2",
            "sleep 0.5 && tmux -L swarm send-keys -t %3 Enter",
            "rg -n sleep src; swarm inbox",
        ] {
            assert_eq!(call(passes), None, "{passes}");
        }
        assert!(crate::bus::PI_LIVE_EXTENSION.contains(&format!(
            "startsWith(\"{}\")",
            &INBOX_WAIT[..INBOX_WAIT.find(" now").unwrap()]
        )));
        // Codex sends the command as an argument list.
        let codex = r#"{"tool_name":"shell","tool_input":{"command":["bash","-lc","sleep 240; swarm inbox"]}}"#;
        assert!(matches!(
            inbox_wait(Provider::Codex, "PreToolUse", codex),
            Some(Verdict::Deny(_))
        ));
    }

    #[test]
    fn a_call_that_names_the_board_files_is_walled() {
        const LS: &str = r#"{"tool_name":"Bash","tool_input":{"command":"ls"}}"#;
        let read =
            r#"{"tool_name":"Read","tool_input":{"file_path":"/Users/u/.swarm/boards/ab12/7.md"}}"#;
        let shell = r#"{"tool_name":"Bash","tool_input":{"command":"cat ~/.swarm/boards/*/7.md"}}"#;
        for payload in [read, shell] {
            assert!(matches!(
                board_wall(Provider::Claude, "PreToolUse", payload),
                Some(Verdict::Deny(_))
            ));
        }
        assert_eq!(board_wall(Provider::Claude, "PreToolUse", LS), None);
    }
}
