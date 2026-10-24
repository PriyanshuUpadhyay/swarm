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
/// AGY reports its conversation id only after a turn, so its idle screen says it is ready.
#[test]
fn an_agy_chair_is_ready_at_its_idle_prompt() {
    let agents =
        |state: Value| json!({"agents":[{"id":"orchestrator","alive":true,"state":state}]});
    assert!(handoff::agy_ready(&agents(json!("done"))));
    assert!(!handoff::agy_ready(&agents(json!(null))));
    assert!(!handoff::agy_ready(&agents(json!("waiting"))));
    assert!(!handoff::agy_ready(
        &json!({"agents":[{"id":"orchestrator","alive":false,"state":"done"}]})
    ));
}
#[test]
fn completed_summary() {
    let events = vec![
        message("agent_message_chunk", "old answer"),
        message("user_message_chunk", handoff::REQUEST),
        message("agent_message_chunk", "files and next step"),
    ];
    assert_eq!(handoff::summary_turn(&rows(events.clone()), 1), None);
    let mut events = events;
    events.push(end("completed"));
    assert_eq!(
        handoff::summary_turn(&rows(events), 1),
        Some(Some("files and next step".into()))
    );
}
/// A summary turn that ends early, by the owner or by a usage limit, has no summary, so the switch
/// stops waiting at once.
#[test]
fn interrupted_summary() {
    for interrupt in [
        vec![
            json!({"type":"system_message","kind":"interrupted","text":"[Request interrupted by user]"}),
        ],
        vec![end("aborted")],
        vec![
            json!({"type":"error","message":"Claude rate limit (seven_day)"}),
            end("aborted"),
        ],
    ] {
        let mut events = vec![
            message("user_message_chunk", handoff::REQUEST),
            message("agent_message_chunk", "Goal: fix the"),
        ];
        events.extend(interrupt);
        assert_eq!(handoff::summary_turn(&rows(events.clone()), 0), Some(None));
        events.extend([
            message("user_message_chunk", "Carry on"),
            message("agent_message_chunk", "Done"),
            end("completed"),
        ]);
        assert_eq!(handoff::summary_turn(&rows(events), 0), Some(None));
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
        handoff::summary_turn(&summary, 0)
            .flatten()
            .unwrap()
            .chars()
            .count(),
        12_000
    );
    assert!(handoff::first_message("context", true).contains("Use this context"));
    assert!(handoff::first_message("context", false).contains("Make a compact working summary"));
}
