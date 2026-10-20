// Tests for codex and records.

use super::*;

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
    let user: Vec<_> = rows.iter().filter(|row| !row.collapsed).collect();
    assert_eq!(user.len(), 1);
    assert!(user[0].arrives_mid_turn);
    assert_eq!(
        rows.iter()
            .filter(|row| row.system_kind.as_deref() == Some("record"))
            .count(),
        5
    );
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
fn bookkeeping_records_are_collapsed_rows_with_their_source() {
    const LINES: [&str; 6] = [
        r#"{"type":"attachment","uuid":"u1","sessionId":"s1","attachment":{"type":"credential_org","organizationUuid":"org-1"}}"#,
        r#"{"type":"worktree-state","worktreeSession":{"originalCwd":"/work/main","worktreePath":"/work/wt","worktreeName":"wt","worktreeBranch":"wt","sessionId":"s1","enteredExisting":true},"sessionId":"s1"}"#,
        r#"{"type":"relocated","sessionId":"s1","relocatedCwd":"/work/wt"}"#,
        r#"{"type":"agent-name","agentName":"council-claude","sessionId":"s1"}"#,
        r#"{"type":"queue-operation","operation":"enqueue","timestamp":"t1","sessionId":"s1","content":"add a test"}"#,
        r#"{"type":"queue-operation","operation":"dequeue","timestamp":"t2","sessionId":"s1"}"#,
    ];
    let events = inline_fixture("claude", &LINES);
    assert_eq!(events.len(), 6);
    assert!(
        events
            .iter()
            .rev()
            .take(2)
            .all(|event| event["type"] == "queue_operation")
    );
    let rows = build(events);
    assert_eq!(rows.len(), 6);
    assert!(
        rows.iter()
            .all(|row| row.collapsed && row.system_kind.as_deref() == Some("record"))
    );
    assert_eq!(rows[0].text, "ignored · attachment/credential_org");
    assert!(
        rows[0]
            .output
            .as_ref()
            .unwrap()
            .text
            .contains("\"organizationUuid\": \"org-1\"")
    );
    assert_eq!(rows[4].text, "queue_operation · enqueue");
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
    let events = inline_fixture("claude", &LINES);
    let rows: Vec<_> = build(events)
        .into_iter()
        .filter(|row| !row.collapsed)
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
