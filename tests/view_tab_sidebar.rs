mod sidebar_fixture;
mod tab_fixture;
use serde_json::json;
use sidebar_fixture::*;
use std::collections::BTreeMap;
use swarm::view::{
    choices::{self, ChoiceOp, OwnerChoices, ViewState},
    tabs::{self},
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
fn every_chat_shows_in_the_sidebar_whether_or_not_its_tab_is_open() {
    let tree = tree();
    let mut owner = owner();
    let mut state = ViewState::default();
    let rows = |owner: &OwnerChoices, state: &ViewState| -> Vec<String> {
        swarm::view::tree::sections(&tree, owner, state, false, 1, &BTreeMap::new())
            .into_iter()
            .flat_map(|s| s.rows)
            .map(|r| r.id)
            .collect()
    };
    let before = rows(&owner, &state);
    assert!(before.contains(&"chat:design".to_owned()));
    assert!(before.contains(&"chat:review".to_owned()));
    // Opening, moving or closing a tab does not change the sidebar (ADR 0115).
    for chat in ["review", "design"] {
        let op = ChoiceOp::OpenTab {
            workspace: "/repo/wt/main".into(),
            chat: chat.into(),
        };
        choices::apply(&mut owner, &mut state, &tree, op, 1).unwrap();
    }
    assert_eq!(rows(&owner, &state), before);
    let op = ChoiceOp::CloseTab {
        workspace: "/repo/wt/main".into(),
        chat: "review".into(),
    };
    choices::apply(&mut owner, &mut state, &tree, op, 1).unwrap();
    assert_eq!(rows(&owner, &state), before);
}
