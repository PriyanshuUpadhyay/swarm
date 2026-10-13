use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    process::Command,
};
use swarm::view::transcript::{self as transcript, Record, Row, RowKind};
use swarm::view::{self, DiffPreview, TextPreview, ToolState};

fn records(events: Vec<Value>, codex: bool) -> Vec<Record> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| Record::new(event, format!("raw-{index}"), codex))
        .collect()
}
fn build(events: Vec<Value>) -> Vec<Row> {
    transcript::rows(&records(events, false), false, false)
}
fn chunk(kind: &str, text: &str, uuid: &str) -> Value {
    json!({"type": kind, "text":text, "meta":{"uuid":uuid}})
}
fn user(text: &str, uuid: &str) -> Value {
    chunk("user_message_chunk", text, uuid)
}
fn assistant(text: &str, uuid: &str) -> Value {
    chunk("agent_message_chunk", text, uuid)
}
fn system(kind: &str, text: &str, uuid: &str, parent: &str) -> Value {
    json!({"type":"system_message", "kind":kind, "text":text, "meta":{"uuid":uuid,"parent_uuid":parent}})
}
fn call(id: &str, name: &str, input: Value) -> Value {
    json!({"type":"tool_call","tool_call_id":id,"name":name,"input":input,"status":"pending"})
}
fn update(id: &str, status: &str, output: &str) -> Value {
    json!({"type":"tool_call_update","tool_call_id":id,"status":status,"content":output})
}
fn ended(reason: &str, millis: Option<i64>) -> Value {
    json!({"type":"turn_ended","reason":reason,"duration_ms":millis})
}
fn diff(id: &str, path: &str) -> Value {
    json!({"type":"tool_diff","tool_call_id":id,"path":path,"hunks":[{"old_start":1,"old_lines":1,"new_start":1,"new_lines":1,"lines":["-old","+new"]}]})
}
fn tool(row: &Row) -> &view::ToolActivity {
    row.tool.as_ref().unwrap()
}
fn kinds(rows: &[Row]) -> Vec<RowKind> {
    rows.iter().map(|row| row.kind).collect()
}
fn texts(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.text.as_str()).collect()
}
fn outputs(rows: &[Row]) -> Vec<&str> {
    rows.iter()
        .filter_map(|row| {
            row.tool
                .as_ref()?
                .output
                .as_ref()
                .map(|preview| preview.text.as_str())
        })
        .collect()
}
fn shell_input(command: &str, uuid: &str) -> Value {
    system(
        "shell_input",
        &format!("<bash-input>{command}</bash-input>"),
        uuid,
        "",
    )
}
fn shell_output(output: &str, parent: &str) -> Value {
    system("shell_output", output, "", parent)
}
fn fixture(format: &str, name: &str) -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(root.join("packages/transcript/zig-out/bin/transcript"))
        .args(["--format", format])
        .arg(root.join("packages/transcript/src/fixtures").join(name))
        .output()
        .unwrap();
    assert!(output.status.success());
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

