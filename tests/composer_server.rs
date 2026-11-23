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

/// What the composer queue holds after its log is replayed, one row per operation.
// One push per row, so a blank line can separate the rows.
#[allow(clippy::vec_init_then_push)]
#[test]
fn replaying_the_queue_log_leaves_the_owners_pending_text() {
    let mut rows: Vec<(Vec<Value>, Vec<&str>)> = Vec::new();
    // remove drops the first equal text
    rows.push((
        vec![
            op("enqueue", Some(ADD_TEST)),
            op("enqueue", Some(KEEP_NAME)),
            op("remove", Some(ADD_TEST)),
        ],
        vec![KEEP_NAME],
    ));

    // dequeue takes the head
    rows.push((
        vec![op("enqueue", Some(ADD_TEST)), op("dequeue", None)],
        vec![],
    ));

    // popAll empties
    rows.push((
        vec![
            op("enqueue", Some(ADD_TEST)),
            op("enqueue", Some(KEEP_NAME)),
            op("popAll", Some(ADD_TEST)),
            op("popAll", Some(KEEP_NAME)),
        ],
        vec![],
    ));

    // CLI entries are hidden, also when one is dequeued
    rows.push((
        vec![
            op("enqueue", Some(TASK_NOTIFICATION)),
            op("enqueue", Some(AGENT_MESSAGE)),
        ],
        vec![],
    ));

    rows.push((
        vec![
            op("enqueue", Some(TASK_NOTIFICATION)),
            op("enqueue", Some(ADD_TEST)),
            op("dequeue", None),
        ],
        vec![ADD_TEST],
    ));

    // an unknown operation and a foreign event are ignored
    rows.push((
        vec![
            op("enqueue", Some(ADD_TEST)),
            op("reorder", Some(ADD_TEST)),
            json!({"type":"user_message_chunk","text":ADD_TEST}),
        ],
        vec![ADD_TEST],
    ));

    for (events, expected) in rows {
        assert_eq!(pending(&events), texts(&expected), "{events:?}");
    }
}

/// The pull-back decision for the owner's text, one row per log, with the Backspace presses a
/// decision needs to clear the box.
// One push per row, so a blank line can separate the rows.
#[allow(clippy::vec_init_then_push)]
#[test]
fn a_pull_back_decision_follows_the_queue_log() {
    let notification = "<task-notification>a\nb";
    let sent = || PullBack::AlreadySent { popped: vec![] };
    let mut rows: Vec<(Vec<&str>, Vec<Value>, bool, PullBack)> = Vec::new();
    rows.push((
        vec![ADD_TEST, KEEP_NAME],
        vec![op("popAll", Some(ADD_TEST)), op("popAll", Some(KEEP_NAME))],
        false,
        pulled(&[ADD_TEST, KEEP_NAME], &[ADD_TEST, KEEP_NAME]),
    ));

    rows.push((
        vec![ADD_TEST, KEEP_NAME],
        vec![op("popAll", Some(ADD_TEST)), op("remove", Some(KEEP_NAME))],
        false,
        pulled(&[ADD_TEST], &[ADD_TEST]),
    ));

    // text the list does not name is kept
    rows.push((
        vec![ADD_TEST],
        vec![op("popAll", Some(ADD_TEST)), op("popAll", Some(KEEP_NAME))],
        false,
        pulled(&[ADD_TEST, KEEP_NAME], &[ADD_TEST, KEEP_NAME]),
    ));

    // a CLI entry is popped but never owned
    rows.push((
        vec![TASK_NOTIFICATION, ADD_TEST],
        vec![
            op("popAll", Some(TASK_NOTIFICATION)),
            op("popAll", Some(ADD_TEST)),
        ],
        false,
        pulled(&[ADD_TEST], &[TASK_NOTIFICATION, ADD_TEST]),
    ));

    rows.push((
        vec![notification, "c"],
        vec![op("popAll", Some(notification)), op("popAll", Some("c"))],
        false,
        pulled(&["c"], &[notification, "c"]),
    ));

    rows.push((
        vec![TASK_NOTIFICATION, ADD_TEST],
        vec![
            op("popAll", Some(TASK_NOTIFICATION)),
            op("remove", Some(ADD_TEST)),
        ],
        false,
        PullBack::AlreadySent {
            popped: texts(&[TASK_NOTIFICATION]),
        },
    ));

    rows.push((
        vec![TASK_NOTIFICATION, ADD_TEST],
        vec![op("dequeue", None)],
        false,
        PullBack::Waiting,
    ));

    rows.push((
        vec![TASK_NOTIFICATION, ADD_TEST],
        vec![op("dequeue", None), op("popAll", Some(ADD_TEST))],
        false,
        pulled(&[ADD_TEST], &[ADD_TEST]),
    ));

    rows.push((
        vec![ADD_TEST],
        vec![op("remove", Some(ADD_TEST))],
        false,
        sent(),
    ));

    rows.push((vec![ADD_TEST], vec![op("dequeue", None)], false, sent()));

    rows.push((vec![ADD_TEST], vec![], false, PullBack::Waiting));

    rows.push((vec![ADD_TEST], vec![], true, PullBack::Unconfirmed));

    rows.push((
        vec![ADD_TEST],
        vec![op("remove", Some(TASK_NOTIFICATION))],
        false,
        PullBack::Waiting,
    ));

    for (list, events, deadline, expected) in rows {
        assert_eq!(decide(&list, &events, deadline), expected, "{events:?}");
    }
    assert_eq!(queue::clear_presses(&texts(&[ADD_TEST])), 3);
    assert_eq!(queue::clear_presses(&texts(&["a\nb"])), 5);
    assert_eq!(queue::clear_presses(&texts(&[notification, "c"])), 7);

    let already = PullBack::AlreadySent {
        popped: texts(&[TASK_NOTIFICATION]),
    };
    assert_eq!(already.clear_presses(ADD_TEST), 5);
    assert_eq!(PullBack::Unconfirmed.clear_presses(ADD_TEST), 0);

    for adapter in ["tmux", "tmux-swarm"] {
        assert!(queue::can_pull_back(Some("claude"), adapter));
    }
    assert!(!queue::can_pull_back(Some("claude"), "herdr"));
    assert!(!queue::can_pull_back(Some("codex"), "tmux-swarm"));
}

#[test]
fn a_submission_is_cleaned_like_claude_and_plain_text_is_kept() {
    let vectors: Vec<(String, String)> =
        serde_json::from_str(include_str!("fixtures/claude-cleaner.json")).unwrap();
    for (input, expected) in vectors {
        assert_eq!(claude_text::cleaned(&input), expected, "{input:?}");
    }
    let sentence = "Add a test for the empty case, then mail me@host.\nThanks";
    assert_eq!(claude_text::cleaned(sentence), sentence);
}
