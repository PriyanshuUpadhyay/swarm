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
fn sidebar_prioritizes_real_prompts_and_counts_every_hidden_project_chat() {
    let now = 700_000;
    let mut prompt = agent("orchestrator", "blocked");
    prompt["prompt"] =
        json!({"id":"choose","question":"Approve this change?","choices":["Yes","No"]});
    let mut sessions: Vec<_> = (1..=6)
        .map(|i| session(&format!("quiet-{i}"), "/repo/wt/main", now - i))
        .collect();
    sessions.extend([
        session("asks", "/repo/wt/feature", now - 9),
        session("quiet-wait", "/repo/wt/main", 1),
        session("old", "/repo/wt/main", 2),
    ]);
    let mut snap = snapshot(
        sessions,
        json!({
            "asks": [prompt], "quiet-wait": [agent("orchestrator", "blocked")],
        }),
    );
    snap["projects"].as_array_mut().unwrap().push(json!({
        "id":"folder:/notes","path":"/notes","launch_directory":"/notes",
        "workspaces":[{"path":"/notes"}],
    }));

    let mut owner = owner();
    owner.project_paths.push("/notes".into());
    let tree = swarm::view::tree::build(&snap, &owner);
    let mut state = ViewState::default();
    let show = |state: &ViewState| {
        swarm::view::tree::sections(&tree, &owner, state, false, now, &Default::default())
    };
    let display = show(&state);

    assert_eq!(
        display.iter().map(|s| s.kind).collect::<Vec<_>>(),
        ["needs-you", "project", "more-projects"]
    );
    assert_eq!(
        display[0]
            .rows
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        ["chat:asks"]
    );
    let project = &display[1].rows;
    assert!(project.iter().any(|r| r.id == "chat:quiet-wait"));
    assert!(
        !project
            .iter()
            .any(|r| r.id == "chat:asks" || r.id == "chat:old")
    );
    assert_eq!(project.last().unwrap().title, "… 2 more");

    state.expanded_lists.insert("project:/repo".into());
    assert!(show(&state)[1].rows.iter().any(|r| r.id == "chat:old"));
}

#[test]
fn needs_you_sends_one_cut_question_and_keeps_the_full_prompt() {
    use swarm::view::tree;
    let question = format!("Keep all of this? {}", "more ".repeat(45));
    let mut waiting = agent("orchestrator", "blocked");
    waiting["prompt"] = json!({"id":"choose","question":question,"choices":["Yes","No"]});
    let snap = snapshot(
        vec![session("ask", "/repo/wt/feature", 1)],
        json!({"ask":[waiting]}),
    );
    let mut owner = owner();
    owner.fields.chat.push("question".into());
    let tree = tree::build(&snap, &owner);
    let state = ViewState::default();
    let fields = BTreeMap::new();
    let display = tree::sections(&tree, &owner, &state, false, 100, &fields);
    let row = &display[0].rows[0];
    assert!(row.detail.starts_with("Keep all of this?") && row.detail.ends_with('…'));
    assert!(row.detail.chars().count() <= 80);
    assert!(!row.fields.iter().any(|f| f.field == "question"));
    let catalog = tree::chat_catalog(&tree, &owner, &state, 100, &fields);
    assert!(catalog[0].detail.is_empty());
    assert!(!catalog[0].fields.iter().any(|f| f.field == "question"));
    assert_eq!(
        tree.chats().next().unwrap().agents[0]["prompt"]["question"],
        question
    );
}

#[test]
fn needs_you_can_return_to_the_saved_five_chat_cut() {
    use swarm::view::tree;
    let mut waiting = agent("orchestrator", "blocked");
    waiting["prompt"] = json!({"id":"choose","question":"Approve?","choices":["Yes","No"]});
    let mut agents = serde_json::Map::new();
    let sessions = (0..6)
        .map(|i| {
            let id = format!("waiting-{i}");
            agents.insert(id.clone(), json!([waiting]));
            session(&id, "/repo/wt/main", i)
        })
        .collect();
    let mut owner = owner();
    let tree = tree::build(&snapshot(sessions, json!(agents)), &owner);
    let mut state = ViewState::default();
    state.expanded_lists.insert("needs-you".into());
    let expanded = tree::sections(&tree, &owner, &state, false, 100, &BTreeMap::new());
    assert!(
        expanded[0]
            .rows
            .last()
            .is_some_and(|r| r.kind == "more" && r.expanded)
    );
    let op = serde_json::from_value(json!({"op":"expand_list","id":"needs-you","expanded":false}))
        .unwrap();
    choices::apply(&mut owner, &mut state, &tree, op, 100).unwrap();
    assert!(!state.expanded_lists.contains("needs-you"));
    let cut = tree::sections(&tree, &owner, &state, false, 100, &BTreeMap::new());
    assert_eq!(cut[0].rows.last().unwrap().title, "… 1 more");
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