#[test]
fn labels() {
    assert_eq!(
        Row::new(RowKind::User, "".into(), "1".into()).label(Some("claude")),
        "You"
    );
    assert_eq!(
        Row::new(RowKind::Assistant, "".into(), "2".into()).label(Some("claude")),
        "Claude"
    );
    assert_eq!(
        Row::new(RowKind::Assistant, "".into(), "3".into()).label(None),
        "Chair"
    );
    assert_eq!(
        Row::new(RowKind::ToolUse, "".into(), "4".into()).label(None),
        "Tool"
    );
}
#[test]
fn agent_chunks_fold() {
    let rows = build(vec![
        assistant("Hello ", "turn-1"),
        assistant("world!", "turn-1"),
    ]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text, "Hello world!");
    assert_eq!(rows[0].id, "turn-1");
    assert_eq!(rows[0].source_ids, ["raw-0", "raw-1"]);
}
#[test]
fn distinct_chunks_stay_apart() {
    assert_eq!(
        texts(&build(vec![assistant("A", "one"), assistant("B", "two")])),
        ["A", "B"]
    );
}
#[test]
fn question_label() {
    let rows = build(vec![
        json!({"type":"elicitation","tool_call_id":"ask-1","questions":[{"question":"Which color?"}]}),
    ]);
    assert_eq!(rows[0].label(Some("claude")), "Question");
    assert_eq!(texts(&rows), ["Which color?"]);
}
#[test]
fn action_rows() {
    let rows = build(vec![
        call("call-1", "ls", json!({})),
        update("call-1", "completed", "one\ntwo"),
        json!({"type":"elicitation","tool_call_id":"ask-1","questions":[{"question":"Proceed?"}]}),
        json!({"type":"error","message":"failed","meta":{"uuid":"event-1"}}),
    ]);
    assert_eq!(
        kinds(&rows),
        [RowKind::ToolUse, RowKind::Question, RowKind::Error]
    );
    assert_eq!(rows[0].id, "call-1:call");
    assert_eq!(tool(&rows[0]).output.as_ref().unwrap().text, "one\ntwo");
    assert_eq!(tool(&rows[0]).state, ToolState::Finished);
}
#[test]
fn tool_summary() {
    let rows = build(vec![
        call(
            "one",
            "exec",
            json!({"description":"List files","command":"ls\npwd"}),
        ),
        call("two", "exec", json!({"command":"pwd\nls"})),
    ]);
    assert_eq!(texts(&rows), ["exec · List files", "exec · pwd"]);
    assert!(rows[0].detail.as_ref().unwrap().contains("\"command\""));
}
#[test]
fn provider_command_labels() {
    let rows = build(vec![
        call("codex", "exec", json!({"cmd":"git status"})),
        call("agy", "exec", json!({"CommandLine":"pwd"})),
        call(
            "file",
            "view_file",
            json!({"TargetFile":"/work/source.swift"}),
        ),
        call(
            "summary",
            "view_file",
            json!({"toolSummary":"Read source","AbsolutePath":"/work/source.swift"}),
        ),
    ]);
    assert_eq!(
        texts(&rows),
        [
            "exec · git status",
            "exec · pwd",
            "view_file · source.swift",
            "view_file · Read source"
        ]
    );
}
#[test]
fn hidden_rows() {
    let rows = build(vec![
        json!({"type":"session_info","kind":"title","value":"Chat"}),
        json!({"type":"hook_result","kind":"hook_success","hook_name":"check"}),
        system("system", "  ", "", ""),
        system("system", "Ready", "", ""),
    ]);
    assert_eq!(
        rows.iter().map(|row| row.hidden).collect::<Vec<_>>(),
        [true, true, true, false]
    );
}
#[test]
fn task_notification() {
    let rows = build(vec![
        system(
            "task_notification",
            "<task-notification>\n<task-id>b4</task-id>\n<status>completed</status>\n<summary>Background command \"Wait 20 seconds\" completed (exit code 0)</summary>\n</task-notification>",
            "",
            "",
        ),
        system(
            "queued_command",
            "<task-notification>\n<status>failed</status>\n</task-notification>",
            "",
            "",
        ),
        system(
            "task_notification",
            "<task-notification></task-notification>",
            "",
            "",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::Notice; 3]);
    assert_eq!(
        texts(&rows),
        [
            "Background command \"Wait 20 seconds\" completed (exit code 0)",
            "Background task failed",
            "Background task finished"
        ]
    );
}
#[test]
fn task_notification_starts_turn() {
    let rows = build(vec![
        user("Run it in the background", "u1"),
        ended("completed", Some(1)),
        system(
            "task_notification",
            " <task-notification><summary>done</summary></task-notification>",
            "",
            "",
        ),
        call("t1", "Bash", json!({})),
    ]);
    assert!(transcript::is_active(&rows));
    assert!(!transcript::is_active(&rows[..2]));
}
#[test]
fn interleaved_calls() {
    let rows = build(vec![
        call("a", "exec", json!({"command":"first"})),
        call("b", "exec", json!({"command":"second"})),
        update("b", "completed", "B result"),
        update("a", "completed", "A result"),
    ]);
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["a:call", "b:call"]
    );
    assert_eq!(outputs(&rows), ["A result", "B result"]);
    assert_eq!(
        rows.iter()
            .map(|row| tool(row).command.as_deref())
            .collect::<Vec<_>>(),
        [Some("first"), Some("second")]
    );
}
#[test]
fn snapshots_and_diffs() {
    let rows = build(vec![
        update("a", "pending", "partial"),
        diff("a", "/work/first.txt"),
        call("a", "edit", json!({"file_path":"/work/first.txt"})),
        assistant("I edited it.", "answer"),
        update("a", "completed", "final output"),
        diff("a", "/work/second.txt"),
    ]);
    assert_eq!(kinds(&rows), [RowKind::ToolUse, RowKind::Assistant]);
    assert_eq!(tool(&rows[0]).output.as_ref().unwrap().text, "final output");
    assert_eq!(
        tool(&rows[0])
            .diffs
            .iter()
            .map(|diff| diff.path.as_str())
            .collect::<Vec<_>>(),
        ["/work/first.txt", "/work/second.txt"]
    );
    assert_eq!(tool(&rows[0]).path.as_deref(), Some("/work/first.txt"));
    assert_eq!(rows[1].text, "I edited it.");
}
#[test]
fn missing_call_and_page_boundary() {
    let rows = build(vec![
        call("a", "edit", Value::Null),
        json!({"type":"page","start_offset":0,"end_offset":10}),
        update("a", "completed", "page result"),
        diff("a", "/work/page.txt"),
    ]);
    assert_eq!(
        kinds(&rows),
        [RowKind::ToolUse, RowKind::ToolResult, RowKind::Diff]
    );
    assert_eq!(tool(&rows[0]).state, ToolState::Waiting);
    assert!(tool(&rows[0]).output.is_none());
    assert_eq!(rows[1].text, "page result");
    assert_eq!(rows[2].diff.as_ref().unwrap().path, "/work/page.txt");
}
#[test]
fn ambiguous_ids() {
    let rows = build(vec![
        call("dup", "first", Value::Null),
        call("dup", "second", Value::Null),
        update("dup", "completed", "ambiguous"),
        call("", "third", Value::Null),
        update("", "completed", "missing ID"),
    ]);
    assert_eq!(
        kinds(&rows),
        [
            RowKind::ToolUse,
            RowKind::ToolUse,
            RowKind::ToolResult,
            RowKind::ToolUse,
            RowKind::ToolResult
        ]
    );
    assert_eq!(
        rows.iter().map(|row| &row.id).collect::<HashSet<_>>().len(),
        rows.len()
    );
    assert!(tool(&rows[0]).output.is_none() && tool(&rows[1]).output.is_none());
    assert_eq!(rows[2].text, "ambiguous");
    assert_eq!(rows[4].text, "missing ID");
    assert_eq!(rows[2].tool_status.as_deref(), Some("completed"));
}
fn in_session(mut event: Value, session: &str) -> Value {
    event["meta"] = json!({"session_id":session});
    event
}
#[test]
fn blank_session_is_ambiguous() {
    let rows = build(vec![
        in_session(call("same", "one", Value::Null), "one"),
        in_session(call("same", "two", Value::Null), "two"),
        update("same", "failed", "unknown owner"),
        diff("same", "/work/ambiguous.txt"),
    ]);
    assert_eq!(
        kinds(&rows),
        [
            RowKind::ToolUse,
            RowKind::ToolUse,
            RowKind::ToolResult,
            RowKind::Diff
        ]
    );
    assert!(tool(&rows[0]).diffs.is_empty());
    assert!(tool(&rows[1]).diffs.is_empty());
    assert_eq!(rows[2].text, "unknown owner");
}
#[test]
fn scope_boundaries() {
    let rows = build(vec![
        in_session(call("same", "first", Value::Null), "one"),
        in_session(update("same", "completed", "one"), "one"),
        in_session(call("same", "second", Value::Null), "two"),
        in_session(update("same", "failed", "two"), "two"),
        ended("completed", None),
        in_session(call("same", "third", Value::Null), "two"),
        in_session(update("same", "completed", "three"), "two"),
    ]);
    assert_eq!(outputs(&rows), ["one", "two", "three"]);
    assert_eq!(
        rows.iter()
            .filter_map(|row| row.tool.as_ref().map(|tool| tool.state))
            .collect::<Vec<_>>(),
        [ToolState::Finished, ToolState::Failed, ToolState::Finished]
    );
    assert_eq!(
        rows.iter().map(|row| &row.id).collect::<HashSet<_>>().len(),
        rows.len()
    );
}
#[test]
fn end_states() {
    let rows = build(vec![
        call("a", "a", Value::Null),
        update("a", "failed", "error text"),
        call("b", "b", Value::Null),
        ended("aborted", None),
        call("c", "c", Value::Null),
        ended("completed", Some(41000)),
        call("d", "d", Value::Null),
    ]);
    assert_eq!(
        rows.iter()
            .filter_map(|row| row.tool.as_ref().map(|tool| tool.state))
            .collect::<Vec<_>>(),
        [
            ToolState::Failed,
            ToolState::Interrupted,
            ToolState::Unreported,
            ToolState::Waiting
        ]
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.ends_turn)
            .map(|row| row.detail.as_deref())
            .collect::<Vec<_>>(),
        [None, Some("41s")]
    );
}
#[test]
fn shell_pair() {
    let rows = build(vec![
        shell_input("ls", "input"),
        shell_output(
            "<bash-stdout>a &amp; b</bash-stdout><bash-stderr></bash-stderr>",
            "input",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::Shell]);
    assert_eq!(rows[0].id, "input:shell");
    assert_eq!(
        rows[0].shell.as_ref().unwrap().command.as_deref(),
        Some("ls")
    );
    assert_eq!(rows[0].shell.as_ref().unwrap().output.text, "a & b");
    assert_eq!(rows[0].text, "$ ls\na & b");
}
#[test]
fn unmatched_shell() {
    let lone = build(vec![shell_input("pwd", "input")]);
    let orphan = build(vec![shell_output(
        "<bash-stdout>/work</bash-stdout>",
        "outside-window",
    )]);
    assert_eq!(
        lone[0].shell.as_ref().unwrap().command.as_deref(),
        Some("pwd")
    );
    assert_eq!(lone[0].shell.as_ref().unwrap().output.text, "");
    assert!(orphan[0].shell.as_ref().unwrap().command.is_none());
    assert_eq!(orphan[0].shell.as_ref().unwrap().output.text, "/work");
    assert!(lone[0].starts_turn && orphan[0].starts_turn);
}
#[test]
fn duplicate_shell_inputs() {
    let rows = build(vec![
        shell_input("first", "same"),
        shell_input("second", "same"),
        shell_output("<bash-stdout>which one</bash-stdout>", "same"),
    ]);
    assert_eq!(kinds(&rows), [RowKind::Shell; 3]);
    assert_eq!(
        rows.iter()
            .map(|row| row.shell.as_ref().unwrap().command.as_deref())
            .collect::<Vec<_>>(),
        [Some("first"), Some("second"), None]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.shell.as_ref().unwrap().output.text.as_str())
            .collect::<Vec<_>>(),
        ["", "", "which one"]
    );
}
#[test]
fn cut_command_has_no_chip() {
    let rows = build(vec![
        system("command", "<command-name>/flo", "command", ""),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "body",
            "command",
        ),
    ]);
    assert!(rows.iter().all(|row| row.command.is_none()));
    assert_eq!(
        texts(&rows),
        [
            "<command-name>/flo",
            "Base directory for this skill: /skills/flow"
        ]
    );
}
#[test]
fn skill_body_to_command() {
    let command = "<command-message>flow</command-message>\n<command-name>/flow</command-name>\n<command-args>start\nthe chat</command-args>";
    let rows = build(vec![
        system("command", command, "command", ""),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "body",
            "command",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::System]);
    assert_eq!(rows[0].text, command);
    assert_eq!(rows[0].system_kind.as_deref(), Some("command"));
    let chip = rows[0].command.as_ref().unwrap();
    assert_eq!(chip.name, "/flow");
    assert_eq!(chip.arguments, "start\nthe chat");
    assert_eq!(
        chip.skill_body.as_ref().unwrap().text,
        "Base directory for this skill: /skills/flow"
    );
    assert!(rows[0].starts_turn);
}
fn skill_body(kind: &str, text: &str, call_id: &str) -> Value {
    let mut event = system(kind, text, "body", "tool-result");
    event["meta"]["source_tool_use_id"] = json!(call_id);
    event
}
#[test]
fn skill_body_to_tool() {
    let rows = build(vec![
        call("skill-call", "Skill", json!({"skill":"flow"})),
        update("skill-call", "completed", "Launching skill: flow"),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "skill-call",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::ToolUse]);
    assert_eq!(
        tool(&rows[0]).skill_body.as_ref().unwrap().text,
        "Base directory for this skill: /skills/flow"
    );
    let twice = build(vec![
        call("skill-call", "Skill", json!({"skill":"flow"})),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "skill-call",
        ),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/flow/more",
            "skill-call",
        ),
    ]);
    assert_eq!(
        tool(&twice[0]).skill_body.as_ref().unwrap().text,
        "Base directory for this skill: /skills/flow\n\nBase directory for this skill: /skills/flow/more"
    );
}
#[test]
fn bundled_skill_body_to_tool() {
    let rows = build(vec![
        call("skill-call", "Skill", json!({"skill":"simplify"})),
        skill_body("injected", "Review target: the changes", "skill-call"),
        call("bash-call", "Bash", json!({})),
        skill_body("injected", "Hook note", "bash-call"),
    ]);
    assert_eq!(
        kinds(&rows),
        [RowKind::ToolUse, RowKind::ToolUse, RowKind::System]
    );
    assert_eq!(
        tool(&rows[0]).skill_body.as_ref().unwrap().text,
        "Review target: the changes"
    );
    assert!(tool(&rows[1]).skill_body.is_none());
    assert!(rows[2].hidden);
}
#[test]
fn unlinked_skill_body() {
    let rows = build(vec![system(
        "skill_body",
        "Base directory for this skill: /skills/flow",
        "body",
        "outside-window",
    )]);
    assert_eq!(kinds(&rows), [RowKind::System]);
    assert_eq!(rows[0].system_kind.as_deref(), Some("skill_body"));
    assert!(!rows[0].hidden);
}
#[test]
fn model_command_output() {
    let rows = build(vec![
        system(
            "command",
            "<command-name>/model</command-name>\n<command-args>sonnet</command-args>",
            "command",
            "",
        ),
        system(
            "command_output",
            "<local-command-stdout>Set model to \u{1b}[1msonnet\u{1b}[22m</local-command-stdout>",
            "stdout",
            "command",
        ),
    ]);
    assert_eq!(rows.len(), 1);
    let chip = rows[0].command.as_ref().unwrap();
    assert_eq!(chip.name, "/model");
    assert_eq!(chip.arguments, "sonnet");
    assert_eq!(chip.output.as_ref().unwrap().text, "Set model to sonnet");
    assert!(!rows[0].starts_turn);
    assert!(!transcript::is_active(&rows));
}
#[test]
fn turn_state_for_injected_rows() {
    for extension in [
        vec![
            shell_input("git status", "input"),
            shell_output("<bash-stdout>clean</bash-stdout>", "input"),
        ],
        vec![
            system(
                "command",
                "<command-name>/flow</command-name>",
                "command",
                "",
            ),
            system(
                "skill_body",
                "Base directory for this skill: /skills/flow",
                "body",
                "command",
            ),
        ],
    ] {
        let mut events = vec![user("Fix the bug", "prompt"), ended("completed", Some(1))];
        events.extend(extension);
        assert!(transcript::is_active(&build(events.clone())));
        events.push(assistant("The tree is clean.", "reply"));
        assert!(transcript::is_active(&build(events.clone())));
        events.push(system(
            "interrupted",
            "[Request interrupted by user]",
            "stop",
            "",
        ));
        let rows = build(events);
        assert!(!transcript::is_active(&rows));
        assert_eq!(rows.last().unwrap().text, "Interrupted");
    }
}
#[test]
fn injected_rows() {
    let rows = build(vec![
        user("Fix the bug", "prompt"),
        system(
            "injected",
            "<system-reminder>Be brief</system-reminder>",
            "prompt",
            "",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::User, RowKind::System]);
    assert_eq!(
        rows.iter().map(|row| row.hidden).collect::<Vec<_>>(),
        [false, true]
    );
}
#[test]
fn peer_message_starts_turn() {
    let rows = build(vec![
        user("Fix the bug", "prompt"),
        ended("completed", Some(1)),
        system(
            "peer_message",
            "<teammate-message teammate_id=\"lead\">\nReview the parser\n</teammate-message>",
            "teammate",
            "",
        ),
    ]);
    let message = rows.last().unwrap();
    assert_eq!(message.kind, RowKind::Notice);
    assert_eq!(message.text, "Review the parser");
    assert!(!message.hidden);
    assert!(transcript::is_active(&rows));
    let peer = "Another Claude session sent a message:\n<agent-message>Done</agent-message>";
    assert_eq!(
        texts(&build(vec![system("peer_message", peer, "peer", "")])),
        [peer]
    );
    assert!(
        !build(vec![system(
            "peer_message",
            "<teammate-message teammate_id=\"lead\">model: use opus</teammate-message>",
            "model-word",
            ""
        )])[0]
            .hidden
    );
}
#[test]
fn bundled_skill_body_to_command() {
    let rows = build(vec![
        user("Fix the bug", "prompt"),
        ended("completed", Some(1)),
        system(
            "command",
            "<command-name>/simplify</command-name>",
            "command",
            "",
        ),
        system("injected", "Review target: the changes", "body", "command"),
    ]);
    assert_eq!(
        kinds(&rows),
        [RowKind::User, RowKind::Result, RowKind::System]
    );
    assert_eq!(
        rows[2]
            .command
            .as_ref()
            .unwrap()
            .skill_body
            .as_ref()
            .unwrap()
            .text,
        "Review target: the changes"
    );
    assert!(transcript::is_active(&rows));
}
#[test]
fn reminder_under_command_stays_hidden() {
    for reminder in [
        "\n<system-reminder>Be brief</system-reminder>",
        "<local-command-caveat>Caveat: do not respond</local-command-caveat>",
    ] {
        let rows = build(vec![
            system(
                "command",
                "<command-name>/simplify</command-name>",
                "command",
                "",
            ),
            system("injected", reminder, "reminder", "command"),
        ]);
        assert_eq!(kinds(&rows), [RowKind::System; 2]);
        assert!(rows[0].command.as_ref().unwrap().skill_body.is_none());
        assert!(!rows[0].starts_turn);
        assert!(rows[1].hidden);
    }
}
#[test]
fn two_skill_bodies_join() {
    let rows = build(vec![
        system(
            "command",
            "<command-name>/flow</command-name>",
            "command",
            "",
        ),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "first-body",
            "command",
        ),
        system(
            "injected",
            "Review target: the changes",
            "second-body",
            "command",
        ),
    ]);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .command
            .as_ref()
            .unwrap()
            .skill_body
            .as_ref()
            .unwrap()
            .text,
        "Base directory for this skill: /skills/flow\n\nReview target: the changes"
    );
}
#[test]
fn search_text_holds_joined_records() {
    let rows = build(vec![
        system(
            "command",
            "<command-name>/model</command-name>",
            "model-command",
            "",
        ),
        system(
            "command_output",
            "<local-command-stdout>Set model to sonnet</local-command-stdout>",
            "stdout",
            "model-command",
        ),
        system(
            "command",
            "<command-name>/flow</command-name>",
            "flow-command",
            "",
        ),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "command-body",
            "flow-command",
        ),
        call("skill-call", "Skill", json!({"skill":"review"})),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/review",
            "skill-call",
        ),
    ]);
    assert_eq!(rows.len(), 3);
    assert!(rows[0].search_text().contains("Set model to sonnet"));
    assert!(rows[1].search_text().contains("/skills/flow"));
    assert!(rows[2].search_text().contains("/skills/review"));
}
#[test]
fn chair_turn() {
    let question = Row::new(RowKind::User, "hi".into(), "question".into());
    let answer = Row::new(RowKind::Assistant, "hello".into(), "answer".into());
    let mut ended = Row::new(RowKind::Result, "completed".into(), "ended".into());
    ended.ends_turn = true;
    assert!(!transcript::is_active(&[]));
    assert!(transcript::is_active(&[question.clone(), answer.clone()]));
    assert!(!transcript::is_active(&[
        question.clone(),
        answer.clone(),
        ended.clone()
    ]));
    assert!(transcript::is_active(&[
        question.clone(),
        answer,
        ended,
        question
    ]));
}
const RING: &str =
    "swarm: new message. Run swarm inbox and read each body at /home/owner/.swarm/<body_path>.";
