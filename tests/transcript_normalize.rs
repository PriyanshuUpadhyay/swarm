//! Local transcript behaviors through the public Rust snapshot reader.

use serde_json::{Value, json};
use swarm::view::transcript::source::NativeFormat;
mod common;

fn snapshot(format: NativeFormat, lines: &[Value]) -> Value {
    serde_json::to_value(common::snapshot(
        format,
        "local-behaviors",
        "chair.jsonl",
        lines,
    ))
    .unwrap()
}

#[test]
fn empty_claude_thinking_keeps_a_thought_row() {
    let result = snapshot(
        NativeFormat::Claude,
        &[json!({
            "type":"assistant", "message":{"content":[{"type":"thinking","thinking":""}]}
        })],
    );
    assert_eq!(
        result["lines"][0]["events"][0]["type"],
        "agent_thought_chunk"
    );
    assert_eq!(result["rows"][0]["kind"], "thought");
    assert_eq!(result["rows"][0]["text"], "");
}

#[test]
fn codex_reasoning_without_a_summary_keeps_a_thought_row() {
    for payload in [
        json!({"type":"reasoning"}),
        json!({"type":"reasoning","summary":null}),
        json!({"type":"reasoning","summary":[]}),
        json!({"type":"reasoning","summary":[{"text":""}]}),
    ] {
        let result = snapshot(
            NativeFormat::Codex,
            &[json!({"type":"response_item","payload":payload})],
        );
        assert_eq!(
            result["lines"][0]["events"][0]["type"],
            "agent_thought_chunk"
        );
        assert_eq!(result["rows"][0]["kind"], "thought");
        assert_eq!(result["rows"][0]["text"], "");
    }
}

#[test]
fn empty_pi_thinking_keeps_a_thought_before_its_tool() {
    let result = snapshot(
        NativeFormat::Pi,
        &[json!({
            "type":"message", "id":"a2", "message":{"role":"assistant", "content":[
                {"type":"thinking","thinking":""},
                {"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"ls"}}
            ],"stopReason":"toolUse"}
        })],
    );
    assert_eq!(
        result["lines"][0]["events"][0]["type"],
        "agent_thought_chunk"
    );
    assert_eq!(result["lines"][0]["events"][0]["text"], "");
    assert_eq!(result["lines"][0]["events"][1]["type"], "tool_call");
}

#[test]
fn codex_review_mode_keeps_visible_system_rows() {
    let result = snapshot(
        NativeFormat::Codex,
        &[
            json!({"type":"event_msg","payload":{"type":"entered_review_mode","user_facing_hint":"current changes"}}),
            json!({"type":"event_msg","payload":{"type":"exited_review_mode"}}),
        ],
    );
    assert_eq!(result["rows"].as_array().unwrap().len(), 2);
    for (row, text) in result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .zip(["Entered review mode: current changes", "Exited review mode"])
    {
        assert_eq!(row["kind"], "system");
        assert_eq!(row["system_kind"], "review_mode");
        assert_eq!(row["text"], text);
        assert_ne!(row["collapsed"], true);
    }
}

#[test]
fn pi_bookkeeping_is_ignored_instead_of_unmapped() {
    for kind in [
        "session_state",
        "agent_status",
        "git_state",
        "child_usage_attribution",
        "service_tier_change",
    ] {
        let result = snapshot(NativeFormat::Pi, &[json!({"type":kind,"id":"s1"})]);
        assert_eq!(result["lines"][0]["events"][0]["type"], "ignored");
        assert_eq!(result["lines"][0]["events"][0]["kind"], kind);
        assert_eq!(result["rows"][0]["system_kind"], "record");
    }
}

#[test]
fn claude_task_result_links_later_updates_to_the_created_task() {
    let result = snapshot(
        NativeFormat::Claude,
        &[
            json!({"type":"assistant","uuid":"a1","message":{"content":[{"type":"tool_use","id":"create-1","name":"TaskCreate","input":{"subject":"Write tests"}}]}}),
            json!({"type":"user","uuid":"r1","message":{"content":[{"type":"tool_result","tool_use_id":"create-1","content":"Task created"}]},"toolUseResult":{"task":{"id":"7","subject":"Write tests"}}}),
            json!({"type":"assistant","uuid":"a2","message":{"content":[{"type":"tool_use","id":"update-1","name":"TaskUpdate","input":{"taskId":"7","status":"completed"}}]}}),
        ],
    );
    assert_eq!(result["lines"][1]["events"][0]["task_id"], "7");
    let latest = result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|row| row["kind"] == "todoList")
        .unwrap();
    assert_eq!(latest["todo"][0]["text"], "Write tests");
    assert_eq!(latest["todo"][0]["status"], "completed");
}

#[test]
fn codex_completed_plan_becomes_a_proposed_plan_row() {
    let result = snapshot(
        NativeFormat::Codex,
        &[json!({
            "type":"event_msg", "payload":{"type":"item_completed","item":{"type":"Plan","id":"plan-1","text":"# Plan\n\n1. Add the row"}}
        })],
    );
    assert_eq!(result["lines"][0]["events"][0]["type"], "proposed_plan");
    assert_eq!(result["rows"][0]["kind"], "proposedPlan");
    assert_eq!(result["rows"][0]["text"], "# Plan\n\n1. Add the row");
}

#[test]
fn codex_hidden_output_copies_keep_their_source_kind() {
    use common::{completed, event, response};
    let call = response(
        json!({"type":"function_call","call_id":"read","name":"exec_command","arguments":"{}"}),
    );
    let output = response(json!({"type":"function_call_output","call_id":"read","output":"done"}));
    let copy = completed(json!({"type":"FunctionCallOutput","id":"read","output":"done"}));

    let orders = [
        (
            output.clone(),
            copy.clone(),
            "item_completed/FunctionCallOutput",
        ),
        (copy, output, "response_item/function_call_output"),
    ];

    for (first, second, kind) in orders {
        let lines = [
            event(json!({"type":"task_started"})),
            call.clone(),
            first,
            second,
        ];

        let result = snapshot(NativeFormat::Codex, &lines);

        assert_eq!(result["lines"][3]["events"][0]["type"], "ignored");
        assert_eq!(result["lines"][3]["events"][0]["kind"], kind);
    }
}
