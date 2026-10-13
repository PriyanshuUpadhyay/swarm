use serde_json::{Value, json};
use swarm::view::{
    handoff,
    transcript::{self, Record, Row},
};

fn rows(events: Vec<Value>) -> Vec<Row> {
    transcript::rows(
        &events
            .into_iter()
            .enumerate()
            .map(|(i, e)| Record::new(e, i.to_string(), false))
            .collect::<Vec<_>>(),
        false,
        false,
    )
}
fn message(kind: &str, text: &str) -> Value {
    json!({"type":kind,"text":text})
}
fn end(reason: &str) -> Value {
    json!({"type":"turn_ended","reason":reason})
}
#[test]
fn ready_before_log() {
    let session = json!({"chair_provider":"codex","chair_id":"ready","chair_log":null});
    assert!(handoff::ready(&session, "codex"));
    assert!(!handoff::ready(&session, "claude"));
    assert!(!handoff::ready(&json!({"chair_provider":"codex"}), "codex"));
}
#[test]
fn completed_summary() {
    let events = vec![
        message("agent_message_chunk", "old answer"),
        message("user_message_chunk", handoff::REQUEST),
        message("agent_message_chunk", "files and next step"),
    ];
    assert_eq!(handoff::completed_summary(&rows(events.clone()), 1), None);
    let mut events = events;
    events.push(end("completed"));
    assert_eq!(
        handoff::completed_summary(&rows(events), 1).as_deref(),
        Some("files and next step")
    );
}
#[test]
fn interrupted_summary() {
    for interrupt in [
        json!({"type":"system_message","kind":"interrupted","text":"[Request interrupted by user]"}),
        end("aborted"),
    ] {
        let mut events = vec![
            message("user_message_chunk", handoff::REQUEST),
            message("agent_message_chunk", "Goal: fix the"),
            interrupt,
        ];
        assert_eq!(handoff::completed_summary(&rows(events.clone()), 0), None);
        events.extend([
            message("user_message_chunk", "Carry on"),
            message("agent_message_chunk", "Done"),
            end("completed"),
        ]);
        assert_eq!(handoff::completed_summary(&rows(events), 0), None);
    }
}
#[test]
fn recent_context() {
    let context = handoff::recent_context(&rows(vec![
        message("user_message_chunk", "Fix the menu"),
        json!({"type":"tool_call_update","content":"secret tool output"}),
        message("agent_message_chunk", "I found the filter"),
    ]))
    .unwrap();
    assert!(context.contains("User: Fix the menu"));
    assert!(context.contains("Agent: I found the filter"));
    assert!(!context.contains("secret tool output"));
    assert!(handoff::recent_context(&[]).is_none());
}
#[test]
fn context_limits_and_delivery_instructions() {
    let events = (0..20)
        .map(|i| message("user_message_chunk", &format!("message-{i:02}")))
        .collect();
    let context = handoff::recent_context(&rows(events)).unwrap();
    assert!(!context.contains("message-03"));
    assert!(context.contains("message-04"));
    let long = "é".repeat(13_000);
    assert_eq!(
        handoff::recent_context(&rows(vec![message("agent_message_chunk", &long)]))
            .unwrap()
            .chars()
            .count(),
        12_000
    );
    let summary = rows(vec![
        message("user_message_chunk", handoff::REQUEST),
        message("agent_message_chunk", &long),
        end("completed"),
    ]);
    assert_eq!(
        handoff::completed_summary(&summary, 0)
            .unwrap()
            .chars()
            .count(),
        12_000
    );
    assert!(handoff::first_message("context", true).contains("Use this context"));
    assert!(handoff::first_message("context", false).contains("Make a compact working summary"));
}