#[test]
fn ring_rows() {
    let idle = build(vec![system("swarm_ring", RING, "ring-1", "")]);
    assert_eq!(kinds(&idle), [RowKind::System]);
    assert_eq!(idle[0].id, "ring-1:ring");
    assert!(idle[0].starts_turn);
    assert!(!idle[0].hidden);
    assert_eq!(idle[0].search_text(), "New swarm message");
    let rows = build(vec![
        user("Run the council", "prompt"),
        system("swarm_ring", RING, "ring-2", ""),
        ended("completed", Some(1000)),
        system("swarm_ring", RING, "ring-3", ""),
    ]);
    assert_eq!(
        rows.iter().map(|row| row.starts_turn).collect::<Vec<_>>(),
        [false, false, false, true]
    );
    assert!(transcript::is_active(&rows));
    assert!(!transcript::is_active(&rows[..3]));
}
#[test]
fn window_starts_mid_turn() {
    for codex in [false, true] {
        let tail = vec![
            call("c1", "exec", json!("ls")),
            update("c1", "completed", "ok"),
            system("swarm_ring", RING, "ring-1", ""),
            call("c2", "exec", json!("swarm inbox")),
            update("c2", "completed", "ok"),
            assistant("Done.", "answer"),
            ended("completed", Some(1000)),
            system("swarm_ring", RING, "ring-2", ""),
        ];
        let rows = transcript::rows(&records(tail, codex), true, codex);
        assert_eq!(
            rows.iter()
                .filter(|row| row.system_kind.as_deref() == Some("swarm_ring"))
                .map(|row| row.starts_turn)
                .collect::<Vec<_>>(),
            [false, true]
        );
        let folded = view::fold_rows(rows, &HashSet::new());
        assert_eq!(
            folded[0]
                .steps
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["c1:call", "ring-1:ring", "c2:call"]
        );
    }
    let idle = transcript::rows(
        &records(
            vec![
                system("swarm_ring", RING, "ring-3", ""),
                user("Run the council", "prompt"),
            ],
            false,
        ),
        true,
        false,
    );
    assert!(idle[0].starts_turn);
}
#[test]
fn codex_log_without_turn_starts() {
    let events = vec![
        user("Fix the build", "prompt"),
        call("c1", "exec", json!("make")),
        update("c1", "completed", "ok"),
        assistant("Done.", "answer"),
        ended("aborted", Some(1000)),
        user("Try again", "retry"),
        system("swarm_ring", RING, "ring", ""),
    ];
    let rows = transcript::rows(&records(events, true), false, true);
    assert!(
        rows.iter()
            .filter(|row| row.kind == RowKind::User)
            .all(|row| !row.arrives_mid_turn)
    );
    assert!(!rows.last().unwrap().starts_turn);
}
#[test]
fn failure_from_output_only_in_codex() {
    let events = vec![
        call("c1", "Bash", json!({"command":"cat run.log"})),
        update(
            "c1",
            "completed",
            "Script failed\nWall time 1 seconds\nOutput:\nboom",
        ),
    ];
    assert_eq!(tool(&build(events.clone())[0]).state, ToolState::Finished);
    assert_eq!(
        tool(&transcript::rows(&records(events, true), false, true)[0]).state,
        ToolState::Failed
    );
}
#[test]
fn codex_exit_code_only_in_codex() {
    for output in [
        "Script completed\nWall time 1 seconds\nOutput:\n{\"exit_code\":1}",
        "Chunk ID: b2\nWall time: 0.0 seconds\nProcess exited with code 1\nOutput:\nboom",
    ] {
        let events = vec![
            call("c1", "Bash", json!({"command":"cat run.log"})),
            update("c1", "completed", output),
        ];
        assert!(tool(&build(events.clone())[0]).exit_code.is_none());
        assert_eq!(
            tool(&transcript::rows(&records(events, true), false, true)[0]).exit_code,
            Some(1)
        );
    }
    assert_eq!(
        tool(
            &build(vec![
                call("c1", "Bash", json!({"command":"false"})),
                update("c1", "failed", "Exit code 1")
            ])[0]
        )
        .exit_code,
        Some(1)
    );
}
#[test]
fn ring_keeps_tool_scope() {
    let rows = build(vec![
        user("Run the council", "prompt"),
        call("c1", "exec", json!("ls")),
        system("swarm_ring", RING, "ring", ""),
        update("c1", "completed", "ok"),
    ]);
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["prompt", "c1:call", "ring:ring"]
    );
    assert_eq!(tool(&rows[1]).output.as_ref().unwrap().text, "ok");
}
#[test]
fn row_sources() {
    let rows = build(vec![
        assistant("Hel", "a1"),
        assistant("lo", "a1"),
        call("c1", "Bash", json!({"command":"ls"})),
        json!({"type":"ignored","kind":"usage"}),
        update("c1", "completed", "ok"),
        shell_input("pwd", "in"),
        shell_output("<bash-stdout>/w</bash-stdout>", "in"),
    ]);
    assert_eq!(
        rows.iter()
            .map(|row| row.source_ids.clone())
            .collect::<Vec<_>>(),
        [
            vec!["raw-0", "raw-1"],
            vec!["raw-2", "raw-4"],
            vec!["raw-5", "raw-6"]
        ]
    );
}
#[test]
fn provider_edit_results() {
    for (format, name) in [
        ("codex", "codex-exec-edit.jsonl"),
        ("agy", "agy-edit.jsonl"),
    ] {
        let events = fixture(format, name);
        assert_eq!(events.len(), 2);
        let content = events[1]["content"].as_str().unwrap();
        let rows = build(events.clone());
        assert_eq!(kinds(&rows), [RowKind::ToolUse]);
        assert_eq!(
            rows[0].id,
            format!("{}:call", events[0]["tool_call_id"].as_str().unwrap())
        );
        assert_eq!(tool(&rows[0]).output.as_ref().unwrap().text, content);
        assert_eq!(tool(&rows[0]).input, events[0]["input"]);
        assert_eq!(tool(&rows[0]).state, ToolState::Finished);
        assert!(tool(&rows[0]).diffs.is_empty());
    }
}
#[test]
fn saved_edit() {
    let events = fixture("claude", "claude-edit.jsonl");
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["meta"]["uuid"], "fixture-result");
    let rows = build(events);
    assert_eq!(kinds(&rows), [RowKind::ToolResult, RowKind::Diff]);
    assert_eq!(
        rows[0].text,
        "The file /workspace/example.txt has been updated successfully."
    );
    assert_eq!(rows[1].text, "example.txt · +1 −0");
    let preview = rows[1].diff.as_ref().unwrap();
    assert!(preview.patch.contains("@@ -445,6 +445,7 @@\n"));
    assert!(preview.patch.contains("+added line\n"));
    assert_eq!(preview.omitted_lines, 0);
}
#[test]
fn bounded_preview() {
    let path = "/workspace/quote\"\n<script>.swift";
    let long_line = format!("+{}", "λ".repeat(900));
    let mut lines = vec![long_line.clone()];
    lines.extend(vec!["+new".to_owned(); 204]);
    let event = json!({"tool_call_id":"edit-2","path":path,"hunks":[{"old_start":0,"old_lines":0,"new_start":1,"new_lines":205,"lines":lines}]});
    let diff: view::Diff = serde_json::from_value(event).unwrap();
    let preview = DiffPreview::new(&diff);
    assert_eq!(preview.omitted_lines, 5);
    assert_eq!(preview.shortened_lines, 1);
    assert!(preview.patch.contains("@@ -0,0 +1,200 @@"));
    assert!(preview.patch.contains("quote\\\"\\n<script>.swift"));
    assert_eq!(diff.path, path);
    assert_eq!(diff.hunks[0].lines, lines);
    let full = view::diff_patch(&diff, true).0;
    assert!(full.contains("@@ -0,0 +1,205 @@"));
    assert!(full.contains(&long_line));
    assert_eq!(full.lines().filter(|line| *line == "+new").count(), 204);
}
#[test]
fn unknown_diff() {
    let raw = json!({"type":"tool_diff","tool_call_id":"x","path":"a","hunks":"invalid"});
    let rows = build(vec![raw.clone()]);
    assert_eq!(kinds(&rows), [RowKind::Notice]);
    assert_eq!(rows[0].text, raw.to_string());
}
#[test]
fn tool_label() {
    let rows = build(vec![call(
        "edit",
        "Edit",
        json!({"file_path":"/workspace/Sources/main.swift"}),
    )]);
    assert_eq!(rows[0].text, "Edit · main.swift");
    assert_eq!(
        serde_json::from_str::<Value>(rows[0].detail.as_ref().unwrap()).unwrap(),
        json!({"file_path":"/workspace/Sources/main.swift"})
    );
}

