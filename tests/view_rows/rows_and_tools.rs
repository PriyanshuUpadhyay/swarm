// Tests for rows and tools.

use super::*;

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
        // Codex's setup prompt, AGENTS.md, and the cwd it repeats each turn.
        system("developer", "<permissions instructions>", "", ""),
        system("context", "<environment_context>", "", ""),
        json!({"type":"session_info","kind":"cwd","value":"/work"}),
    ]);
    assert_eq!(
        rows.iter().map(|row| row.collapsed).collect::<Vec<_>>(),
        [true, true, true, false, true, true, true]
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
            vec!["raw-3"],
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
