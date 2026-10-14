use serde_json::{Value, json};
use swarm::view::{
    claude_text,
    queue::{self, PullBack},
};
const ADD_TEST: &str = "add a test for the empty case";
const KEEP_NAME: &str = "keep the old name";
const TASK_NOTIFICATION: &str = "<task-notification>\n<task-id>t1</task-id>";
const AGENT_MESSAGE: &str = "<agent-message from=\"coder\">done</agent-message>";
fn op(operation: &str, content: Option<&str>) -> Value {
    json!({"type":"queue_operation","operation":operation,"content":content})
}
fn texts(values: &[&str]) -> Vec<String> {
    values.iter().map(|x| (*x).into()).collect()
}
fn pending(events: &[Value]) -> Vec<String> {
    queue::replay(events)
        .into_iter()
        .filter(|text| queue::is_owner(text))
        .collect()
}
fn decide(values: &[&str], events: &[Value], deadline: bool) -> PullBack {
    PullBack::decide(&texts(values), events, deadline)
}
fn pulled(owner: &[&str], popped: &[&str]) -> PullBack {
    PullBack::Pulled {
        owner: texts(owner),
        popped: texts(popped),
    }
}
#[test]
fn remove_drops_first_equal() {
    assert_eq!(
        pending(&[
            op("enqueue", Some(ADD_TEST)),
            op("enqueue", Some(KEEP_NAME)),
            op("remove", Some(ADD_TEST))
        ]),
        texts(&[KEEP_NAME])
    );
}
#[test]
fn dequeue_takes_head() {
    assert!(pending(&[op("enqueue", Some(ADD_TEST)), op("dequeue", None)]).is_empty());
}
#[test]
fn pop_all_empties() {
    assert!(
        pending(&[
            op("enqueue", Some(ADD_TEST)),
            op("enqueue", Some(KEEP_NAME)),
            op("popAll", Some(ADD_TEST)),
            op("popAll", Some(KEEP_NAME))
        ])
        .is_empty()
    );
}
#[test]
fn cli_entries_hidden() {
    assert!(
        pending(&[
            op("enqueue", Some(TASK_NOTIFICATION)),
            op("enqueue", Some(AGENT_MESSAGE))
        ])
        .is_empty()
    );
    assert_eq!(
        pending(&[
            op("enqueue", Some(TASK_NOTIFICATION)),
            op("enqueue", Some(ADD_TEST)),
            op("dequeue", None)
        ]),
        texts(&[ADD_TEST])
    );
}
#[test]
fn unknown_operation_ignored() {
    assert_eq!(
        pending(&[
            op("enqueue", Some(ADD_TEST)),
            op("reorder", Some(ADD_TEST)),
            json!({"type":"user_message_chunk","text":ADD_TEST})
        ]),
        texts(&[ADD_TEST])
    );
}
#[test]
fn pull_back_pulls_each() {
    assert_eq!(
        decide(
            &[ADD_TEST, KEEP_NAME],
            &[op("popAll", Some(ADD_TEST)), op("popAll", Some(KEEP_NAME))],
            false
        ),
        pulled(&[ADD_TEST, KEEP_NAME], &[ADD_TEST, KEEP_NAME])
    );
    assert_eq!(
        decide(
            &[ADD_TEST, KEEP_NAME],
            &[op("popAll", Some(ADD_TEST)), op("remove", Some(KEEP_NAME))],
            false
        ),
        pulled(&[ADD_TEST], &[ADD_TEST])
    );
}
#[test]
fn pull_back_keeps_unlisted_text() {
    assert_eq!(
        decide(
            &[ADD_TEST],
            &[op("popAll", Some(ADD_TEST)), op("popAll", Some(KEEP_NAME))],
            false
        ),
        pulled(&[ADD_TEST, KEEP_NAME], &[ADD_TEST, KEEP_NAME])
    );
}
#[test]
fn pull_back_skips_cli_entries() {
    assert_eq!(
        decide(
            &[TASK_NOTIFICATION, ADD_TEST],
            &[
                op("popAll", Some(TASK_NOTIFICATION)),
                op("popAll", Some(ADD_TEST))
            ],
            false
        ),
        pulled(&[ADD_TEST], &[TASK_NOTIFICATION, ADD_TEST])
    );
}
#[test]
fn pull_back_clears_cli_entry_lines() {
    let notification = "<task-notification>a\nb";
    let decision = decide(
        &[notification, "c"],
        &[op("popAll", Some(notification)), op("popAll", Some("c"))],
        false,
    );
    assert_eq!(decision, pulled(&["c"], &[notification, "c"]));
    assert_eq!(queue::clear_presses(&texts(&[notification, "c"])), 7);
}
#[test]
fn already_sent_clears_popped_cli_entry() {
    let decision = decide(
        &[TASK_NOTIFICATION, ADD_TEST],
        &[
            op("popAll", Some(TASK_NOTIFICATION)),
            op("remove", Some(ADD_TEST)),
        ],
        false,
    );
    assert_eq!(
        decision,
        PullBack::AlreadySent {
            popped: texts(&[TASK_NOTIFICATION])
        }
    );
    assert_eq!(decision.clear_presses(ADD_TEST), 5);
    assert_eq!(decision.clear_presses("one\ntwo\nthree"), 7);
    assert_eq!(
        PullBack::AlreadySent { popped: vec![] }.clear_presses(ADD_TEST),
        3
    );
    assert_eq!(
        pulled(&[ADD_TEST], &[TASK_NOTIFICATION, ADD_TEST]).clear_presses(ADD_TEST),
        7
    );
    assert_eq!(PullBack::Unconfirmed.clear_presses(ADD_TEST), 0);
}
#[test]
fn pull_back_dequeue_of_cli_entry() {
    assert_eq!(
        decide(
            &[TASK_NOTIFICATION, ADD_TEST],
            &[op("dequeue", None)],
            false
        ),
        PullBack::Waiting
    );
    assert_eq!(
        decide(
            &[TASK_NOTIFICATION, ADD_TEST],
            &[op("dequeue", None), op("popAll", Some(ADD_TEST))],
            false
        ),
        pulled(&[ADD_TEST], &[ADD_TEST])
    );
}
#[test]
fn pull_back_already_sent() {
    for event in [op("remove", Some(ADD_TEST)), op("dequeue", None)] {
        assert_eq!(
            decide(&[ADD_TEST], &[event], false),
            PullBack::AlreadySent { popped: vec![] }
        );
    }
    assert_eq!(decide(&[ADD_TEST], &[], false), PullBack::Waiting);
    assert_eq!(decide(&[ADD_TEST], &[], true), PullBack::Unconfirmed);
    assert_eq!(
        decide(&[ADD_TEST], &[op("remove", Some(TASK_NOTIFICATION))], false),
        PullBack::Waiting
    );
}
#[test]
fn pull_back_needs_key_verb() {
    for adapter in ["tmux", "tmux-swarm"] {
        assert!(queue::can_pull_back(Some("claude"), adapter));
    }
    for (provider, adapter) in [
        (Some("claude"), "herdr"),
        (Some("claude"), ""),
        (Some("codex"), "tmux-swarm"),
    ] {
        assert!(!queue::can_pull_back(provider, adapter));
    }
}
#[test]
fn clear_presses() {
    assert_eq!(queue::clear_presses(&texts(&[ADD_TEST])), 3);
    assert_eq!(queue::clear_presses(&texts(&["a\nb"])), 5);
}
#[test]
fn submission_cleans_like_claude() {
    let vectors: Vec<(String, String)> =
        serde_json::from_str(include_str!("fixtures/claude-cleaner.json")).unwrap();
    assert_eq!(vectors.len(), 47);
    for (input, expected) in vectors {
        assert_eq!(claude_text::cleaned(&input), expected, "{input:?}");
    }
}
#[test]
fn submission_keeps_plain_text() {
    let sentence = "Add a test for the empty case, then mail me@host.\nThanks";
    assert_eq!(claude_text::cleaned(sentence), sentence);
}
#[test]
fn context_uses_only_sixteen_kept_scalars() {
    let marks = "\u{301}".repeat(16);
    assert_eq!(
        claude_text::cleaned(&format!("क{marks}\u{200d}x")),
        format!("क{marks}x")
    );
    assert_eq!(
        claude_text::cleaned("אב\u{200e}ג\nabc\u{200e} def"),
        "אבג\nabc def"
    );
    assert_eq!(claude_text::cleaned("\u{61c}م"), "\u{61c}م");
}
