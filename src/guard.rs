//! `swarm guard <provider> PreToolUse`: one rule list guards a tool call on every agent CLI
//! (ADR 0040). The runner turns each CLI's payload into one plain JSON, runs each matching rule,
//! and answers in the CLI's own format. A guard that cannot decide blocks the call.

use crate::providers::Provider;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// The hook events a rule may name. `swarm hooks setup` registers the runner for each of them.
pub const EVENTS: [&str; 1] = ["PreToolUse"];
/// The hook timeout each registration gives the runner, in seconds.
pub const REGISTRATION_TIMEOUT: u64 = 10;
/// The runner answers before the CLI's timeout, because a CLI that times a hook out lets the call
/// through.
pub const DEADLINE: Duration = Duration::from_secs(8);
const RULE_TIMEOUT: u64 = 3;
/// How often the runner looks for a rule's exit; it bounds the delay each rule adds.
const POLL: Duration = Duration::from_millis(5);
/// A rule that leaves a process behind can hold stderr open, so the reason gets this long.
const REASON_WAIT: Duration = Duration::from_millis(200);
/// The most of a rule's stderr that becomes a deny reason.
const REASON_MAX: u64 = 64 * 1024;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    rules: Vec<Rule>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    name: String,
    event: String,
    /// Tool names, matched without case. None matches every tool.
    tools: Option<Vec<String>>,
    command: Vec<String>,
    timeout: Option<u64>,
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Allow,
    Deny(String),
}

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
        Provider::Claude | Provider::Codex => (
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

/// Run each rule of `list` (the rule list's text, None when the file is missing) that matches
/// the call, in list order, until one denies.
pub fn decide(
    list: Option<&str>,
    path: &str,
    provider: Provider,
    event: &str,
    payload: &str,
    home: &str,
    deadline: Instant,
) -> Verdict {
    let refuse = |why: String| Verdict::Deny(format!("swarm guard: {why}, so the call is blocked"));
    if !EVENTS.contains(&event) {
        return refuse(format!("{event} is not a guard event"));
    }
    let Some(list) = list else {
        return refuse(format!(
            "{path} is missing; restore it, or remove the `swarm guard` hook from each CLI"
        ));
    };
    let list: List = match serde_json::from_str(list) {
        Ok(list) => list,
        Err(error) => return refuse(format!("{path} is not a valid rule list: {error}")),
    };
    // A rule that can never match would let every call through with no message.
    if let Some(rule) = list.rules.iter().find(|rule| {
        !EVENTS.contains(&rule.event.as_str()) || rule.tools.as_ref().is_some_and(Vec::is_empty)
    }) {
        return refuse(format!(
            "{path} is not a valid rule list: rule {} needs an event in {EVENTS:?} and, when it has tools, at least one",
            rule.name
        ));
    }
    let payload: serde_json::Value = match serde_json::from_str(payload) {
        Ok(value @ serde_json::Value::Object(_)) => value,
        _ => return refuse(format!("the {provider:?} payload is not a JSON object")),
    };
    let call = normalize(provider, event, &payload);
    let tool = call["tool"].as_str().unwrap_or_default();
    // With no tool name no tools filter matches, so every tool-scoped rule would be skipped.
    if tool.is_empty() {
        return refuse(format!("the {provider:?} payload has no tool name"));
    }
    let cwd = call["cwd"].as_str().map(std::path::PathBuf::from);
    let input = call.to_string() + "\n";
    for rule in list.rules.iter().filter(|rule| {
        rule.event == event
            && rule
                .tools
                .as_ref()
                .is_none_or(|tools| tools.iter().any(|name| name.eq_ignore_ascii_case(tool)))
    }) {
        let limit = Duration::from_secs(rule.timeout.unwrap_or(RULE_TIMEOUT))
            .min(deadline.saturating_duration_since(Instant::now()));
        let failure = match run(&rule.command, home, cwd.as_deref(), &input, limit) {
            Outcome::Allow => continue,
            Outcome::Deny(reason) if reason.is_empty() => {
                return Verdict::Deny(format!("rule {} denied the call", rule.name));
            }
            Outcome::Deny(reason) => return Verdict::Deny(reason),
            Outcome::Failed(why) => why,
        };
        return refuse(format!(
            "rule {} failed ({failure}); fix it or remove it from {path}",
            rule.name
        ));
    }
    Verdict::Allow
}

enum Outcome {
    Allow,
    Deny(String),
    Failed(String),
}

/// Run one rule with the call on stdin, in the session's folder when it exists. Exit 0 allows,
/// exit 2 denies with stderr as the reason, and anything else is a failure.
fn run(
    command: &[String],
    home: &str,
    cwd: Option<&std::path::Path>,
    input: &str,
    limit: Duration,
) -> Outcome {
    let command: Vec<String> = command
        .iter()
        .map(|word| match word.strip_prefix("~/") {
            Some(rest) => format!("{home}/{rest}"),
            None => word.clone(),
        })
        .collect();
    let Some((program, args)) = command.split_first() else {
        return Outcome::Failed("its command is empty".into());
    };
    if limit.is_zero() {
        return Outcome::Failed("no time left before the deadline".into());
    }
    let mut process = std::process::Command::new(program);
    if let Some(cwd) = cwd.filter(|cwd| cwd.is_dir()) {
        process.current_dir(cwd);
    }
    let mut child = match process
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return Outcome::Failed(format!("{program} did not start: {error}")),
    };
    // Both pipes go through threads: a rule that does not read a large payload, or that writes
    // much to stderr, must not stall the runner past its deadline.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = input.to_string();
    std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (sender, reason) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = (&mut stderr).take(REASON_MAX).read_to_end(&mut text);
        // Each invalid byte decodes to a 3-byte U+FFFD, so the cut is made again on the text, at
        // a character boundary, to keep the reason within 64 KiB.
        let mut text = String::from_utf8_lossy(&text).into_owned();
        text.truncate(text.floor_char_boundary(REASON_MAX as usize));
        let _ = sender.send(text);
        // The rest is read and dropped, so a rule that writes more does not die on a closed pipe.
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < limit => std::thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::Failed(format!("no answer within {} s", limit.as_secs_f32()));
            }
            Err(error) => return Outcome::Failed(format!("cannot wait for it: {error}")),
        }
    };
    match status.code() {
        Some(0) => Outcome::Allow,
        Some(2) => Outcome::Deny(
            reason
                .recv_timeout(REASON_WAIT)
                .unwrap_or_default()
                .trim()
                .to_string(),
        ),
        Some(code) => Outcome::Failed(format!("exit {code}")),
        None => Outcome::Failed("killed by a signal".into()),
    }
}

