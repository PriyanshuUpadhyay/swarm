use serde_json::Value;
use swarm::view::transcript::source::{NativeFormat, SnapshotError};

mod common;

fn turns(format: NativeFormat, path: &str, log: &str) -> Vec<(String, String)> {
    let mut reader = common::reader(format, "chat", path);
    reader.push(log.as_bytes(), &mut common::Ready).unwrap();
    reader
        .finish_native(&mut common::Ready)
        .unwrap()
        .lines
        .into_iter()
        .flat_map(|line| line.events)
        .filter_map(|event| {
            let event: Value = serde_json::to_value(event).unwrap();
            let kind = event["type"].as_str().unwrap();
            matches!(kind, "user_message_chunk" | "agent_message_chunk")
                .then(|| (kind.to_owned(), event["text"].as_str().unwrap().to_owned()))
        })
        .collect()
}

#[test]
fn codex_normalizes_a_prompt_and_reply() {
    let log = concat!(
        r#"{"type":"response_item","payload":{"type":"message","id":"u1","role":"user","content":[{"type":"input_text","text":"List the files."}]}}"#,
        "\n",
        r#"{"type":"response_item","payload":{"type":"message","id":"a1","role":"assistant","content":[{"type":"output_text","text":"I will look."}]}}"#,
        "\n",
    );
    assert_eq!(
        turns(NativeFormat::Codex, "rollout.jsonl", log),
        [
            ("user_message_chunk".into(), "List the files.".into()),
            ("agent_message_chunk".into(), "I will look.".into()),
        ]
    );
}

#[test]
fn agy_normalizes_a_prompt_and_reply() {
    let log = concat!(
        r#"{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":1,"created_at":"2026-11-05T10:00:01Z","content":"<USER_REQUEST>Read the file.</USER_REQUEST>"}"#,
        "\n",
        r#"{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":2,"created_at":"2026-11-05T10:00:02Z","content":"Reading it now."}"#,
        "\n",
    );
    assert_eq!(
        turns(NativeFormat::Agy, "chat.jsonl", log),
        [
            ("user_message_chunk".into(), "Read the file.".into()),
            ("agent_message_chunk".into(), "Reading it now.".into()),
        ]
    );
}

#[test]
fn pi_normalizes_a_prompt_and_reply() {
    let log = concat!(
        r#"{"type":"message","id":"u1","parentId":null,"timestamp":"2026-11-05T00:00:00.000Z","message":{"role":"user","content":[{"type":"text","text":"Read café.txt."}]}}"#,
        "\n",
        r#"{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-11-05T00:00:01.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Done."}],"stopReason":"stop"}}"#,
        "\n",
    );
    assert_eq!(
        turns(NativeFormat::Pi, "2026-11-05T00-00-00-000Z_chat.jsonl", log),
        [
            ("user_message_chunk".into(), "Read café.txt.".into()),
            ("agent_message_chunk".into(), "Done.".into()),
        ]
    );
}

#[test]
fn pi_token_sum_overflow_returns_no_snapshot() {
    let log = concat!(
        r#"{"type":"message","id":"u1","parentId":null,"timestamp":"2026-11-05T00:00:00.000Z","message":{"role":"user","content":"Read the file."}}"#,
        "\n",
        r#"{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-11-05T00:00:01.000Z","message":{"role":"assistant","content":[{"type":"text","text":"Done."}],"stopReason":"stop","usage":{"input":4611686018427387904,"output":0,"cacheRead":4611686018427387904,"cacheWrite":4611686018427387904}}}"#,
        "\n",
    );
    let mut reader = common::reader(NativeFormat::Pi, "chat", "chat.jsonl");
    let result = reader
        .push(log.as_bytes(), &mut common::Ready)
        .and_then(|()| reader.finish_native(&mut common::Ready));
    assert!(matches!(result, Err(SnapshotError::NativeTokenOverflow)));
}
