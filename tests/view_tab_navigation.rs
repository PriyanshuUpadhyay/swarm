mod sidebar_fixture;
mod tab_fixture;
use serde_json::json;
use sidebar_fixture::*;
use std::collections::BTreeMap;
use swarm::view::{
    choices::{self, ChoiceOp, ViewState},
    navigation,
    tabs::{self},
};
use tab_fixture::*;

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
fn unknown_agent_liveness_keeps_the_tab_when_leaving() {
    let mut unknown = agent("orchestrator", "working");
    unknown["alive"] = json!(null);
    let tree = swarm::view::tree::build(
        &snapshot(
            vec![
                session("design", "/repo/wt/main", 1),
                session("review", "/repo/wt/main", 2),
            ],
            json!({"design":[unknown, agent("worker", "ended")],"review":[agent("orchestrator", "waiting")]}),
        ),
        &owner(),
    );
    assert_eq!(
        tree.chat_in("/repo/wt/main", "design").unwrap().is_running,
        Some(false)
    );
    let mut owner = owner();
    owner
        .tabs
        .insert("/repo/wt/main".into(), strip(&["design", "review"]));
    let mut state = ViewState::default();
    for chat in ["design", "review"] {
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
    assert_eq!(owner.tabs["/repo/wt/main"].open, ["design", "review"]);
    assert_eq!(state.selected_chats["/repo/wt/main"], "review");
    assert_eq!(state.tab_history["/repo/wt/main"], ["design", "review"]);
}
#[test]
fn ended_tab_only_chat_keeps_the_tab_when_leaving() {
    let tree = tree();
    let mut owner = owner();
    owner
        .tabs
        .insert("/repo/wt/main".into(), strip(&["design", "review"]));
    let mut state = ViewState::default();
    choices::apply(
        &mut owner,
        &mut state,
        &tree,
        ChoiceOp::OpenTab {
            workspace: "/repo/wt/main".into(),
            chat: "ended".into(),
            tab_only: true,
        },
        1,
    )
    .unwrap();
    for chat in ["ended", "review"] {
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
    assert_eq!(
        owner.tabs["/repo/wt/main"].open,
        ["design", "review", "ended"]
    );
    assert!(owner.tab_chats.contains("ended"));
    assert_eq!(state.selected_chats["/repo/wt/main"], "review");
    assert_eq!(state.tab_history["/repo/wt/main"], ["ended", "review"]);
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