#[test]
fn preserves_small_output() {
    let text = "first\n\n  indented λ\n";
    let preview = TextPreview::new(text, 120);
    assert_eq!(preview.text, text);
    assert!(!preview.is_truncated);
    assert!(!TextPreview::new("", 120).is_truncated);
}
#[test]
fn bounds_long_and_multiline_output() {
    let long_line = "👩🏽‍💻".repeat(12001);
    let preview = TextPreview::new(&long_line, 120);
    assert_eq!(
        preview.text,
        long_line.chars().take(12000).collect::<String>()
    );
    assert!(preview.is_truncated);
    let many_lines = (0..121)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let preview = TextPreview::new(&many_lines, 120);
    assert!(preview.text.ends_with("line 119"));
    assert!(preview.is_truncated);
}
#[test]
fn shell_output_folds_at_fifty_lines() {
    let fifty = (0..50)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!TextPreview::new(&fifty, 50).is_truncated);
    let preview = TextPreview::new(&(fifty.clone() + "\nline 50"), 50);
    assert_eq!(preview.text, fifty);
    assert!(preview.is_truncated);
}
#[test]
fn tool_output_folds_at_five_lines() {
    let preview = TextPreview::new("one\ntwo\nthree\nfour\nfive\nsix", 5);
    assert_eq!(preview.text, "one\ntwo\nthree\nfour\nfive");
    assert_eq!(preview.hidden_lines, 1);
    assert!(!TextPreview::new("one\ntwo\nthree\nfour\nfive", 5).is_truncated);
    assert_eq!(TextPreview::new(&"x".repeat(12001), 5).hidden_lines, 0);
}
#[test]
fn trailing_newline_ends_the_last_line() {
    let text = "a\nb\nc\nd\ne\n";
    let preview = TextPreview::new(text, 5);
    assert_eq!(preview.text, text);
    assert!(!preview.is_truncated);
    assert_eq!(preview.full_lines, 5);
    assert_eq!(preview.hidden_lines, 0);
    assert_eq!(TextPreview::new("\n", 120).full_lines, 1);
}
#[test]
fn character_cap_counts_every_line_it_hides() {
    let source = "x".repeat(20000)
        + (0..9)
            .map(|index| format!("\nline {index}"))
            .collect::<String>()
            .as_str();
    let preview = TextPreview::new(&source, 5);
    assert_eq!(preview.text, "x".repeat(12000));
    assert_eq!(preview.hidden_lines, 9);
    let source = vec!["w".repeat(290); 60].join("\n");
    assert_eq!(TextPreview::new(&source, 50).hidden_lines, 18);
}
#[test]
fn windows_line_ends_count_and_fold_as_lines() {
    let preview = TextPreview::new("one\r\ntwo\r\nthree", 2);
    assert_eq!(preview.text, "one\r\ntwo\r");
    assert!(preview.is_truncated);
    assert_eq!(preview.full_lines, 3);
    assert_eq!(preview.hidden_lines, 1);
    assert_eq!(TextPreview::new("", 120).full_lines, 1);
}
#[test]
fn chunks_keep_ordinary_lines_together() {
    assert_eq!(
        view::text_chunks("first\nsecond\nthird\n", 10),
        ["first\n", "second\n", "third\n"]
    );
}
#[test]
fn chunks_keep_every_character_in_order() {
    let source = "αβγ\n\n".to_owned() + &"👩🏽‍💻".repeat(17) + "\nlast\n";
    let pieces = view::text_chunks(&source, 5);
    assert_eq!(pieces.join(""), source);
    assert!(pieces.iter().all(|piece| piece.chars().count() <= 5));
    assert!(view::text_chunks("", 5).is_empty());
}

