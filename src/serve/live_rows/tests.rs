//! Tests for server live rows.

use super::*;
use crate::serve::test_support::*;
#[test]
fn live_chunks_match_full_frames_for_claude_and_codex() {
    for codex in [false, true] {
        let mut incremental = LiveRows::empty(0, codex);
        let mut reference = LiveRows::empty(0, codex);
        let mut events = (0..320)
            .map(|number| synthetic_event(number, true))
            .collect::<Vec<_>>();
        events.extend([
            json!({"type":"tool_call_update","tool_call_id":"tool-5","status":"completed","content":"late result"}),
            json!({"type":"usage","kind":"context","context_tokens":80,"context_capacity_tokens":100}),
            json!({"type":"turn_ended","reason":"completed"}),
            json!({"type":"turn_started"}),
            json!({"type":"user_message_chunk","text":"next","meta":{"uuid":"next"}}),
            json!({"type":"agent_message_chunk","text":"answer","meta":{"uuid":"answer"}}),
            json!({"type":"agent_message_chunk","text":" more","meta":{"uuid":"answer"}}),
            json!({"type":"agent_thought_chunk","text":"thinking","meta":{"uuid":"thinking"}}),
            json!({"type":"system_message","kind":"swarm_ring","text":"ring"}),
            json!({"type":"tool_call","tool_call_id":"joined","name":"Bash","input":{"command":"true"}}),
            json!({"type":"tool_call_update","tool_call_id":"joined","status":"completed","content":"done"}),
            json!({"type":"tool_call","tool_call_id":"joined","name":"Bash","input":{"command":"false"}}),
            json!({"type":"system_message","kind":"compaction"}),
            json!({"type":"queue_operation","operation":"enqueue","content":"queued"}),
            json!({"type":"queue_operation","operation":"dequeue"}),
            json!({"type":"tool_call_update","tool_call_id":"retro","status":"completed","content":"before call"}),
            json!({"type":"tool_call","tool_call_id":"retro","name":"Read","input":{}}),
            json!({"type":"system_message","kind":"command","text":"<command-name>/example</command-name>","meta":{"uuid":"command"}}),
            json!({"type":"system_message","kind":"skill_body","text":"body","meta":{"parent_uuid":"command"}}),
            json!({"type":"system_message","kind":"command_output","text":"output","meta":{"parent_uuid":"command"}}),
            json!({"type":"system_message","kind":"shell_input","text":"<bash-input>true</bash-input>","meta":{"uuid":"shell"}}),
            json!({"type":"system_message","kind":"shell_output","text":"<bash-stdout>done</bash-stdout>","meta":{"parent_uuid":"shell"}}),
            json!({"type":"system_message","kind":"shell_output","text":"<bash-stdout>second</bash-stdout>","meta":{"parent_uuid":"shell"}}),

        ]);
        events.extend((0..99).map(|number|json!({"type":"session_info","kind":"title","value":"hidden","meta":{"uuid":format!("hidden-{number}")}})));
        events.extend([
            json!({"type":"tool_call","tool_call_id":"hidden-100","name":"Read","input":{}}),
            json!({"type":"turn_ended","reason":"completed"}),
            json!({"type":"tool_call","tool_call_id":"hidden-101","name":"Read","input":{}}),
        ]);
        for (index, event) in events.into_iter().enumerate() {
            incremental.ingest(index as u64, index as u64 + 1, std::slice::from_ref(&event));
            if event["type"] == "queue_operation" {
                assert!(!incremental.rebuild, "a queue edit rebuilt the open turn");
            }
            reference.ingest(index as u64, index as u64 + 1, &[event]);
            let expected = reference_delta(&mut reference, "chat");
            let actual = incremental.delta("chat").data;
            assert_eq!(actual, expected.to_string(), "event {index}, codex={codex}");
        }
        assert_eq!(incremental.rows(), reference.last_rows);
        assert_eq!(
            incremental.usage.value(),
            view::usage(&reference.records.values().cloned().collect::<Vec<_>>())
        );
    }
}
#[test]
fn trimmed_history_has_the_same_live_frames() {
    let mut live = LiveRows::empty(0, false);
    let mut reference = LiveRows::empty(0, false);
    let first = [
        json!({"type":"queue_operation","operation":"enqueue","content":"pending"}),
        json!({"type":"usage","kind":"context","context_tokens":80,"context_capacity_tokens":100}),
    ];
    live.ingest(0, 1, &first);
    reference.ingest(0, 1, &first);
    live.delta("chat");
    reference_delta(&mut reference, "chat");
    for index in 0..2400 {
        let event = synthetic_event(index, false);
        live.ingest(
            index as u64 + 1,
            index as u64 + 2,
            std::slice::from_ref(&event),
        );
        reference.ingest(index as u64 + 1, index as u64 + 2, &[event]);
        if index % 8 == 7 {
            assert_eq!(
                serde_json::from_str::<Value>(&live.delta("chat").data).unwrap(),
                reference_delta(&mut reference, "chat"),
                "event {index}"
            );
        }
    }
    assert!(live.records.len() <= ROW_RECORD_LIMIT);
}
#[tokio::test]
async fn synthetic_provider_logs_match_full_rebuild() {
    for codex in [false, true] {
        let fixture = Fixture::new();
        let lines=(0..12).flat_map(|turn| {
            let id=format!("tool-{turn}");
            if codex { vec![codex_line(turn).trim_end().to_owned(),
                json!({"type":"response_item","payload":{"type":"function_call","call_id":id,"name":"shell","arguments":"{\"command\":\"true\"}"}}).to_string(),
                json!({"type":"response_item","payload":{"type":"function_call_output","call_id":id,"output":"done"}}).to_string()]
            } else { vec![
                json!({"type":"user","uuid":format!("user-{turn}"),"message":{"content":"prompt"}}).to_string(),
                json!({"type":"assistant","uuid":format!("thought-{turn}"),"message":{"content":[{"type":"thinking","thinking":"think"}]}}).to_string(),
                json!({"type":"assistant","uuid":format!("call-{turn}"),"message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":"true"}}]}}).to_string(),
                json!({"type":"user","uuid":format!("result-{turn}"),"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"done"}]}}).to_string()]
            }
        }).collect::<Vec<_>>();
        std::fs::write(&fixture.log, lines.join("\n") + "\n").unwrap();
        let log = Log {
            path: fixture.log.clone(),
            format: if codex { "codex" } else { "claude" }.into(),
        };
        let mut state = LiveRows::empty(0, codex);
        let mut start = 0;
        for line in &lines {
            let end = start + line.len() as u64 + 1;
            let page = read_row_window(&fixture.app.tool, &log, start, end)
                .await
                .unwrap();
            state.ingest(
                start,
                end,
                &page
                    .records
                    .values()
                    .map(|record| record.event.clone())
                    .collect::<Vec<_>>(),
            );
            state.delta("chat");
            start = end;
        }
        let mut full = read_row_window(&fixture.app.tool, &log, 0, start)
            .await
            .unwrap();
        full.shown = state.shown.clone();
        assert_eq!(state.rows(), full.rows_full());
        assert_eq!(
            state.usage.value(),
            view::usage(&full.records.values().cloned().collect::<Vec<_>>())
        );
    }
}
#[test]
fn two_hundred_fifty_tools_make_three_stable_folds() {
    let records = (0..250)
        .map(|number| {
            Record::new(
                synthetic_event(number, true),
                format!("raw-{number}-0"),
                false,
            )
        })
        .collect::<Vec<_>>();
    let rows = view::view_rows(&records, false, false, &HashSet::new());
    assert_eq!(
        rows.iter().map(|row| row.steps.len()).collect::<Vec<_>>(),
        [100, 100, 50]
    );
    let first = view::view_rows(&records[..100], false, false, &HashSet::new());
    assert_eq!(rows[0].id, first[0].id);
}
#[test]
fn row_frames_report_changed_usage_and_clear_it() {
    let mut state = LiveRows::empty(0, false);
    let usage =
        json!({"type":"usage","kind":"context","context_tokens":80,"context_capacity_tokens":100});
    state.ingest(0, 1, &[usage]);
    let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
    assert_eq!(payload["usage"]["remaining_percent"], 20);
    let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
    assert!(payload.get("usage").is_none());
    state.ingest(1, 2, &[json!({"type":"usage","kind":"context","context_tokens":90,"context_capacity_tokens":100})]);
    let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
    assert_eq!(payload["usage"]["remaining_percent"], 10);
    state.ingest(
        2,
        3,
        &[json!({"type":"system_message","kind":"compaction"})],
    );
    let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
    assert!(payload.get("usage").unwrap().is_null());
}
