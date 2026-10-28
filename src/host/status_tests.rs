// Hook-state mappings and terminal status-report tests.

use super::*;

#[test]
fn a_host_record_needs_its_time() {
    let record = |body| host_record(body, true).map(|(s, at)| (s.state, s.kind, at));
    assert_eq!(
        record("state=working:at=1792408632"),
        Some(("working", None, 1792408632))
    );
    assert_eq!(
        record("state=blocked:kind=permission:at=5"),
        Some(("waiting", Some("permission"), 5))
    );
    assert_eq!(record("state=working"), None);
    assert_eq!(record("state=working:at=soon"), None);
    assert_eq!(record("state=working:at=-5"), None);
    assert_eq!(record("state=working:id=build:at=5"), None);
}

#[test]
fn program_status_maps_the_root_record() {
    let state = |body: &str, seen_work: bool| {
        program_status(body, seen_work).map(|s| (s.state, s.kind, s.detail))
    };
    assert_eq!(
        state("state=working:app=pi", false),
        Some(("working", None, None))
    );
    assert_eq!(
        state("state=blocked:kind=question", true),
        Some(("waiting", Some("question"), None))
    );
    assert_eq!(state("state=blocked:kind=auth", true), None);
    assert_eq!(
        state("state=blocked", true),
        Some(("waiting", None, None))
    );
    assert_eq!(state("state=done:app=pi", true), Some(("done", None, None)));
    // pi reports idle when it starts and when the owner cancels a run.
    assert_eq!(state("state=idle:app=pi", false), None);
    assert_eq!(state("state=idle:app=pi", true), Some(("done", None, None)));
    // "rate limited", base64 without padding.
    assert_eq!(
        state("state=error:msg=cmF0ZSBsaW1pdGVk", true),
        Some(("failed", None, Some("rate limited".into())))
    );
    assert_eq!(state("state=working:id=build/test", true), None);
    assert_eq!(state("state=clear", true), None);
    assert_eq!(state("state=paused", true), None);
    assert_eq!(state("app=pi", true), None);
    // A message with a newline ("a\nb") or bad base64 discards the report, but a pair with
    // a byte outside the value set is skipped.
    assert_eq!(state("state=error:msg=YQpi", true), None);
    assert_eq!(state("state=error:msg=A", true), None);
    assert_eq!(
        state("state=error:msg=***", true),
        Some(("failed", None, None))
    );
    // A malformed pair is skipped and the rest still counts.
    assert_eq!(state("junk:state=done", true), Some(("done", None, None)));
}

#[test]
fn a_hook_state_report_reads_back_as_the_same_state() {
    let report = |state: &'static str, kind: Option<&'static str>| {
        let hook = HookState {
            state,
            detail: None,
            kind,
        };
        program_status_report("claude", &hook)
    };
    assert_eq!(
        report("waiting", Some("permission")).as_deref(),
        Some("state=blocked:kind=permission:app=claude")
    );
    assert_eq!(report("idle", None), None);
    for (state, kind) in [
        ("working", None),
        ("waiting", Some("permission")),
        ("waiting", Some("question")),
        ("waiting", None),
        ("done", None),
        ("failed", None),
    ] {
        let body = report(state, kind).unwrap();
        let back = program_status(&body, true).unwrap();
        assert_eq!((back.state, back.kind), (state, kind), "{body}");
    }
}

#[test]
fn status_scanner_finds_reports_across_reads() {
    let mut scanner = StatusScanner::default();
    assert!(scanner.push(b"text \x1b]75").is_empty());
    assert_eq!(scanner.push(b"01;state=working\x07more"), ["state=working"]);
    assert!(scanner.push(b"\x1b]7501;state=do").is_empty());
    assert_eq!(
        scanner.push(b"ne\x1b\\\x1b]0;title\x07\x1b]7501;state=idle\x1b\\"),
        ["state=done", "state=idle"]
    );
    let mut long = b"\x1b]7501;state=working:msg=".to_vec();
    long.extend(std::iter::repeat_n(b'A', 5000));
    long.extend(b"\x07\x1b]7501;state=done\x07");
    assert_eq!(scanner.push(&long), ["state=done"]);
}
#[test]
fn hook_waits_carry_the_permission_or_question_kind() {
    use serde_json::json;
    for (provider, event, payload, expected) in [
        ("claude", "PermissionRequest", json!({}), Some("permission")),
        ("codex", "PermissionRequest", json!({}), Some("permission")),
        ("claude", "Elicitation", json!({}), Some("question")),
        (
            "claude",
            "Notification",
            json!({"notification_type": "permission_prompt"}),
            Some("permission"),
        ),
        (
            "claude",
            "Notification",
            json!({"notification_type": "elicitation_dialog"}),
            Some("question"),
        ),
        (
            "claude",
            "Notification",
            json!({"notification_type": "agent_needs_input"}),
            None,
        ),
        ("claude", "PreToolUse", json!({}), None),
        ("codex", "Stop", json!({}), None),
        ("claude", "StopFailure", json!({"error": "quota"}), None),
        ("agy", "Stop", json!({"error": "quota"}), None),
    ] {
        let report = hook_state(provider, event, &payload).unwrap();
        assert_eq!(report.kind, expected, "{provider} {event} {payload}");
        if expected.is_some() {
            assert_eq!(report.state, "waiting");
        }
    }
}