/// The reply each CLI reads from a PreToolUse hook: stdout, stderr, and the exit code. AGY must
/// always get a decision, because `{}` makes it refuse every tool.
pub fn render(provider: Provider, verdict: &Verdict) -> (String, String, i32) {
    match (provider, verdict) {
        (Provider::Claude | Provider::Codex, Verdict::Allow) => (String::new(), String::new(), 0),
        (Provider::Claude, Verdict::Deny(reason)) => (
            serde_json::json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason}}).to_string() + "\n",
            String::new(),
            0,
        ),
        (Provider::Codex, Verdict::Deny(reason)) => (String::new(), format!("{reason}\n"), 2),
        (Provider::Agy, Verdict::Allow) => ("{\"decision\":\"allow\"}\n".into(), String::new(), 0),
        (Provider::Agy, Verdict::Deny(reason)) => (
            serde_json::json!({"decision": "deny", "reason": reason}).to_string() + "\n",
            String::new(),
            0,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_BASH: &str = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"},"session_id":"s1","cwd":"/w"}"#;

    fn list(rules: &str) -> String {
        format!(r#"{{"rules": [{rules}]}}"#)
    }

    fn rule(name: &str, script: &str, extra: &str) -> String {
        format!(
            r#"{{"name": "{name}", "event": "PreToolUse", "command": ["/bin/sh", "-c", {}]{extra}}}"#,
            serde_json::to_string(script).unwrap()
        )
    }

    fn decide_claude(list: Option<&str>) -> Verdict {
        decide(
            list,
            "/h/.swarm/guards.json",
            Provider::Claude,
            "PreToolUse",
            CLAUDE_BASH,
            "/h",
            Instant::now() + DEADLINE,
        )
    }

    #[test]
    fn exit_0_allows_and_exit_2_denies_with_stderr_as_the_reason() {
        assert_eq!(
            decide_claude(Some(&list(&rule("ok", "exit 0", "")))),
            Verdict::Allow
        );
        assert_eq!(
            decide_claude(Some(&list(&rule(
                "no",
                "echo 'no force push' >&2; exit 2",
                ""
            )))),
            Verdict::Deny("no force push".into())
        );
        assert_eq!(
            decide_claude(Some(&list(&rule("quiet", "exit 2", "")))),
            Verdict::Deny("rule quiet denied the call".into())
        );
    }

    #[test]
    fn a_guard_that_crashes_times_out_or_does_not_start_blocks_the_call() {
        for (script, why) in [
            ("exit 1", "exit 1"),
            ("kill -9 $$", "killed by a signal"),
            ("sleep 5", "no answer within 1 s"),
        ] {
            let verdict = decide_claude(Some(&list(&rule("g", script, r#", "timeout": 1"#))));
            assert_eq!(
                verdict,
                Verdict::Deny(format!(
                    "swarm guard: rule g failed ({why}); fix it or remove it from /h/.swarm/guards.json, so the call is blocked"
                ))
            );
        }
        let missing =
            list(r#"{"name": "gone", "event": "PreToolUse", "command": ["~/no-such-rule"]}"#);
        let Verdict::Deny(reason) = decide_claude(Some(&missing)) else {
            panic!("a missing rule program allowed the call");
        };
        assert!(
            reason.starts_with("swarm guard: rule gone failed (/h/no-such-rule did not start"),
            "{reason}"
        );
    }

    #[test]
    fn a_missing_or_broken_list_or_payload_blocks_the_call() {
        let Verdict::Deny(missing) = decide_claude(None) else {
            panic!()
        };
        assert!(
            missing.starts_with("swarm guard: /h/.swarm/guards.json is missing"),
            "{missing}"
        );
        for bad in [
            "{",
            r#"{"rules": [{"name": "x"}]}"#,
            r#"{"rules": [], "extra": 1}"#,
        ] {
            let Verdict::Deny(reason) = decide_claude(Some(bad)) else {
                panic!("{bad}")
            };
            assert!(reason.contains("is not a valid rule list"), "{reason}");
        }
        let verdict = decide(
            Some(&list("")),
            "p",
            Provider::Codex,
            "PreToolUse",
            "[1]",
            "/h",
            Instant::now() + DEADLINE,
        );
        assert_eq!(
            verdict,
            Verdict::Deny(
                "swarm guard: the Codex payload is not a JSON object, so the call is blocked"
                    .into()
            )
        );
        for payload in ["{}", r#"{"tool_name":["Bash"]}"#, r#"{"tool_name":""}"#] {
            let verdict = decide(
                Some(&list("")),
                "p",
                Provider::Codex,
                "PreToolUse",
                payload,
                "/h",
                Instant::now() + DEADLINE,
            );
            assert_eq!(
                verdict,
                Verdict::Deny(
                    "swarm guard: the Codex payload has no tool name, so the call is blocked"
                        .into()
                ),
                "{payload}"
            );
        }
        let verdict = decide(
            Some(&list("")),
            "p",
            Provider::Codex,
            "Stop",
            "{}",
            "/h",
            Instant::now() + DEADLINE,
        );
        assert_eq!(
            verdict,
            Verdict::Deny("swarm guard: Stop is not a guard event, so the call is blocked".into())
        );
    }

    #[test]
    fn a_rule_runs_only_for_its_event_and_tools_and_the_first_deny_wins() {
        let rules = [
            rule("edits", "exit 2", r#", "tools": ["Edit"]"#),
            rule("shell", "echo first >&2; exit 2", r#", "tools": ["bash"]"#),
            rule("never", "echo second >&2; exit 2", ""),
        ]
        .join(",");
        assert_eq!(
            decide_claude(Some(&list(&rules))),
            Verdict::Deny("first".into())
        );
    }

    #[test]
    fn the_deadline_bounds_the_whole_run() {
        let verdict = decide(
            Some(&list(&rule("slow", "sleep 5", r#", "timeout": 5"#))),
            "p",
            Provider::Claude,
            "PreToolUse",
            CLAUDE_BASH,
            "/h",
            Instant::now() + Duration::from_millis(300),
        );
        let Verdict::Deny(reason) = verdict else {
            panic!()
        };
        // A loaded machine can spend the 300 ms before the rule starts.
        assert!(
            reason.contains("(no answer within 0.") || reason.contains("(no time left"),
            "{reason}"
        );
    }

    #[test]
    fn every_cli_payload_becomes_the_same_plain_call() {
        let claude: serde_json::Value = serde_json::from_str(CLAUDE_BASH).unwrap();
        let agy = serde_json::json!({"toolCall": {"name": "run_command", "args": {"CommandLine": "ls"}}, "conversationId": "c1", "transcriptPath": "/t"});
        assert_eq!(
            normalize(Provider::Claude, "PreToolUse", &claude),
            serde_json::json!({"provider": "claude", "event": "PreToolUse", "tool": "Bash", "input": {"command": "ls"}, "cwd": "/w", "session_id": "s1"})
        );
        let call = normalize(Provider::Agy, "PreToolUse", &agy);
        assert_eq!(
            (
                &call["provider"],
                &call["tool"],
                &call["input"],
                &call["session_id"]
            ),
            (
                &serde_json::json!("agy"),
                &serde_json::json!("run_command"),
                &serde_json::json!({"CommandLine": "ls"}),
                &serde_json::json!("c1")
            )
        );
        assert!(call["cwd"].is_string());
        let empty = normalize(Provider::Codex, "PreToolUse", &serde_json::json!({}));
        assert_eq!(
            (&empty["tool"], &empty["input"], &empty["session_id"]),
            (
                &serde_json::json!(""),
                &serde_json::Value::Null,
                &serde_json::Value::Null
            )
        );
    }

    #[test]
    fn each_cli_gets_its_own_allow_and_deny() {
        let deny = Verdict::Deny("no".into());
        assert_eq!(
            render(Provider::Claude, &Verdict::Allow),
            (String::new(), String::new(), 0)
        );
        assert_eq!(
            render(Provider::Claude, &deny).0,
            "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"no\"}}\n"
        );
        assert_eq!(
            render(Provider::Codex, &Verdict::Allow),
            (String::new(), String::new(), 0)
        );
        assert_eq!(
            render(Provider::Codex, &deny),
            (String::new(), "no\n".into(), 2)
        );
        assert_eq!(
            render(Provider::Agy, &Verdict::Allow).0,
            "{\"decision\":\"allow\"}\n"
        );
        assert_eq!(
            render(Provider::Agy, &deny),
            (
                "{\"decision\":\"deny\",\"reason\":\"no\"}\n".into(),
                String::new(),
                0
            )
        );
    }

    #[test]
    fn a_rule_that_can_never_match_makes_the_list_invalid() {
        for bad in [
            r#"{"name": "later", "event": "PostToolUse", "command": ["/bin/true"]}"#,
            r#"{"name": "typo", "event": "pretooluse", "command": ["/bin/true"]}"#,
            r#"{"name": "none", "event": "PreToolUse", "tools": [], "command": ["/bin/true"]}"#,
        ] {
            let Verdict::Deny(reason) = decide_claude(Some(&list(bad))) else {
                panic!("{bad}")
            };
            assert!(
                reason.contains("is not a valid rule list: rule "),
                "{reason}"
            );
        }
    }

    #[test]
    fn every_command_word_expands_home_and_the_rule_runs_in_the_session_folder() {
        let home = std::env::temp_dir().join(format!("swarm-guard-home-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("rule.sh"), "pwd >&2; exit 2\n").unwrap();
        let rules =
            list(r#"{"name": "h", "event": "PreToolUse", "command": ["/bin/sh", "~/rule.sh"]}"#);
        let payload = format!(
            r#"{{"tool_name": "Bash", "tool_input": {{}}, "cwd": "{}"}}"#,
            home.display()
        );
        let verdict = decide(
            Some(&rules),
            "p",
            Provider::Claude,
            "PreToolUse",
            &payload,
            &home.to_string_lossy(),
            Instant::now() + DEADLINE,
        );
        let folder = std::fs::canonicalize(&home).unwrap();
        assert_eq!(
            verdict,
            Verdict::Deny(folder.to_string_lossy().into_owned())
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_rule_that_writes_much_to_stderr_still_allows_or_denies() {
        // The shell writes 205 KB itself, so a closed pipe would kill it.
        let loud = "i=0; while [ $i -lt 5000 ]; do echo 0123456789012345678901234567890123456789 >&2; i=$((i+1)); done";
        assert_eq!(
            decide_claude(Some(&list(&rule(
                "quiet-ok",
                &format!("{loud}; exit 0"),
                ""
            )))),
            Verdict::Allow
        );
    }

    #[test]
    fn a_deny_reason_is_cut_at_64_kib() {
        let script = "i=0; while [ $i -lt 5000 ]; do echo 0123456789012345678901234567890123456789 >&2; i=$((i+1)); done; exit 2";
        let Verdict::Deny(reason) = decide_claude(Some(&list(&rule("loud", script, "")))) else {
            panic!()
        };
        assert_eq!(reason.len(), 64 * 1024);
        // A character cut at the limit is dropped, not shown as U+FFFD.
        let script = r"printf '%65535s' '' | tr ' ' a >&2; printf '\303\251' >&2; exit 2";
        let Verdict::Deny(reason) = decide_claude(Some(&list(&rule("wide", script, "")))) else {
            panic!()
        };
        assert_eq!(reason.len(), 64 * 1024 - 1);
        let script = r"head -c 65536 /dev/zero | LC_ALL=C tr '\0' '\377' >&2; exit 2";
        let Verdict::Deny(reason) = decide_claude(Some(&list(&rule("bytes", script, "")))) else {
            panic!()
        };
        assert_eq!(reason.len(), 64 * 1024 - 1);
    }
}
