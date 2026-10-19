mod sidebar_fixture;
mod tab_fixture;
use serde_json::json;
use sidebar_fixture::*;
use std::collections::BTreeMap;
use swarm::view::{
    choices::{self, ChoiceOp, OwnerChoices, ViewState},
    tabs::{self},
    tree::AgentStatus,
};
use tab_fixture::*;

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
    let main_row = |owner: &OwnerChoices, state: &ViewState| {
        swarm::view::tree::sections(&tree, owner, state, false, 1, &BTreeMap::new())
            .into_iter()
            .flat_map(|s| s.rows)
            .find(|r| r.id == "/repo/wt/main")
            .unwrap()
    };
    // "design" is the first tab: the first open tab always shows in the sidebar.
    owner
        .tabs
        .insert("/repo/wt/main".into(), strip(&["design", "review"]));
    let before = main_row(&owner, &state);
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
    // The workspace row counts only the chats that the sidebar lists.
    let count = |detail: &str| -> usize {
        let part = detail.rsplit(" · ").next().unwrap();
        part.split(' ').next().unwrap().parse().unwrap()
    };
    let after = main_row(&owner, &state);
    assert_eq!(count(&after.detail), count(&before.detail) - 1);
    // Its status and badges skip the hidden chat too, so a waiting tab chat shows no waiting row.
    assert_eq!(after.status, Some(AgentStatus::Working));
    assert!(
        after
            .counts
            .iter()
            .all(|c| c.status != AgentStatus::Waiting)
    );
    let view = tabs::view(&tree, &owner, &state, "/repo/wt/main", 1, &BTreeMap::new()).unwrap();
    let tab_only = |id: &str| view.tabs.iter().find(|t| t.id == id).unwrap().tab_only;
    assert!(tab_only("review"));
    assert!(!tab_only("design"));
    // As the first open tab, the tab chat shows, so the workspace always has a row to open.
    let strip = owner.tabs.get_mut("/repo/wt/main").unwrap();
    let at = strip
        .open
        .iter()
        .position(|k| k.contains("review"))
        .unwrap();
    let key = strip.open.remove(at);
    strip.open.insert(0, key);
    let rows: Vec<_> =
        swarm::view::tree::sections(&tree, &owner, &state, false, 1, &BTreeMap::new())
            .into_iter()
            .flat_map(|s| s.rows)
            .map(|r| r.id)
            .collect();
    assert!(rows.contains(&"chat:review".to_owned()));
    // A live tab chat whose tab left the strip without an archive shows in the sidebar again.
    let strip = owner.tabs.get_mut("/repo/wt/main").unwrap();
    strip.open.retain(|k| !k.contains("review"));
    let rows: Vec<_> =
        swarm::view::tree::sections(&tree, &owner, &state, false, 1, &BTreeMap::new())
            .into_iter()
            .flat_map(|s| s.rows)
            .map(|r| r.id)
            .collect();
    assert!(rows.contains(&"chat:review".to_owned()));
    assert_eq!(main_row(&owner, &state).status, before.status);
}

#[test]
fn after_the_first_tab_closes_the_next_tab_chat_shows() {
    let tree = tree();
    let mut owner = owner();
    let mut state = ViewState::default();
    for chat in ["review", "design"] {
        let op = ChoiceOp::OpenTab {
            workspace: "/repo/wt/main".into(),
            chat: chat.into(),
            tab_only: true,
        };
        choices::apply(&mut owner, &mut state, &tree, op, 1).unwrap();
    }
    let rows = |owner: &OwnerChoices, state: &ViewState| -> Vec<String> {
        swarm::view::tree::sections(&tree, owner, state, false, 1, &BTreeMap::new())
            .into_iter()
            .flat_map(|s| s.rows)
            .map(|r| r.id)
            .collect()
    };
    let before = rows(&owner, &state);
    assert!(before.contains(&"chat:review".to_owned()));
    assert!(!before.contains(&"chat:design".to_owned()));
    let op = ChoiceOp::CloseTab {
        workspace: "/repo/wt/main".into(),
        chat: "review".into(),
    };
    choices::apply(&mut owner, &mut state, &tree, op, 1).unwrap();
    assert!(rows(&owner, &state).contains(&"chat:design".to_owned()));
}
