use super::*;

fn codex_rows(lines: Vec<Value>) -> Vec<Row> {
    let bytes = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let events = translated("codex", Path::new(""), bytes.as_bytes());
    transcript::rows(&records(events, true), false, true)
}

fn response(payload: Value) -> Value {
    json!({"type":"response_item", "payload":payload})
}

fn event(payload: Value) -> Value {
    json!({"type":"event_msg", "payload":payload})
}

fn completed(item: Value) -> Value {
    event(json!({"type":"item_completed", "item":item}))
}

fn apply_patch_call() -> Value {
    response(
        json!({"type":"custom_tool_call", "call_id":"patch", "name":"apply_patch", "input":"*** Begin Patch\n*** End Patch"}),
    )
}

#[test]
fn codex_file_changes_join_the_patch_call() {
    for source in ["patch_apply_end", "FileChange"] {
        let changes = json!({
            "new.txt":{"type":"add","content":"new\n"},
            "old.txt":{"type":"delete","content":"old\n"},
            "edit.txt":{"type":"update","unified_diff":"@@ -2,1 +2,1 @@\n-before\n+after\n","move_path":null}
        });
        let update = if source == "FileChange" {
            completed(
                json!({"type":source,"id":"patch","status":"completed","changes":changes,"stdout":"done"}),
            )
        } else {
            event(
                json!({"type":source,"call_id":"patch","success":true,"status":"completed","changes":changes,"stdout":"done"}),
            )
        };
        let rows = codex_rows(vec![apply_patch_call(), update]);
        assert_eq!(rows.len(), 1, "{source}");
        let tool = tool(&rows[0]);
        assert_eq!(tool.state, ToolState::Finished);
        assert_eq!(
            tool.diffs
                .iter()
                .map(|diff| (diff.path.as_str(), diff.added, diff.removed))
                .collect::<Vec<_>>(),
            [("new.txt", 1, 0), ("old.txt", 0, 1), ("edit.txt", 1, 1)]
        );
        assert!(tool.diffs[2].patch.contains("-before\n+after"));
    }
}

#[test]
fn codex_subagent_activity_updates_the_spawn_row() {
    for (source, kind, expected) in [
        ("sub_agent_activity", "started", ToolState::Waiting),
        ("SubAgentActivity", "interrupted", ToolState::Interrupted),
    ] {
        let activity = json!({"type":source,"id":"activity", "event_id":"activity", "agent_thread_id":"child", "agent_path":"/worker", "kind":kind});
        let activity = if source == "SubAgentActivity" {
            completed(activity)
        } else {
            event(activity)
        };
        let rows = codex_rows(vec![
            response(
                json!({"type":"function_call", "call_id":"spawn", "name":"spawn_agent", "arguments":"{\"message\":\"Check the parser\"}"}),
            ),
            response(
                json!({"type":"function_call_output", "call_id":"spawn", "output":"{\"agent_id\":\"child\"}"}),
            ),
            event(json!({"type":"task_complete"})),
            activity,
        ]);
        let visible: Vec<_> = rows.iter().filter(|row| !row.collapsed).collect();
        assert_eq!(visible.len(), 2, "{source}");
        let run = visible[0].subagent.as_ref().unwrap();
        assert_eq!(run.state, expected);
        assert_eq!(
            run.result.as_ref().unwrap().text,
            "{\"agent_id\":\"child\"}"
        );
    }
}

#[test]
fn codex_completed_items_keep_their_rows_and_command_output() {
    let rows = codex_rows(vec![
        response(
            json!({"type":"message","id":"u","role":"user","content":[{"type":"input_text","text":"Build it"}]}),
        ),
        completed(
            json!({"type":"UserMessage","id":"u","content":[{"type":"text","text":"Build it"}]}),
        ),
        response(
            json!({"type":"reasoning","id":"r","summary":[{"type":"summary_text","text":"Check the code"}]}),
        ),
        completed(
            json!({"type":"Reasoning","id":"r","summary_text":["Check the code"],"raw_content":[]}),
        ),
        response(
            json!({"type":"function_call","call_id":"cmd","name":"exec_command","arguments":"{\"cmd\":\"cargo check\"}"}),
        ),
        completed(
            json!({"type":"CommandExecution","id":"cmd","command":["cargo","check"],"status":"completed","aggregated_output":"Build passed","exit_code":0}),
        ),
        completed(
            json!({"type":"CommandExecution","id":"nested","command":["cargo","fmt"],"status":"failed","aggregated_output":"Needs format","exit_code":1}),
        ),
        response(
            json!({"type":"message","id":"a","role":"assistant","content":[{"type":"output_text","text":"Done"}]}),
        ),
        completed(
            json!({"type":"AgentMessage","id":"a","content":[{"type":"Text","text":"Done"}]}),
        ),
    ]);
    let visible: Vec<_> = rows.iter().filter(|row| !row.collapsed).cloned().collect();
    assert_eq!(
        kinds(&visible),
        [
            RowKind::User,
            RowKind::Thought,
            RowKind::ToolUse,
            RowKind::ToolUse,
            RowKind::Assistant
        ]
    );
    assert_eq!(outputs(&visible), ["Build passed", "Needs format"]);
    assert_eq!(tool(&visible[3]).state, ToolState::Failed);
    assert_eq!(tool(&visible[3]).exit_code, Some(1));
    assert!(
        !rows
            .iter()
            .any(|row| row.system_kind.as_deref() == Some("unmapped"))
    );
}

