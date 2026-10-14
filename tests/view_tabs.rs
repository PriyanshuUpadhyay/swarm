mod sidebar_fixture;
use serde_json::json;
use sidebar_fixture::*;
use std::collections::{BTreeMap, BTreeSet};
use swarm::view::{
    choices::{self, ChoiceOp, OwnerChoices, PendingTab, ViewState},
    navigation,
    tabs::{self, Grouping, TabGroup, TabStrip},
};
fn strip(open: &[&str]) -> TabStrip {
    TabStrip {
        open: open.iter().map(|s| (*s).into()).collect(),
        groups: Vec::new(),
    }
}
fn group(id: &str, members: &[&str]) -> TabGroup {
    TabGroup {
        id: id.into(),
        name: id.into(),
        color: "blue".into(),
        members: members.iter().map(|s| (*s).into()).collect(),
        folded: false,
    }
}

#[test]
fn opening() {
    let original = strip(&["design", "review"]);
    assert_eq!(
        original.opening("fix", Some("design")).open,
        ["design", "fix", "review"]
    );
    assert_eq!(original.opening("design", Some("review")), original);
    assert_eq!(
        original.opening("fix", Some("missing")).open,
        ["design", "review", "fix"]
    );
    assert_eq!(TabStrip::default().opening("design", None).open, ["design"]);
}
#[test]
fn closing_and_pruning() {
    let original = TabStrip {
        open: vec![
            "design".into(),
            "review".into(),
            "fix".into(),
            "design".into(),
        ],
        groups: vec![
            group("planning", &["review", "design"]),
            group("repair", &["fix"]),
        ],
    };
    let closed = original.closing("fix");
    assert_eq!(closed.open, ["design", "review"]);
    assert_eq!(closed.groups.len(), 1);
    assert_eq!(closed.groups[0].members, ["design", "review"]);
    assert_eq!(original.pruned(&["fix".into()].into()).open, ["fix"]);
    assert!(original.pruned(&BTreeSet::new()).groups.is_empty());
    assert_eq!(closed.closing("absent"), closed);
}
#[test]
fn moving() {
    let original = strip(&["design", "review", "fix"]);
    assert_eq!(
        original.moving("design", 2).open,
        ["review", "fix", "design"]
    );
    assert_eq!(original.moving("fix", 0).open, ["fix", "design", "review"]);
    for (key, index) in [
        ("review", 1),
        ("missing", 0),
        ("design", usize::MAX),
        ("design", 3),
    ] {
        assert_eq!(original.moving(key, index), original);
    }
}
#[test]
fn menu_moves_and_targets() {
    let mut original = strip(&["pending", "design", "review", "fix", "notes"]);
    original.groups.push(group("review", &["review", "fix"]));
    original.groups[0].folded = true;
    let targets = original.step_targets(&["pending".into()].into());
    assert!(!targets.contains_key("pending"));
    assert_eq!(targets["design"], (None, Some("fix".into())));
    assert_eq!(targets["review"], (None, Some("fix".into())));
    assert_eq!(targets["fix"], (Some("review".into()), None));
    assert_eq!(targets["notes"], (Some("review".into()), None));
    let original = original.closing("pending");
    assert_eq!(
        original.stepping("design", "fix").open,
        ["review", "fix", "design", "notes"]
    );
    assert_eq!(
        original.stepping("notes", "review").open,
        ["design", "notes", "review", "fix"]
    );
    assert_eq!(original.stepping("design", "fix").groups, original.groups);
    assert_eq!(original.stepping("review", "design"), original);
    assert_eq!(original.stepping("fix", "notes"), original);
    assert_eq!(
        original.stepping("review", "fix").groups[0].members,
        ["fix", "review"]
    );
    assert_eq!(original.stepping("missing", "design"), original);
    assert_eq!(original.stepping("design", "missing"), original);
    assert!(
        TabStrip::default()
            .step_targets(&BTreeSet::new())
            .is_empty()
    );
    assert_eq!(
        strip(&["design"]).step_targets(&BTreeSet::new())["design"],
        (None, None)
    );
}
#[test]
fn duplicate_membership() {
    let original = TabStrip {
        open: strip(&["design", "review", "fix", "notes"]).open,
        groups: vec![
            group("planning", &["design", "review"]),
            group("repair", &["review", "fix"]),
        ],
    };
    let targets = original.step_targets(&BTreeSet::new());
    assert_eq!(targets["design"].1.as_deref(), Some("review"));
    assert_eq!(targets["review"], (Some("design".into()), None));
    assert_eq!(targets["fix"], (None, None));
    let moved = original.stepping("design", "review");
    assert_eq!(moved.open, ["review", "design", "fix", "notes"]);
    assert_eq!(moved.groups[0].members, ["review", "design"]);
    assert_eq!(moved.groups[1].members, ["fix"]);
}
#[test]
fn tab_persistence_and_absent_fields() {
    let mut owner = OwnerChoices::default();
    owner.tabs.insert(
        "/repo".into(),
        TabStrip {
            open: strip(&["design", "review"]).open,
            groups: vec![group("planning", &["design"])],
        },
    );
    assert_eq!(
        serde_json::from_value::<OwnerChoices>(serde_json::to_value(&owner).unwrap()).unwrap(),
        owner
    );
    assert!(
        serde_json::from_str::<OwnerChoices>("{}")
            .unwrap()
            .tabs
            .is_empty()
    );
    assert_eq!(
        serde_json::from_str::<TabStrip>("{}").unwrap(),
        TabStrip::default()
    );
    assert_eq!(
        serde_json::from_str::<TabStrip>(r#"{"open":["design"]}"#).unwrap(),
        strip(&["design"])
    );
    assert!(
        serde_json::from_str::<TabStrip>(r#"{"groups":[]}"#)
            .unwrap()
            .open
            .is_empty()
    );
    let g: TabGroup = serde_json::from_str(
        r#"{"id":"planning","name":"Planning","color":"future","members":["design"]}"#,
    )
    .unwrap();
    assert_eq!(g.color, "grey");
    assert!(!g.folded);
}
#[test]
fn grouping_actions_and_boundaries() {
    let original = strip(&["design", "review", "fix", "notes"]);
    let planning = original.grouping(Grouping::New {
        id: "planning".into(),
        name: " Planning ".into(),
        color: "blue".into(),
        tab: "design".into(),
    });
    assert_eq!(planning.groups[0].name, "Planning");
    let joined = planning.grouping(Grouping::Add {
        tab: "fix".into(),
        group: "planning".into(),
    });
    assert_eq!(joined.open, ["design", "fix", "review", "notes"]);
    assert_eq!(joined.groups[0].members, ["design", "fix"]);
    let renamed = joined
        .grouping(Grouping::Rename {
            group: "planning".into(),
            name: "Build".into(),
        })
        .grouping(Grouping::Color {
            group: "planning".into(),
            color: "green".into(),
        })
        .grouping(Grouping::Fold {
            group: "planning".into(),
            folded: true,
        });
    assert_eq!(renamed.groups[0].name, "Build");
    assert_eq!(renamed.groups[0].color, "green");
    assert!(renamed.groups[0].folded);
    let second = renamed.grouping(Grouping::New {
        id: "reading".into(),
        name: "Reading".into(),
        color: "purple".into(),
        tab: "review".into(),
    });
    let transferred = second.grouping(Grouping::Add {
        tab: "fix".into(),
        group: "reading".into(),
    });
    assert_eq!(transferred.open, ["design", "review", "fix", "notes"]);
    assert_eq!(transferred.groups[0].members, ["design"]);
    assert_eq!(transferred.groups[1].members, ["review", "fix"]);
    assert_eq!(
        transferred
            .grouping(Grouping::Remove("review".into()))
            .groups[1]
            .members,
        ["fix"]
    );
    assert_eq!(
        transferred
            .grouping(Grouping::Remove("design".into()))
            .groups[0]
            .id,
        "reading"
    );
    let deleted = transferred.grouping(Grouping::Delete("reading".into()));
    assert_eq!(deleted.open, transferred.open);
    assert_eq!(deleted.groups.len(), 1);
    for action in [
        Grouping::New {
            id: "planning".into(),
            name: "Duplicate".into(),
            color: "red".into(),
            tab: "review".into(),
        },
        Grouping::New {
            id: "empty".into(),
            name: "  ".into(),
            color: "red".into(),
            tab: "review".into(),
        },
        Grouping::New {
            id: "missing".into(),
            name: "Missing".into(),
            color: "red".into(),
            tab: "absent".into(),
        },
        Grouping::Add {
            tab: "review".into(),
            group: "absent".into(),
        },
        Grouping::Add {
            tab: "absent".into(),
            group: "planning".into(),
        },
        Grouping::Rename {
            group: "planning".into(),
            name: "  ".into(),
        },
        Grouping::Rename {
            group: "absent".into(),
            name: "Name".into(),
        },
        Grouping::Color {
            group: "absent".into(),
            color: "red".into(),
        },
        Grouping::Fold {
            group: "absent".into(),
            folded: true,
        },
        Grouping::Remove("absent".into()),
        Grouping::Delete("absent".into()),
    ] {
        assert_eq!(planning.grouping(action), planning);
    }
}
#[test]
fn contiguous_group_order() {
    let grouped = TabStrip {
        open: strip(&["design", "review", "fix", "notes"]).open,
        groups: vec![group("planning", &["design", "review", "fix"])],
    };
    assert_eq!(
        grouped.opening("draft", Some("design")).open,
        ["design", "review", "fix", "draft", "notes"]
    );
    assert_eq!(
        grouped.moving("fix", 0).groups[0].members,
        ["fix", "design", "review"]
    );
    let outside = grouped.moving("review", 3);
    assert_eq!(outside.open, ["design", "fix", "notes", "review"]);
    assert_eq!(outside.groups[0].members, ["design", "fix"]);
    let inside = grouped.moving("notes", 1);
    assert_eq!(inside.groups[0].members, inside.open);
    assert_eq!(
        grouped.closing("review").groups[0].members,
        ["design", "fix"]
    );
    assert!(
        grouped
            .closing("design")
            .closing("review")
            .closing("fix")
            .groups
            .is_empty()
    );
    let scattered = TabStrip {
        open: strip(&["design", "notes", "review", "fix"]).open,
        groups: grouped.groups.clone(),
    };
    assert_eq!(
        scattered
            .pruned(&scattered.open.iter().cloned().collect())
            .open,
        grouped.open
    );
}
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
#[test]
fn open_hide_move_and_cross_workspace_guards() {
    let tree = tree();
    let mut owner = owner();
    let mut state = ViewState::default();
    owner
        .tabs
        .insert("/repo/wt/main".into(), strip(&["design", "review"]));
    owner
        .tabs
        .insert("/repo/wt/feature".into(), strip(&["notes"]));
    state
        .selected_chats
        .insert("/repo/wt/main".into(), "design".into());
    state.record_selection("/repo/wt/main", "review");
    state.record_selection("/repo/wt/main", "design");
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::OpenTab {
            workspace: "/repo/wt/main".into(),
            chat: "ended".into(),
            tab_only: false,
        },
        1,
    )
    .unwrap();
    assert_eq!(
        owner.tabs["/repo/wt/main"].open,
        ["design", "ended", "review"]
    );
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::CloseTab {
            workspace: "/repo/wt/main".into(),
            chat: "design".into(),
        },
        1,
    )
    .unwrap();
    assert_eq!(state.selected_chats["/repo/wt/main"], "review");
    assert_eq!(tree.workspace("/repo/wt/main").unwrap().1.chats.len(), 3);
    assert!(
        choices::apply(
            &mut owner,
            &mut state,
            &tree,
            ChoiceOp::MoveTab {
                workspace: "/repo/wt/main".into(),
                chat: "review".into(),
                target: "notes".into()
            },
            1
        )
        .is_err()
    );
    assert!(
        choices::apply(
            &mut owner,
            &mut state,
            &tree,
            ChoiceOp::OpenTab {
                workspace: "/missing".into(),
                chat: "review".into(),
                tab_only: false
            },
            1
        )
        .is_err()
    );
    assert_eq!(owner.tabs["/repo/wt/feature"].open, ["notes"]);
}
#[test]
fn menu_navigation_keeps_selection_and_history() {
    let tree = tree();
    let mut owner = owner();
    owner
        .tabs
        .insert("/repo/wt/main".into(), strip(&["design", "review"]));
    owner
        .tabs
        .insert("/repo/wt/feature".into(), strip(&["notes"]));
    let mut state = ViewState::default();
    state
        .selected_chats
        .insert("/repo/wt/main".into(), "design".into());
    state.tab_history.insert(
        "/repo/wt/main".into(),
        vec!["review".into(), "design".into()],
    );
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::StepTab {
            workspace: "/repo/wt/main".into(),
            chat: "design".into(),
            target: "review".into(),
        },
        1,
    )
    .unwrap();
    assert_eq!(owner.tabs["/repo/wt/main"].open, ["review", "design"]);
    assert_eq!(state.selected_chats["/repo/wt/main"], "design");
    assert_eq!(state.tab_history["/repo/wt/main"], ["review", "design"]);
    assert_eq!(owner.tabs["/repo/wt/feature"].open, ["notes"]);
}
#[test]
fn ended_tabs_hide_when_leaving() {
    let tree = tree();
    let mut owner = owner();
    owner.tabs.insert(
        "/repo/wt/main".into(),
        strip(&["design", "ended", "review"]),
    );
    let mut state = ViewState::default();
    for chat in ["design", "ended"] {
        choices::apply(
            &mut owner,
            &mut state,
            &tree,
            ChoiceOp::SelectedChat {
                workspace: "/repo/wt/main".into(),
                chat: Some(chat.into()),
                recording_history: true,
            },
            1,
        )
        .unwrap();
    }
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::SelectedChat {
            workspace: "/repo/wt/main".into(),
            chat: Some("ended".into()),
            recording_history: true,
        },
        1,
    )
    .unwrap();
    assert!(owner.tabs["/repo/wt/main"].open.contains(&"ended".into()));
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::SelectedChat {
            workspace: "/repo/wt/main".into(),
            chat: Some("review".into()),
            recording_history: true,
        },
        1,
    )
    .unwrap();
    assert_eq!(owner.tabs["/repo/wt/main"].open, ["design", "review"]);
    assert!(!state.tab_history["/repo/wt/main"].contains(&"ended".into()));
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::OpenTab {
            workspace: "/repo/wt/main".into(),
            chat: "ended".into(),
            tab_only: false,
        },
        1,
    )
    .unwrap();
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::SelectedChat {
            workspace: "/repo/wt/main".into(),
            chat: Some("ended".into()),
            recording_history: true,
        },
        1,
    )
    .unwrap();
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::SelectedChat {
            workspace: "/repo/wt/feature".into(),
            chat: Some("notes".into()),
            recording_history: true,
        },
        1,
    )
    .unwrap();
    assert!(!owner.tabs["/repo/wt/main"].open.contains(&"ended".into()));
    assert_eq!(tree.workspace("/repo/wt/main").unwrap().1.chats.len(), 3);
}
#[test]
fn recent_cycle_keeps_history_and_workspace_selection() {
    let tree = tree();
    let mut owner = owner();
    owner
        .tabs
        .insert("/repo/wt/main".into(), strip(&["design", "review"]));
    let mut state = ViewState::default();
    for key in ["design", "review"] {
        choices::apply(
            &mut owner,
            &mut state,
            &tree,
            ChoiceOp::SelectedChat {
                workspace: "/repo/wt/main".into(),
                chat: Some(key.into()),
                recording_history: true,
            },
            1,
        )
        .unwrap();
    }
    for expected in ["design", "review", "design"] {
        let view = tabs::view(&tree, &owner, &state, "/repo/wt/main", 1, &BTreeMap::new()).unwrap();
        assert_eq!(view.previous_recent.as_deref(), Some(expected));
        choices::apply(
            &mut owner,
            &mut state,
            &tree,
            ChoiceOp::SelectedChat {
                workspace: "/repo/wt/main".into(),
                chat: Some(expected.into()),
                recording_history: false,
            },
            1,
        )
        .unwrap();
    }
    assert_eq!(state.tab_history["/repo/wt/main"], ["design", "review"]);
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::SelectedChat {
            workspace: "/repo/wt/feature".into(),
            chat: None,
            recording_history: true,
        },
        1,
    )
    .unwrap();
    assert_eq!(state.selected_chats["/repo/wt/feature"], "notes");
    assert_eq!(
        navigation::selected_chat(&tree, &owner, &state, "/repo/wt/main")
            .unwrap()
            .id,
        "design"
    );
    state
        .selected_chats
        .insert("/repo/wt/main".into(), "notes".into());
    assert!(
        tree.workspace("/repo/wt/main")
            .unwrap()
            .1
            .chats
            .iter()
            .any(|c| c.id
                == navigation::selected_chat(&tree, &owner, &state, "/repo/wt/main")
                    .unwrap()
                    .id)
    );
}
#[test]
fn continued_tab_key_survives_session_change() {
    let mut current = session("next", "/repo/wt/main", 2);
    current["continuation_of"] = json!("old");
    let snap = snapshot(
        vec![session("old", "/repo/wt/main", 1), current],
        json!({"old":[agent("orchestrator","ended")],"next":[agent("orchestrator","ended")]}),
    );
    let tree = swarm::view::tree::build(&snap, &owner());
    let mut owner = owner();
    owner.tabs.insert("/repo/wt/main".into(), strip(&["old"]));
    let mut state = ViewState::default();
    state
        .selected_chats
        .insert("/repo/wt/main".into(), "old".into());
    state.selected_workspace = Some("/repo/wt/main".into());
    let view = tabs::view(&tree, &owner, &state, "/repo/wt/main", 1, &BTreeMap::new()).unwrap();
    assert_eq!(view.tabs[0].id, "old");
    assert_eq!(view.tabs[0].session.as_deref(), Some("next"));
    assert!(!view.tabs[0].can_close);
    assert!(view.tabs[0].can_hide);
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::SelectedChat {
            workspace: "/repo/wt/main".into(),
            chat: Some("next".into()),
            recording_history: true,
        },
        1,
    )
    .unwrap();
    assert_eq!(owner.tabs["/repo/wt/main"].open, ["old"]);
}

#[test]
fn a_chat_started_in_a_tab_stays_out_of_the_sidebar() {
    let tree = tree();
    let mut owner = owner();
    let mut state = ViewState::default();
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::OpenTab {
            workspace: "/repo/wt/main".into(),
            chat: "review".into(),
            tab_only: true,
        },
        1,
    )
    .unwrap();
    let rows: Vec<_> =
        swarm::view::tree::sections(&tree, &owner, &state, false, 1, &BTreeMap::new())
            .into_iter()
            .flat_map(|s| s.rows)
            .map(|r| r.id)
            .collect();
    assert!(rows.contains(&"chat:design".to_owned()));
    assert!(!rows.contains(&"chat:review".to_owned()));
    let view = tabs::view(&tree, &owner, &state, "/repo/wt/main", 1, &BTreeMap::new()).unwrap();
    let tab_only = |id: &str| view.tabs.iter().find(|t| t.id == id).unwrap().tab_only;
    assert!(tab_only("review"));
    assert!(!tab_only("design"));
}