#[test]
fn maps_every_hook_event_in_the_state_table() {
    use serde_json::json;
    let empty = json!({});
    let notification = |kind: &str| json!({"notification_type": kind});
    type Expected<'a> = Option<(&'a str, Option<&'a str>)>;
    let cases: &[(&str, &str, serde_json::Value, Expected)] = &[
        (
            "claude",
            "UserPromptSubmit",
            empty.clone(),
            Some(("working", None)),
        ),
        (
            "claude",
            "PreToolUse",
            empty.clone(),
            Some(("working", None)),
        ),
        (
            "claude",
            "PostToolUse",
            empty.clone(),
            Some(("working", None)),
        ),
        (
            "claude",
            "PermissionRequest",
            empty.clone(),
            Some(("waiting", None)),
        ),
        (
            "claude",
            "Elicitation",
            empty.clone(),
            Some(("waiting", None)),
        ),
        (
            "claude",
            "Notification",
            notification("permission_prompt"),
            Some(("waiting", None)),
        ),
        (
            "claude",
            "Notification",
            notification("elicitation_dialog"),
            Some(("waiting", None)),
        ),
        (
            "claude",
            "Notification",
            notification("agent_needs_input"),
            Some(("waiting", None)),
        ),
        (
            "claude",
            "Notification",
            notification("idle_prompt"),
            Some(("done", None)),
        ),
        ("claude", "Notification", notification("auth_success"), None),
        ("claude", "Notification", empty.clone(), None),
        ("claude", "Stop", empty.clone(), Some(("done", None))),
        (
            "claude",
            "StopFailure",
            json!({"error": "rate_limit"}),
            Some(("failed", Some("rate_limit"))),
        ),
        (
            "claude",
            "StopFailure",
            empty.clone(),
            Some(("failed", None)),
        ),
        (
            "claude",
            "PreToolUse",
            json!({"agent_id": "explorer"}),
            None,
        ),
        (
            "claude",
            "Stop",
            json!({"agent_id": ""}),
            Some(("done", None)),
        ),
        ("claude", "SessionStart", empty.clone(), None),
        ("claude", "Interrupt", empty.clone(), None),
        (
            "codex",
            "UserPromptSubmit",
            empty.clone(),
            Some(("working", None)),
        ),
        (
            "codex",
            "PreToolUse",
            empty.clone(),
            Some(("working", None)),
        ),
        (
            "codex",
            "PostToolUse",
            empty.clone(),
            Some(("working", None)),
        ),
        (
            "codex",
            "PermissionRequest",
            empty.clone(),
            Some(("waiting", None)),
        ),
        ("codex", "Stop", empty.clone(), Some(("done", None))),
        ("codex", "Interrupt", empty.clone(), Some(("done", None))),
        ("codex", "StopFailure", empty.clone(), None),
        ("codex", "SessionStart", empty.clone(), None),
        (
            "agy",
            "PreInvocation",
            empty.clone(),
            Some(("working", None)),
        ),
        ("agy", "PostToolUse", empty.clone(), Some(("working", None))),
        ("agy", "Stop", empty.clone(), Some(("done", None))),
        ("agy", "Stop", json!({"error": ""}), Some(("done", None))),
        (
            "agy",
            "Stop",
            json!({"error": "quota"}),
            Some(("failed", Some("quota"))),
        ),
        ("agy", "PreToolUse", empty.clone(), None),
        ("gemini", "Stop", empty.clone(), None),
    ];
    for (provider, event, payload, expected) in cases {
        let got = hook_state(provider, event, payload);
        let got = got
            .as_ref()
            .map(|report| (report.state, report.detail.as_deref()));
        assert_eq!(got, *expected, "{provider} {event} {payload}");
    }
}

/// The order a live Codex seat sent on 2026-10-19 while it asked the owner one question.
#[test]
fn a_codex_question_waits_through_its_sleeps_until_the_reply() {
    use serde_json::json;
    let state = |event: &str, payload: serde_json::Value| {
        hook_state("codex", event, &payload).map(|s| (s.state, s.kind))
    };
    let ask = json!({"tool_name": "request_user_input_async"});
    let sleep = json!({"tool_name": "clocksleep"});
    assert_eq!(
        state("PreToolUse", ask.clone()),
        Some(("waiting", Some("question")))
    );
    assert_eq!(
        state("PostToolUse", ask),
        Some(("waiting", Some("question")))
    );
    assert_eq!(state("PreToolUse", sleep.clone()), None);
    assert_eq!(state("PostToolUse", sleep), None);
    let reply = json!({"prompt": "<send_user_message_question_reply>\n[]"});
    assert_eq!(state("UserPromptSubmit", reply), Some(("working", None)));
    assert_eq!(
        state("PreToolUse", json!({"tool_name": "Bash"})),
        Some(("working", None))
    );
}