#[test]
fn codex_completed_notices_are_visible_and_unknown_items_are_counted() {
    let rows = codex_rows(vec![
        completed(
            json!({"type":"EnteredReviewMode","id":"review","user_facing_hint":"Review the patch"}),
        ),
        completed(json!({"type":"ExitedReviewMode","id":"end-review"})),
        completed(
            json!({"type":"HookPrompt","id":"hook","fragments":[{"text":"Hook feedback","hookRunId":"run"}]}),
        ),
        completed(json!({"type":"ImageView","id":"image","path":"/tmp/picture.png"})),
        completed(json!({"type":"FutureItem","id":"future"})),
        completed(json!({"type":"Extension","id":"extension"})),
    ]);
    let visible: Vec<_> = rows.iter().filter(|row| !row.collapsed).collect();
    assert_eq!(
        visible
            .iter()
            .map(|row| row.system_kind.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("review_mode"),
            Some("review_mode"),
            Some("hook_prompt"),
            None
        ]
    );
    assert!(visible[2].text.contains("Hook feedback"));
    assert!(visible[3].text.contains("/tmp/picture.png"));
    assert_eq!(
        rows.iter()
            .filter(|row| row.system_kind.as_deref() == Some("unmapped"))
            .count(),
        2
    );
}

#[test]
fn codex_image_generation_copies_make_one_image_row() {
    for reverse in [false, true] {
        let mut lines = vec![
            response(
                json!({"type":"image_generation_call","id":"image","status":"completed","result":"base64"}),
            ),
            event(
                json!({"type":"image_generation_end","call_id":"image","status":"completed","saved_path":"/tmp/image.png"}),
            ),
            completed(
                json!({"type":"ImageGeneration","id":"image","status":"completed","result":"base64","saved_path":"/tmp/image.png"}),
            ),
        ];
        if reverse {
            lines.reverse();
        }
        let rows = codex_rows(lines);
        assert_eq!(kinds(&rows), [RowKind::Notice]);
        assert!(!rows[0].collapsed);
    }
}

#[test]
fn codex_local_shell_calls_show_the_command_and_output() {
    let rows = codex_rows(vec![
        response(
            json!({"type":"local_shell_call","id":"shell-item","call_id":"shell","status":"in_progress","action":{"type":"exec","command":["ls","src"],"working_directory":"/work"}}),
        ),
        response(json!({"type":"function_call_output","call_id":"shell","output":"main.rs"})),
    ]);
    assert_eq!(kinds(&rows), [RowKind::ToolUse]);
    let tool = tool(&rows[0]);
    assert_eq!(tool.command.as_deref(), Some("ls src"));
    assert_eq!(tool.output.as_ref().unwrap().text, "main.rs");
    assert_eq!(tool.state, ToolState::Finished);
}

#[test]
fn codex_rollback_removes_the_last_turns_before_replay_continues() {
    let prompt = |id, text| {
        response(
            json!({"type":"message","id":id,"role":"user","content":[{"type":"input_text","text":text}]}),
        )
    };
    let rows = codex_rows(vec![
        event(json!({"type":"task_started"})),
        prompt("keep", "Keep this turn"),
        event(json!({"type":"task_complete"})),
        event(json!({"type":"task_started"})),
        prompt("gone1", "Remove this turn"),
        event(json!({"type":"task_complete"})),
        event(json!({"type":"task_started"})),
        prompt("gone2", "Remove this unfinished turn"),
        event(json!({"type":"thread_rolled_back","num_turns":2})),
        event(json!({"type":"thread_rolled_back","num_turns":0})),
        event(json!({"type":"task_started"})),
        prompt("new", "Continue here"),
    ]);
    let visible: Vec<_> = rows.iter().filter(|row| !row.collapsed).collect();
    assert_eq!(
        visible
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["Keep this turn", "completed", "Continue here"]
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.system_kind.as_deref() == Some("unmapped"))
    );
}

#[test]
fn codex_raw_reasoning_joins_the_thought_row() {
    let rows = codex_rows(vec![
        response(
            json!({"type":"reasoning","id":"thought","summary":[{"type":"summary_text","text":"Plan the fix"}]}),
        ),
        event(json!({"type":"agent_reasoning_raw_content","text":"Check the public reader first"})),
    ]);
    assert_eq!(kinds(&rows), [RowKind::Thought]);
    assert!(rows[0].text.contains("Plan the fix"));
    assert!(rows[0].text.contains("Check the public reader first"));
}

