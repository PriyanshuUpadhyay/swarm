use serde_json::{Value, json};
use swarm::view::transcript::{
    Row, RowKind,
    live_shown::{keeps_ended, letters, live_text, live_thought},
    reader_rows::ReaderRows,
};

fn kind(value: &str) -> RowKind {
    match value {
        "assistant" => RowKind::Assistant,
        "thought" => RowKind::Thought,
        "user" => RowKind::User,
        "toolUse" => RowKind::ToolUse,
        other => panic!("unexpected fixture kind {other}"),
    }
}
fn live_row(text: Option<&str>, thought: bool) -> Value {
    text.map_or(Value::Null, |text| {
        json!({
            "id": if thought { "live-thought" } else { "live" },
            "kind": if thought { "thought" } else { "assistant" },
            "text": text,
            "order": [9_007_199_254_740_991_i64, if thought { -2 } else { -1 }]
        })
    })
}

#[test]
fn matches_the_original_browser_decision_table() {
    let inputs: Value =
        serde_json::from_str(include_str!("fixtures/live-shown/inputs.json")).unwrap();
    let capture: Value =
        serde_json::from_str(include_str!("fixtures/live-shown/capture.json")).unwrap();
    let actual: Vec<_> = inputs["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let rows: Vec<_> = case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    Row::new(
                        kind(row["kind"].as_str().unwrap()),
                        row["text"].as_str().unwrap().to_owned(),
                        "fixture".into(),
                    )
                })
                .collect();
            let text = live_row(live_text(&rows, case["text"].as_str()), false);
            let thought = live_row(live_thought(&rows, case["thinking"].as_str()), true);
            let live_rows: Vec<_> = [thought, text.clone()]
                .into_iter()
                .filter(|row| !row.is_null())
                .collect();
            json!({ "id": case["id"], "liveRow": text, "liveRows": live_rows,
            "keepsEnded": keeps_ended(&rows, case["text"].as_str(), case["thinking"].as_str()) })
        })
        .collect();
    assert_eq!(json!({"node": "v22.23.2", "cases": actual}), capture);
}

#[test]
fn letters_match_the_regex_edge_fixture_in_utf16_units() {
    for fixture in [
        include_str!("fixtures/live-shown/edges-0.json"),
        include_str!("fixtures/live-shown/edges-1.json"),
    ] {
        let pairs: Vec<(u32, u8)> = serde_json::from_str(fixture).unwrap();
        let actual: Vec<_> = pairs
            .iter()
            .map(|&(point, _)| {
                let units = char::from_u32(point)
                    .map(|c| letters(&c.to_string()))
                    .unwrap_or_default();
                (point, units)
            })
            .collect();
        let expected: Vec<_> = pairs
            .iter()
            .map(|&(point, included)| {
                let units = if included == 1 {
                    char::from_u32(point)
                        .unwrap()
                        .to_string()
                        .encode_utf16()
                        .collect::<Vec<u16>>()
                } else {
                    Vec::new()
                };
                (point, units)
            })
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn tail_is_unavailable_before_the_first_delta() {
    assert_eq!(ReaderRows::empty(0, false).tail(50), None);
}

#[test]
fn a_thought_inside_a_fold_hides_the_live_thought() {
    let thinking = "I need to check that each anchor string appears as a new line in the diff.";
    let mut rows = ReaderRows::empty(0, false);
    let row = |kind, text: &str| {
        swarm::view::transcript::Row::new(kind, text.to_owned(), "fixture".into())
    };
    let mut fold = row(RowKind::Fold, "1 command");
    fold.steps = vec![
        row(RowKind::Thought, thinking),
        row(RowKind::ToolUse, "python3 check.py"),
    ];
    rows.prepare(vec![row(RowKind::User, "review it"), fold]);
    let mut shown = swarm::view::transcript::live_shown::LiveShown::default();
    let frame = shown
        .step(
            &rows.tail(50).unwrap(),
            Some(swarm::view::transcript::live_shown::LiveRaw {
                text: None,
                thinking: Some(thinking.to_owned()),
            }),
            false,
        )
        .unwrap();
    assert!(frame.hide_thinking);
}

#[test]
fn codex_batches_without_joins_keep_the_prepared_tail() {
    let mut rows = ReaderRows::empty(1, true);
    rows.ingest(
        1,
        2,
        &[json!({"type":"agent_thought_part","text":"Read the code"})],
    );
    rows.delta();

    rows.ingest(2, 3, &[
        json!({"type":"usage"}),
        json!({"type":"agent_message_chunk","text":"Ready"}),
        json!({"type":"tool_call","tool_call_id":"read","name":"Read","input":{"file_path":"a.rs"}}),
        json!({"type":"tool_call_update","tool_call_id":"read","status":"completed","content":"contents"}),
        json!({"type":"tool_diff","tool_call_id":"read","path":"a.rs","hunks":[]}),
    ]);
    assert_eq!(rows.tail(1).unwrap()[0].text, "Read the code");
    rows.delta();
    assert_eq!(rows.rows(), rows.rows_full());
}
