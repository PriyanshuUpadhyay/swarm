// Agent status mapping and aggregation tests.
use super::*;

#[test]
fn agent_status_mapping() {
    assert_eq!(Status::new(Some(false), Some("working")), Status::Ended);
    for alive in [Some(true), None] {
        for state in [None, Some("compacting"), Some("ended")] {
            assert_eq!(Status::new(alive, state), Status::Done);
        }
    }
    for (state, status) in [
        ("working", Status::Working),
        ("waiting", Status::Waiting),
        ("done", Status::Done),
        ("failed", Status::Failed),
    ] {
        assert_eq!(Status::new(Some(true), Some(state)), status);
    }
}
#[test]
fn agent_status_aggregate() {
    assert_eq!(Status::aggregate([]), None);
    for (values, status) in [
        (vec![Status::Ended, Status::Done], Status::Done),
        (
            vec![Status::Done, Status::Working, Status::Ended],
            Status::Working,
        ),
        (vec![Status::Working, Status::Failed], Status::Failed),
        (
            vec![Status::Failed, Status::Waiting, Status::Working],
            Status::Waiting,
        ),
    ] {
        assert_eq!(Status::aggregate(values), Some(status));
    }
}
#[test]
fn agent_status_decoding() {
    let future: Value = serde_json::from_str(
        r#"{"id":"coder","pane":"%2","alive":true,"state":"future","state_kind":"future-kind"}"#,
    )
    .unwrap();
    assert_eq!(Status::agent(&future), Status::Done);
    assert_eq!(
        Status::agent(&json!({"pane":null,"alive":true,"state":"waiting"})),
        Status::Ended
    );
    assert_eq!(
        Status::agent(&json!({"pane":"%1","alive":false,"state":"working"})),
        Status::Ended
    );
    assert_eq!(
        Status::agent(
            &json!({"pane":"%1","alive":true,"state":"waiting","state_at_s":1790000000,"state_detail":"permission prompt"})
        ),
        Status::Waiting
    );
    let list: Value = serde_json::from_str(
        r#"{"agents":[{"id":"coder","role":"code","provider":"codex","pane":"p1","alive":true}]}"#,
    )
    .unwrap();
    let coder = &list["agents"][0];
    assert_eq!(coder["provider"], "codex");
    assert_eq!(coder["alive"], true);
    let context = swarm::view::fields::agent_context(coder, "Coder");
    assert_eq!(context.text["provider"], "codex");
    let reviewer: Value = serde_json::from_str(r#"{"id":"reviewer","role":"review","pane":"%3","alive":true,"state":"waiting","state_at_s":1790000000,"state_source":"hook","state_detail":"permission prompt","state_kind":"permission"}"#).unwrap();
    assert_eq!(reviewer["state_at_s"], 1790000000i64);
    assert_eq!(reviewer["state_source"], "hook");
    assert_eq!(reviewer["state_kind"], "permission");
    assert_eq!(
        swarm::view::fields::agent_context(&reviewer, "Reviewer").text["question"],
        "permission prompt"
    );
}