#[test]
fn known_input_keys() {
    let command = "printf 'one\ntwo'";
    let path = "/work/Folder With Spaces/file.txt";
    for key in ["command", "cmd", "CommandLine"] {
        let mut input = json!({});
        input[key] = json!(command);
        assert_eq!(view::command(&input, "Bash").as_deref(), Some(command));
    }
    for key in ["file_path", "TargetFile", "AbsolutePath"] {
        let mut input = json!({});
        input[key] = json!(path);
        assert_eq!(view::path(&input).as_deref(), Some(path));
    }
}
#[test]
fn duration() {
    assert_eq!(
        view::duration("2026-09-29T10:00:00Z", "2026-09-29T10:00:12Z"),
        Some(12.0)
    );
    assert_eq!(
        view::duration("2026-09-29T10:00:00.250Z", "2026-09-29T10:00:00.750Z"),
        Some(0.5)
    );
    assert_eq!(view::duration("", "2026-09-29T10:00:12Z"), None);
    assert_eq!(
        view::duration("2026-09-29T10:00:12Z", "2026-09-29T10:00:00Z"),
        None
    );
    assert_eq!(view::duration_label(0.42), "0.4s");
    assert_eq!(view::duration_label(12.4), "12s");
    assert_eq!(view::duration_label(185.0), "3m 5s");
}
#[test]
fn raw_input() {
    assert_eq!(
        view::command(&json!("ls -la"), "exec_command").as_deref(),
        Some("ls -la")
    );
    assert_eq!(
        view::command(
            &json!({"code":"await tools.exec_command({cmd: 'ls'})"}),
            "functions.exec"
        ),
        None
    );
    assert_eq!(view::command(&json!("some text"), "write_file"), None);
}
#[test]
fn header_title() {
    assert_eq!(
        view::title(
            &json!({"description":"Build the UI package","command":"swift build"}),
            Some("swift build"),
            None
        ),
        "Build the UI package"
    );
    assert_eq!(
        view::title(
            &json!({}),
            Some("\n  swift build \\\n  --package-path ui"),
            None
        ),
        "swift build \\"
    );
    assert_eq!(
        view::title(&json!({}), Some(&"x".repeat(5000)), None)
            .chars()
            .count(),
        200
    );
    for (input, expected) in [
        (
            json!({"offset":300,"limit":61}),
            "SessionDetail.swift · lines 300–360",
        ),
        (json!({"limit":40}), "SessionDetail.swift · lines 1–40"),
        (json!({"offset":12}), "SessionDetail.swift · from line 12"),
        (json!({}), "SessionDetail.swift"),
    ] {
        assert_eq!(
            view::title(&input, None, Some("/work/ui/SessionDetail.swift")),
            expected
        );
    }
    assert_eq!(
        view::title(&json!({"skill":"research"}), None, None),
        "research"
    );
    assert_eq!(
        view::title(
            &json!({"pattern":"ShellRecord","path":"/work/ui/Sources"}),
            None,
            None
        ),
        "\"ShellRecord\" in Sources"
    );
    assert_eq!(
        view::title(&json!({"pattern":"**/*.swift"}), None, None),
        "\"**/*.swift\""
    );
    assert_eq!(view::title(&Value::Null, None, None), "");
}
#[test]
fn read_range_edges() {
    for (input, expected) in [
        (
            json!({"offset":2,"limit":i64::MAX}),
            "a.swift · from line 2".into(),
        ),
        (
            json!({"offset":1,"limit":i64::MAX}),
            format!("a.swift · lines 1–{}", i64::MAX),
        ),
        (json!({"limit":0}), "a.swift · from line 1".into()),
        (
            json!({"offset":5,"limit":i64::MIN}),
            "a.swift · from line 5".into(),
        ),
    ] {
        assert_eq!(view::title(&input, None, Some("/work/a.swift")), expected);
    }
}
#[test]
fn row_text_uses_header_title() {
    assert_eq!(
        texts(&build(vec![call(
            "grep",
            "Grep",
            json!({"pattern":"ShellRecord","path":"/work/ui/Sources"})
        )])),
        ["Grep · \"ShellRecord\" in Sources"]
    );
}
#[test]
fn diff_counts() {
    let diff: view::Diff=serde_json::from_value(json!({"tool_call_id":"edit-1","path":"/work/a.swift","hunks":[{"old_start":1,"old_lines":2,"new_start":1,"new_lines":3,"lines":[" keep","-old","+new","+more"]},{"old_start":9,"old_lines":1,"new_start":10,"new_lines":0,"lines":["-gone"]}]})).unwrap();
    let preview = DiffPreview::new(&diff);
    assert_eq!(preview.added * 2, 4);
    assert_eq!(preview.removed * 2, 4);
    assert!(
        tool(&build(vec![call("read", "Read", Value::Null)])[0])
            .diffs
            .is_empty()
    );
}
#[test]
fn exit_code() {
    assert_eq!(
        view::exit_code(Some("Exit code 1\nusage: swarm"), Some("false"), false),
        Some(1)
    );
    assert_eq!(
        view::exit_code(Some("Exit code 127"), Some("false"), false),
        Some(127)
    );
    assert_eq!(
        view::exit_code(Some("build ok\nExit code 1"), Some("false"), false),
        None
    );
    assert_eq!(view::exit_code(Some("Exit code 1"), None, false), None);
    assert_eq!(view::exit_code(None, Some("false"), false), None);
}
#[test]
fn codex_exec_command() {
    let inbox = r#"text(await tools.exec_command({cmd:"swarm inbox","sandbox_permissions":"require_escalated","max_output_tokens":1000}));"#;
    for (script, expected) in [
        (inbox, "swarm inbox"),
        (
            r#"text(await tools.exec_command({"cmd":"swarm ack 7"}));"#,
            "swarm ack 7",
        ),
        (
            r#"text(await tools.exec_command({cmd:"python3 - <<'PY'\nprint(\"hi\")\nPY"}));"#,
            "python3 - <<'PY'\nprint(\"hi\")\nPY",
        ),
        (
            "const r = await Promise.allSettled([\ntools.exec_command({cmd:\"swarm roles get council.claude\"}),\ntools.exec_command({cmd:\"swarm roles get council.gpt\"}),\n]);",
            "swarm roles get council.claude\nswarm roles get council.gpt",
        ),
        (
            r#"await tools.apply_patch("*** Begin Patch\n*** End Patch"); text(await tools.exec_command({cmd:"ls"}));"#,
            "apply_patch(…)\nls",
        ),
        (
            r#"text(await tools.exec_command({cmd:"rg 'tools.read(' src"}));"#,
            "rg 'tools.read(' src",
        ),
        (
            r#"text(await tools.exec_command({cmd:"ls"})); text("see tools.read( first"); const note = 'tools.write('; const tip = `tools.view(`;"#,
            "ls",
        ),
        (
            r#"const dir = "src"; await tools.exec_command({cmd: `ls ${dir}`}); await tools.exec_command({cmd: dir}); await tools.exec_command({cmd:"rm -rf " + dir}); text(await tools.exec_command({cmd:"pwd"}));"#,
            "exec_command(…)\nexec_command(…)\nexec_command(…)\npwd",
        ),
    ] {
        assert_eq!(
            view::command(&json!(script), "exec").as_deref(),
            Some(expected)
        );
    }
    for script in [
        r#"text(await tools.apply_patch("*** Begin Patch\n*** End Patch"));"#,
        r#"text(await tools.exec_command({cmd:"swarm inb"#,
    ] {
        assert_eq!(
            view::command(&json!(script), "exec").as_deref(),
            Some(script)
        );
    }
    assert_eq!(
        texts(&build(vec![call("c1", "exec", json!(inbox))])),
        ["exec · swarm inbox"]
    );
}
#[test]
fn codex_exec_failure() {
    for (output, code, failed) in [
        (
            "Script completed\nWall time 3.6 seconds\nOutput:\n{\"chunk_id\":\"a\",\"exit_code\":1,\"output\":\"Traceback\\n\"}",
            Some(1),
            true,
        ),
        (
            "Script completed\nWall time 3.1 seconds\nOutput:\n{\"exit_code\":0,\"output\":\"7 claude reply\\n\"}",
            None,
            false,
        ),
        (
            "Script completed\nWall time 1 seconds\nOutput:\n{\"exit_code\":0}\n{\"exit_code\":2}",
            Some(2),
            true,
        ),
        (
            "Script completed\nWall time 1 seconds\nOutput:\n{\"exit_code\":0,\"output\":\"{\\\"exit_code\\\":1}\"}",
            None,
            false,
        ),
        (
            "Script failed\nWall time 10.8 seconds\nOutput:\nScript error:\nexec_command failed",
            None,
            true,
        ),
        (
            "Script running with cell ID 7\nWall time 31.0 seconds\nOutput:\n{\"exit_code\":3}",
            Some(3),
            true,
        ),
        (
            "Script started, output log file is 'typescript'.\nOutput:\n{\"exit_code\":2}",
            None,
            false,
        ),
        (
            "Script completed\nWall time 1 seconds\nOutput:\n[{\"status\":\"fulfilled\",\"value\":{\"exit_code\":0}},{\"status\":\"rejected\",\"reason\":\"exec_command failed: ProcessFailed\"}]",
            None,
            true,
        ),
        (
            "Script completed\nOutput:\n{\"output\":\"{\\\"status\\\":\\\"rejected\\\"}\"}",
            None,
            false,
        ),
    ] {
        assert_eq!(view::exit_code(Some(output), Some("x"), true), code);
        assert_eq!(view::reports_failure(Some(output), Some("x"), true), failed);
    }
}
#[test]
fn codex_printed_text_is_not_a_result() {
    for output in [
        "Script completed\nWall time 1 seconds\nOutput:\nserver log {\"exit_code\":1} retried\n",
        "Script completed\nWall time 1 seconds\nOutput:\nbuild ok\n{\"exit_code\":1}\n",
        "Script completed\nWall time 1 seconds\nOutput:\nqueue item [{\"status\":\"rejected\"}] skipped\n",
    ] {
        assert_eq!(view::exit_code(Some(output), Some("x"), true), None);
        assert!(!view::reports_failure(Some(output), Some("x"), true));
    }
}
#[test]
fn script_failed_needs_code_mode_shape() {
    assert!(!view::reports_failure(
        Some("Script failed: lint skipped"),
        None,
        false
    ));
    assert!(!view::reports_failure(
        Some("Script failed: lint skipped"),
        Some("make lint"),
        false
    ));
}
#[test]
fn codex_process_exit() {
    let header = "Chunk ID: b294b6\nWall time: 0.0000 seconds\nProcess exited with code ";
    let failed = format!("{header}1\nOriginal token count: 3\nOutput:\nboom\n");
    assert_eq!(view::exit_code(Some(&failed), Some("false"), true), Some(1));
    assert!(view::reports_failure(Some(&failed), Some("false"), true));
    assert_eq!(
        view::exit_code(
            Some(&(header.to_owned() + "0\nOriginal token count: 3\nOutput:\nok\n")),
            Some("false"),
            true
        ),
        None
    );
    assert_eq!(
        view::exit_code(
            Some(
                "Chunk ID: c1\nWall time: 1.0 seconds\nProcess running with session ID 7\nOutput:\nProcess exited with code 1\n"
            ),
            Some("false"),
            true
        ),
        None
    );
}