#[test]
fn codex_compaction_records_make_one_compaction_row() {
    for kind in ["compaction", "compaction_summary", "context_compaction"] {
        let rows = codex_rows(vec![
            response(json!({"type":kind,"id":"encrypted","encrypted_content":"ciphertext"})),
            completed(json!({"type":"ContextCompaction","id":"compaction"})),
            json!({"type":"compacted","payload":{"message":"Kept the useful context"}}),
            event(json!({"type":"context_compacted"})),
        ]);
        assert_eq!(rows.len(), 1, "{kind}");
        assert_eq!(rows[0].system_kind.as_deref(), Some("compaction"));
        assert_eq!(rows[0].text, "Kept the useful context");
        assert!(!rows[0].collapsed);
    }
}

#[test]
fn pi_custom_messages_follow_the_display_flag() {
    for display in [true, false] {
        let text = json!({"type":"custom_message","id":"custom","display":display,"content":"Extension message"}).to_string();
        let rows = build(inline_fixture("pi", &[&text]));
        assert_eq!(
            rows.iter()
                .any(|row| !row.collapsed && row.text == "Extension message"),
            display
        );
        assert!(
            !rows
                .iter()
                .any(|row| row.system_kind.as_deref() == Some("unmapped"))
        );
        let image = json!({"type":"custom_message","display":display,"content":[{"type":"image","mimeType":"image/png","data":"eA=="}]}).to_string();
        let rows = build(inline_fixture("pi", &[&image]));
        assert_eq!(
            rows.iter()
                .any(|row| row.kind == RowKind::Notice && !row.collapsed),
            display
        );
    }
}

#[test]
fn provider_context_stays_hidden_and_codex_hook_prompts_are_not_user_turns() {
    let claude = build(inline_fixture(
        "claude",
        &[
            r#"{"type":"attachment","attachment":{"type":"hook_additional_context","hookName":"SessionStart"}}"#,
            r#"{"type":"attachment","attachment":{"type":"sandbox_instructions"}}"#,
        ],
    ));
    assert!(
        claude
            .iter()
            .all(|row| row.collapsed && row.system_kind.as_deref() == Some("record"))
    );
    let rows = codex_rows(vec![
        response(
            json!({"type":"message","role":"system","content":[{"type":"input_text","text":"System setup"}]}),
        ),
        completed(
            json!({"type":"UserMessage","id":"setup","content":[{"type":"text","text":"<environment_context>Setup</environment_context>"}]}),
        ),
        response(
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":"<hook_prompt hook_run_id=\"run\">Hook feedback</hook_prompt>"}]}),
        ),
    ]);
    let visible: Vec<_> = rows.iter().filter(|row| !row.collapsed).collect();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].kind, RowKind::System);
    assert_eq!(visible[0].system_kind.as_deref(), Some("hook_prompt"));
    assert!(!visible[0].opens_turn());
}

#[test]
fn codex_bookkeeping_is_hidden_without_an_unmapped_count() {
    let mut lines: Vec<_> = [
        "inter_agent_communication",
        "security_risk_score",
        "dynamic_tools_updated",
        "realtime_item",
    ]
    .into_iter()
    .map(|kind| json!({"type":kind,"payload":{}}))
    .collect();
    lines.push(event(json!({"type":"thread_goal_updated"})));
    lines.push(response(json!({"type":"configuration_update"})));
    lines.push(json!({"type":"security_risk_score","payload":1}));
    lines.push(json!({"type":"realtime_item","payload":[{"type":"message"}]}));
    let rows = codex_rows(lines);
    assert!(
        rows.iter()
            .all(|row| row.collapsed && row.system_kind.as_deref() == Some("record"))
    );
}

#[test]
fn codex_completed_outputs_keep_question_rules() {
    for reverse in [false, true] {
        let call = response(
            json!({"type":"function_call","call_id":"ask","name":"request_user_input_async","arguments":"{\"questions\":[{\"title\":\"Continue?\",\"options\":[\"Yes\",\"No\"]}]}"}),
        );
        let mut copies = vec![
            response(
                json!({"type":"function_call_output","call_id":"ask","output":"{\"accepted\":true}"}),
            ),
            completed(
                json!({"type":"FunctionCallOutput","id":"ask","name":"request_user_input_async","output":"{\"accepted\":true}"}),
            ),
        ];
        if reverse {
            copies.reverse();
        }
        let rows = codex_rows(std::iter::once(call).chain(copies).collect());
        let visible: Vec<_> = rows.iter().filter(|row| !row.collapsed).collect();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].kind, RowKind::Question);
    }
}

#[test]
fn malformed_or_empty_provider_records_do_not_drop_silently() {
    for (format, line) in [
        ("claude", r#"{"type":"user"}"#),
        ("claude", r#"{"type":"assistant"}"#),
        ("claude", r#"{"type":"user","message":{"content":[17]}}"#),
        (
            "pi",
            r#"{"type":"message","message":{"role":"user","content":[]}}"#,
        ),
    ] {
        let events = inline_fixture(format, &[line]);
        assert!(!events.is_empty(), "{line}");
        let rows = build(events);
        assert!(
            rows.iter()
                .any(|row| row.system_kind.as_deref() == Some("unmapped")),
            "{line}"
        );
    }
}
