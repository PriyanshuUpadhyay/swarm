mod sidebar_fixture;
mod tab_fixture;
use serde_json::json;
use sidebar_fixture::*;
use std::collections::BTreeMap;
use swarm::view::{
    choices::{PendingTab, ViewState},
    tabs::{self, TabStrip},
};
use tab_fixture::*;

#[test]
fn recent_chats_wrap_and_missing_current() {
    let history = strip(&["oldest", "hidden", "middle", "newest", "middle"]).open;
    let open = strip(&["newest", "middle", "oldest"]).open;
    for (current, step, target) in [
        (Some("middle"), -1, "newest"),
        (Some("oldest"), -1, "middle"),
        (Some("middle"), 1, "oldest"),
        (Some("oldest"), 1, "newest"),
    ] {
        assert_eq!(
            tabs::flip(&history, &open, current, step).as_deref(),
            Some(target)
        );
    }
    let h = strip(&["older", "recent"]).open;
    assert_eq!(tabs::flip(&h, &h, None, -1).as_deref(), Some("recent"));
    assert_eq!(
        tabs::flip(&h, &h, Some("hidden"), 1).as_deref(),
        Some("older")
    );
    assert!(tabs::flip(&["hidden".into()], &["open".into()], Some("open"), -1).is_none());
    assert!(tabs::flip(&h, &[], None, 1).is_none());
    assert_eq!(
        tabs::flip(&["only".into()], &["only".into()], Some("only"), 1).as_deref(),
        Some("only")
    );
}
#[test]
fn history_twenty_distinct_per_workspace() {
    let mut state = ViewState::default();
    for i in 0..25 {
        state.record_selection("/repo", &format!("chat-{i}"));
    }
    state.record_selection("/repo", "chat-5");
    state.record_selection("/other", "notes");
    let expected: Vec<_> = (6..25)
        .map(|i| format!("chat-{i}"))
        .chain(["chat-5".into()])
        .collect();
    assert_eq!(state.tab_history["/repo"], expected);
    assert_eq!(state.tab_history["/other"], ["notes"]);
    assert_eq!(
        serde_json::from_value::<ViewState>(serde_json::to_value(&state).unwrap()).unwrap(),
        state
    );
}
#[test]
fn selection_after_close() {
    assert_eq!(
        strip(&["review", "design"])
            .selection_after_close(&strip(&["design", "review", "fix"]).open),
        Some("review")
    );
    assert_eq!(
        strip(&["design", "review"]).selection_after_close(&["missing".into()]),
        Some("design")
    );
    assert!(
        TabStrip::default()
            .selection_after_close(&["design".into()])
            .is_none()
    );
}
#[test]
fn first_sight_seeds_live_only_once() {
    let tree = tree();
    let (_, entry) = tree.workspace("/repo/wt/main").unwrap();
    assert_eq!(entry.seed_tabs().open, ["review", "design"]);
    let mut owner = owner();
    owner.tabs.insert(entry.path.clone(), TabStrip::default());
    let view = tabs::view(
        &tree,
        &owner,
        &ViewState::default(),
        &entry.path,
        1,
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(view.tabs.is_empty());
    owner
        .tabs
        .insert(entry.path.clone(), strip(&["missing", "review", "design"]));
    assert_eq!(
        tabs::view(
            &tree,
            &owner,
            &ViewState::default(),
            &entry.path,
            1,
            &BTreeMap::new()
        )
        .unwrap()
        .strip
        .open,
        ["review", "design"]
    );
}
#[test]
fn pending_order_start_dedup_and_closing() {
    let tree = tree();
    let mut owner = owner();
    owner
        .tabs
        .insert("/repo/wt/main".into(), strip(&["review", "design"]));
    let mut state = ViewState::default();
    state.pending.push(PendingTab {
        id: "pending:1".into(),
        workspace: "/repo/wt/main".into(),
        session: None,
        state: "starting".into(),
    });
    state.pending.push(PendingTab {
        id: "pending:2".into(),
        workspace: "/repo/wt/main".into(),
        session: Some("design".into()),
        state: "launched".into(),
    });
    let view = tabs::view(&tree, &owner, &state, "/repo/wt/main", 1, &BTreeMap::new()).unwrap();
    assert_eq!(
        view.tabs.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["pending:2", "pending:1", "review"]
    );
    assert!(!view.tabs[0].can_hide);
    assert!(!view.tabs[0].can_close);
    assert_eq!(view.tabs[0].pending.as_deref(), Some("starting"));
    state.pending.clear();
    state.closing.insert("review".into());
    let view = tabs::view(&tree, &owner, &state, "/repo/wt/main", 1, &BTreeMap::new()).unwrap();
    assert!(!view.tabs[0].can_close);
    assert!(view.tabs[1].can_close);
    assert_eq!(view.tabs[1].badge.as_deref(), Some("X"));
    assert_eq!(tabs::badge("claude"), "C");
    assert_eq!(tabs::badge("agy"), "A");
}
#[test]
fn group_runs_and_overflow() {
    let tree = tree();
    let mut owner = owner();
    let mut strip = strip(&["design", "review", "ended"]);
    strip.groups.push(group("planning", &["design", "review"]));
    strip.groups[0].folded = true;
    owner.tabs.insert("/repo/wt/main".into(), strip);
    let view = tabs::view(
        &tree,
        &owner,
        &ViewState::default(),
        "/repo/wt/main",
        1,
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(view.runs.len(), 2);
    assert_eq!(view.runs[0].tabs, ["design", "review"]);
    assert!(view.runs[0].group.as_ref().unwrap().folded);
    assert!(view.runs[1].group.is_none());
    let edges = [
        ("design".into(), 500.0),
        ("ended".into(), 250.0),
        ("review".into(), 400.0),
    ]
    .into();
    assert_eq!(
        tabs::overflow(&view.tabs, &edges, 250.0),
        ["design", "review"]
    );
    assert!(tabs::overflow(&view.tabs, &edges, 500.0).is_empty());
    assert!(tabs::overflow(&view.tabs, &edges, 0.0).is_empty());
    assert!(tabs::overflow(&view.tabs, &BTreeMap::new(), 250.0).is_empty());
}
#[test]
fn child_badges_count_live_and_first_waiting() {
    let snap = snapshot(
        vec![session("design", "/repo/wt/main", 1)],
        json!({"design":[agent("orchestrator","waiting"),agent("builder","working"),agent("reviewer","waiting"),agent("analyst","waiting"),agent("ended","ended"),json!({"id":"registered","alive":true,"pane":null,"state":"waiting"}),json!({"id":"unknown","pane":"p1","state":"waiting"})]}),
    );
    let tree = swarm::view::tree::build(&snap, &owner());
    let view = tabs::view(
        &tree,
        &owner(),
        &ViewState::default(),
        "/repo/wt/main",
        1,
        &BTreeMap::new(),
    )
    .unwrap();
    let children = view.tabs[0].children.as_ref().unwrap();
    assert_eq!(children.text, "3 · 2 waiting");
    assert_eq!(children.first_waiting.as_deref(), Some("analyst"));
    let mut named = session("design", "/repo/wt/main", 1);
    named["chair_id"] = json!("chair-custom");
    let tree = swarm::view::tree::build(
        &snapshot(
            vec![named],
            json!({"design":[agent("chair-custom","waiting"),agent("builder","working")]}),
        ),
        &owner(),
    );
    assert_eq!(
        tabs::view(
            &tree,
            &owner(),
            &ViewState::default(),
            "/repo/wt/main",
            1,
            &BTreeMap::new()
        )
        .unwrap()
        .tabs[0]
            .children
            .as_ref()
            .unwrap()
            .text,
        "1"
    );
}
