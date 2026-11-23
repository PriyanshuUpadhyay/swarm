// Tests for turns and folds.

use super::*;

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
        rows.iter().map(|row| row.collapsed).collect::<Vec<_>>(),
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
    assert!(!message.collapsed);
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
            .collapsed
    );
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
#[test]
fn ring_rows() {
    let idle = build(vec![system("swarm_ring", RING, "ring-1", "")]);
    assert_eq!(kinds(&idle), [RowKind::System]);
    assert_eq!(idle[0].id, "ring-1:ring");
    assert!(idle[0].starts_turn);
    assert!(!idle[0].collapsed);
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
/// Rows of real log lines, with `fold_rows` applied as the reader does.
fn folded(format: &str, lines: &[&str]) -> Vec<Row> {
    let codex = format == "codex";
    let rows = transcript::rows(&records(inline_fixture(format, lines), codex), false, codex);
    view::fold_rows(rows, &HashSet::new())
}
#[test]
fn claude_groups_reads_and_searches_with_their_thinking() {
    // Claude Code 2.1.329 draws Read, Grep, and read-only Bash in one `collapsed_read_search` row.
    const LINES: [&str; 4] = [
        r#"{"type":"assistant","uuid":"a1","sessionId":"s","message":{"content":[{"type":"tool_use","id":"read","name":"Read","input":{"file_path":"/w/a.rs"}}]}}"#,
        r#"{"type":"assistant","uuid":"a2","sessionId":"s","message":{"content":[{"type":"thinking","thinking":"Check the callers."},{"type":"tool_use","id":"grep","name":"Bash","input":{"command":"rg -n fold src | head"}}]}}"#,
        r#"{"type":"assistant","uuid":"a3","sessionId":"s","message":{"content":[{"type":"tool_use","id":"make","name":"Bash","input":{"command":"make"}}]}}"#,
        r#"{"type":"assistant","uuid":"a4","sessionId":"s","message":{"content":[{"type":"text","text":"Done."}]}}"#,
    ];
    let rows = folded("claude", &LINES);
    assert_eq!(
        kinds(&rows),
        [RowKind::Fold, RowKind::ToolUse, RowKind::Assistant]
    );
    assert_eq!(
        kinds(&rows[0].steps),
        [RowKind::ToolUse, RowKind::Thought, RowKind::ToolUse]
    );
    assert_eq!(rows[0].text, "Searching for 1 pattern, reading 1 file");
}
#[test]
fn codex_groups_only_its_exploring_commands() {
    // Codex 0.169.0 puts a command in its "Explored" cell when every `parsed_cmd` part reads, lists, or searches.
    const LINES: [&str; 2] = [
        r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"id":"exec-1","type":"CommandExecution","command":["/bin/zsh","-lc","cat a.rs"],"cwd":"/w","status":"completed","aggregated_output":"fn a() {}","parsed_cmd":[{"type":"read","cmd":"cat a.rs","name":"a.rs","path":"a.rs"}]}}}"#,
        r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"id":"exec-2","type":"CommandExecution","command":["/bin/zsh","-lc","make"],"cwd":"/w","status":"completed","aggregated_output":"ok","parsed_cmd":[{"type":"unknown","cmd":"make"}]}}}"#,
    ];
    let rows: Vec<_> = folded("codex", &LINES)
        .into_iter()
        .filter(|row| !row.collapsed)
        .collect();
    assert_eq!(kinds(&rows), [RowKind::Fold, RowKind::ToolUse]);
    assert_eq!(rows[0].text, "Explored");
    assert_eq!(rows[0].steps.len(), 1);
}
#[test]
fn pi_never_groups_tool_calls() {
    // pi draws each tool call and each thought as its own block.
    const LINES: [&str; 2] = [
        r#"{"type":"message","id":"a1","parentId":null,"timestamp":"t","message":{"role":"assistant","content":[{"type":"toolCall","id":"t1","name":"bash","arguments":{"command":"cat a.rs"}},{"type":"thinking","thinking":"Now the release."}]}}"#,
        r#"{"type":"message","id":"a2","parentId":"a1","timestamp":"t","message":{"role":"assistant","content":[{"type":"toolCall","id":"t2","name":"read","arguments":{"path":"b.rs"}}]}}"#,
    ];
    let rows = folded("pi", &LINES);
    assert!(rows.iter().all(|row| row.kind != RowKind::Fold));
    assert_eq!(
        kinds(&rows),
        [RowKind::ToolUse, RowKind::Thought, RowKind::ToolUse]
    );
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
                assert!(row.steps.iter().filter(|row| !row.collapsed).count() >= 2);
                assert!(row.steps.iter().any(|row| row.tool.is_some()));
            }
        }
    }
}