#[test]
fn folds_keep_hidden_steps_and_open_failed_runs() {
    let rows = build(vec![
        user("Fix the build", "prompt"),
        system("injected", "reminder", "hidden", ""),
        call("read", "Read", json!({})),
        call("make", "Bash", json!({"command":"make"})),
        update("make", "failed", "Exit code 1"),
        assistant("I found the cause.", "answer"),
    ]);
    let folded = view::fold_rows(rows.clone(), &HashSet::new());
    assert_eq!(folded[1].id, "fold:read:call");
    assert!(folded[1].default_open);
    assert_eq!(folded[1].steps.len(), 3);
    assert_eq!(folded[1].fold.as_ref().unwrap().failed, 1);
    let flattened = folded
        .iter()
        .flat_map(|row| {
            if row.kind == RowKind::Fold {
                row.steps.clone()
            } else {
                vec![row.clone()]
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(flattened, rows);
    assert!(!view::fold_expanded(
        &folded[1].steps,
        &HashMap::from([("fold:read:call".into(), false)]),
        &HashSet::new()
    ));
}
#[test]
fn fold_choice_survives_prepend_and_live_append() {
    let rows = build(vec![
        call("read", "Read", json!({})),
        call("grep", "Read", json!({})),
        call("make", "Read", json!({})),
    ]);
    let mut overrides = HashMap::from([("fold:grep:call".into(), true)]);
    assert!(view::fold_expanded(&rows, &overrides, &HashSet::new()));
    overrides.insert("fold:read:call".into(), false);
    assert!(!view::fold_expanded(&rows, &overrides, &HashSet::new()));
    assert_eq!(
        view::fold_rows(rows[..2].to_vec(), &HashSet::new())[0].id,
        view::fold_rows(rows, &HashSet::new())[0].id
    );
}
#[test]
fn seen_rows_keep_a_fold_open() {
    let rows = build(vec![
        call("read", "Read", json!({})),
        call("grep", "Read", json!({})),
    ]);
    assert!(view::fold_expanded(
        &rows,
        &HashMap::new(),
        &HashSet::from(["read:call".into(), "grep:call".into()])
    ));
    assert!(!view::fold_expanded(
        &rows,
        &HashMap::new(),
        &HashSet::from(["read:call".into()])
    ));
}
#[test]
fn fold_summary_counts_states_and_names() {
    let mut rows = build(vec![
        user("prompt", "prompt"),
        call("c1", "exec", json!({"command":"swarm inbox"})),
        call("c2", "Bash", json!({"command":"false"})),
        update("c1", "completed", "done"),
        update("c2", "failed", "Exit code 1"),
        call("w1", "sleep", json!({})),
        call("read", "Read", json!({})),
        call("read-2", "Read", json!({})),
        call("grep", "Grep", json!({})),
        system("swarm_ring", RING, "r1", ""),
        system("swarm_ring", RING, "r2", ""),
    ]);
    for row in &mut rows {
        if let Some(tool) = &mut row.tool {
            if tool.state == ToolState::Waiting {
                tool.state = ToolState::Finished;
            }
            tool.duration_seconds = Some(match tool.name.as_str() {
                "exec" => 3.0,
                "Bash" => 2.0,
                "sleep" => 15.0,
                "Read" => 0.5,
                _ => 1.0,
            });
        }
    }
    let summary = view::fold_summary(&rows);
    assert_eq!(
        summary.text,
        "2 commands · 1 wait · Read ×2 · Grep · 2 rings · 1 failed"
    );
    assert_eq!(summary.duration_seconds, Some(22.0));
    assert_eq!(summary.state, ToolState::Failed);
}
#[test]
fn running_summary_names_waiting_tool() {
    let rows = build(vec![
        user("prompt", "prompt"),
        call("inbox", "exec", json!({"command":"swarm inbox"})),
        system("swarm_ring", RING, "ring", ""),
        call("read", "Read", json!({})),
        update("read", "completed", "done"),
    ]);
    let summary = view::fold_summary(&rows);
    assert!(summary.is_running);
    assert_eq!(summary.latest_title, "swarm inbox");
    assert_eq!(summary.text, "1 command · Read · 1 ring");
}
#[test]
fn fold_is_lossless() {
    let mut makers = build(vec![
        assistant("reply", "reply"),
        call("tool", "Read", json!({})),
        call("failed", "Read", json!({})),
        update("failed", "failed", "error"),
        call("waiting", "Read", json!({})),
        system("swarm_ring", RING, "mid", ""),
        system("swarm_ring", RING, "start", ""),
        chunk("agent_thought_chunk", "plan", "thought"),
        system("injected", "reminder", "hidden", ""),
    ]);
    makers[4].starts_turn = false;
    makers[5].starts_turn = true;
    for length in 0..=6 {
        for mut number in 0..8usize.pow(length) {
            let mut rows = Vec::new();
            for index in 0..length {
                let mut row = makers[number % 8].clone();
                row.id = format!("r{index}");
                rows.push(row);
                number /= 8;
            }
            let folded = view::fold_rows(rows.clone(), &HashSet::new());
            let flat = folded
                .iter()
                .flat_map(|row| {
                    if row.kind == RowKind::Fold {
                        row.steps.clone()
                    } else {
                        vec![row.clone()]
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(flat, rows);
            for row in folded.iter().filter(|row| row.kind == RowKind::Fold) {
                assert!(row.steps.iter().filter(|row| !row.hidden).count() >= 2);
                assert!(row.steps.iter().any(|row| row.tool.is_some()));
            }
        }
    }
}
#[test]
fn queue_replay_keeps_owner_messages_and_sent_rows() {
    let queue = |operation: &str, content: Option<&str>| json!({"type":"queue_operation","operation":operation,"content":content});
    let events = vec![
        queue("enqueue", Some("<task-notification>done")),
        queue("enqueue", Some("keep the old name")),
        queue("dequeue", None),
        system("queued_prompt", "already sent", "q1", ""),
        queue("enqueue", Some("second")),
        queue("remove", Some("second")),
    ];
    let records = records(events, false);
    let queued = transcript::queued(&records);
    assert_eq!(texts(&queued), ["keep the old name"]);
    let rows = transcript::rows(&records, false, false);
    assert_eq!(kinds(&rows), [RowKind::User]);
    assert!(rows[0].arrives_mid_turn);
}
#[test]
fn clear_boundary_removes_only_preamble() {
    let records = records(
        vec![
            system(
                "command",
                "<command-name>/clear</command-name>",
                "clear",
                "",
            ),
            system(
                "injected",
                "<local-command-caveat>Caveat</local-command-caveat>",
                "caveat",
                "",
            ),
            system(
                "command_output",
                "<local-command-stdout></local-command-stdout>",
                "stdout",
                "clear",
            ),
            user("new prompt", "prompt"),
            system(
                "command",
                "<command-name>/clear</command-name>",
                "later",
                "",
            ),
        ],
        false,
    );
    let mut rows = transcript::rows(&records, false, false);
    transcript::clear_boundary(&records, &mut rows, true);
    assert_eq!(
        kinds(&rows),
        [RowKind::Divider, RowKind::User, RowKind::System]
    );
    assert_eq!(rows[0].text, "Context cleared");
}
#[test]
fn usage_resets_after_model_change_and_compaction() {
    let report = json!({"type":"usage","kind":"context","context_tokens":20,"context_capacity_tokens":100,"meta":{"timestamp":"now"}});
    let events = vec![
        json!({"type":"session_info","kind":"model","value":"first"}),
        report.clone(),
    ];
    let usage = transcript::usage(&records(events.clone(), false)).unwrap();
    assert_eq!(usage.remaining_percent, Some(80));
    assert_eq!(usage.context_timestamp, "now");
    let mut changed = events.clone();
    changed.push(json!({"type":"session_info","kind":"model","value":"second"}));
    assert!(transcript::usage(&records(changed, false)).is_none());
    let mut compacted = events;
    compacted.push(system("compaction", "summary", "", ""));
    assert!(transcript::usage(&records(compacted, false)).is_none());
}
#[test]
fn previews_bound_wire_bytes_and_keep_full_counts() {
    let huge = (0..100000)
        .map(|index| format!("line {index}\n"))
        .collect::<String>();
    let records = records(
        vec![
            call("giant", "Bash", json!({"command":"print"})),
            update("giant", "completed", &huge),
        ],
        false,
    );
    assert!(records[1].event["content"].as_str().unwrap().len() < 12000);
    let rows = transcript::rows(&records, false, false);
    let preview = tool(&rows[0]).output.as_ref().unwrap();
    assert_eq!(preview.full_lines, 100000);
    assert_eq!(preview.full_bytes, huge.len());
    assert!(serde_json::to_vec(&rows).unwrap().len() < 2500);
    let wire = serde_json::to_value(&rows).unwrap();
    assert!(wire[0].get("hidden").is_none());
    assert!(wire[0].get("steps").is_none());
    for field in [
        "diffs",
        "path",
        "duration_seconds",
        "exit_code",
        "skill_body",
    ] {
        assert!(wire[0]["tool"].get(field).is_none(), "{field}");
    }
    assert_eq!(wire[0]["tool"]["output"]["is_truncated"], true);
    let small = TextPreview::new("done", 120);
    assert!(
        serde_json::to_value(small)
            .unwrap()
            .get("is_truncated")
            .is_none()
    );
    let plain = Row::new(RowKind::User, "hello".into(), "user".into());
    assert_eq!(
        serde_json::to_value(plain).unwrap(),
        json!({"id":"user","kind":"user","text":"hello","order":[0,0]})
    );
}

fn inline_fixture(lines: &[&str]) -> Vec<Value> {
    let path =
        std::env::temp_dir().join(format!("swarm-rows-fixture-{}.jsonl", uuid::Uuid::now_v7()));
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let output = Command::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/transcript/zig-out/bin/transcript"),
    )
    .args(["--format", "claude"])
    .arg(&path)
    .output();
    std::fs::remove_file(path).unwrap();
    let output = output.unwrap();
    assert!(output.status.success());
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

#[test]
fn bookkeeping_records_have_no_rows() {
    const LINES: [&str; 6] = [
        r#"{"type":"attachment","uuid":"u1","sessionId":"s1","attachment":{"type":"credential_org","organizationUuid":"org-1"}}"#,
        r#"{"type":"worktree-state","worktreeSession":{"originalCwd":"/work/main","worktreePath":"/work/wt","worktreeName":"wt","worktreeBranch":"wt","sessionId":"s1","enteredExisting":true},"sessionId":"s1"}"#,
        r#"{"type":"relocated","sessionId":"s1","relocatedCwd":"/work/wt"}"#,
        r#"{"type":"agent-name","agentName":"council-claude","sessionId":"s1"}"#,
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"add a test"}"#,
        r#"{"type":"queue-operation","operation":"dequeue","timestamp":"t2","sessionId":"s1"}"#,
    ];
    let events = inline_fixture(&LINES);
    assert_eq!(events.len(), 6);
    assert!(
        events
            .iter()
            .rev()
            .take(2)
            .all(|event| event["type"] == "queue_operation")
    );
    assert!(build(events).is_empty());
}

#[test]
fn absorbed_queued_message() {
    const LINES: [&str; 6] = [
        r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"Fix the bug"}}"#,
        r#"{"type":"assistant","uuid":"a1","sessionId":"s1","message":{"role":"assistant","model":"m","content":[{"type":"text","text":"On it"}]}}"#,
        r#"{"type":"attachment","uuid":"q1","sessionId":"s1","attachment":{"type":"queued_command","prompt":"keep the old name","origin":{"kind":"human"}}}"#,
        r#"{"type":"attachment","uuid":"q2","sessionId":"s1","attachment":{"type":"queued_command","prompt":"<task-notification>done</task-notification>","origin":{"kind":"task-notification"}}}"#,
        r#"{"type":"assistant","uuid":"a2","sessionId":"s1","message":{"role":"assistant","model":"m","content":[{"type":"text","text":"Kept it"}]}}"#,
        r#"{"type":"system","subtype":"turn_duration","durationMs":1200,"uuid":"d1","sessionId":"s1"}"#,
    ];
    let events = inline_fixture(&LINES);
    let rows: Vec<_> = build(events)
        .into_iter()
        .filter(|row| !row.hidden)
        .collect();
    assert_eq!(
        kinds(&rows),
        vec![
            RowKind::User,
            RowKind::Assistant,
            RowKind::User,
            RowKind::Notice,
            RowKind::Assistant,
            RowKind::Result
        ]
    );
    assert_eq!(rows[2].text, "keep the old name");
    assert!(transcript::is_active(&rows[..rows.len() - 1]));
    assert!(!transcript::is_active(&rows));
}

#[test]
fn call_id_stays_fixed_when_an_older_result_precedes_it() {
    let call = Record::new(
        call("ordered", "Read", json!({})),
        "raw-500-0".into(),
        false,
    );
    let output = Record::new(
        update("ordered", "completed", "done"),
        "raw-100-0".into(),
        false,
    );
    let initial = transcript::view_rows(std::slice::from_ref(&call), true, false, &HashSet::new());
    let joined = transcript::view_rows(&[output, call], true, false, &HashSet::new());
    assert_eq!(initial[0].id, joined[0].id);
    assert_eq!(joined[0].id, "ordered:call@raw-500-0");
    assert_eq!(tool(&joined[0]).state, ToolState::Finished);
}
