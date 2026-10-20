mod sidebar_fixture;
use serde_json::{Value, json};
use sidebar_fixture::*;
use std::collections::BTreeMap;
use swarm::view::{
    choices::{self, ChoiceOp, OwnerChoices, ViewState},
    navigation,
    tree::{self, AgentStatus as Status},
};

#[path = "view_tree/status.rs"]
mod status;
#[test]
fn repository_and_workspaces() {
    let tree = tree();
    assert_eq!(tree.projects.len(), 1);
    assert_eq!(tree.projects[0].name, "repo");
    assert_eq!(tree.projects[0].main_path.as_deref(), Some("/repo/wt/main"));
    assert_eq!(
        tree.projects[0]
            .workspaces
            .iter()
            .map(|w| w.name.as_str())
            .collect::<Vec<_>>(),
        ["main", "empty", "feature"]
    );
    assert_eq!(tree.workspace("/repo/wt/main").unwrap().1.chats.len(), 3);
    assert_eq!(tree.workspace("/repo/wt/feature").unwrap().1.chats.len(), 1);
    assert!(tree.workspace("/repo/wt/empty").unwrap().1.chats.is_empty());
    assert!(tree.chat_in("/repo/wt/main", "notes").is_none());
}
#[test]
fn nested_workspaces_have_one_owner() {
    let mut snapshot = snapshot(vec![session("feature", "/repo/wt/feature", 2)], json!({}));
    snapshot["projects"][0]["workspaces"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"/repo","branch":"main"}));
    assert_eq!(tree::build(&snapshot, &owner()).chats().count(), 1);
}
#[test]
fn folder_and_empty_projects() {
    let mut owner = OwnerChoices::default();
    owner.project_paths.push("/repo".into());
    let empty = tree::build(&folder_snapshot(Vec::new(), json!({})), &owner);
    assert_eq!(empty.projects.len(), 1);
    assert_eq!(empty.projects[0].workspaces.len(), 1);
    let folder = tree::build(
        &folder_snapshot(vec![session("plain", "/repo", 1)], json!({})),
        &OwnerChoices::default(),
    );
    assert_eq!(folder.projects[0].workspaces[0].name, "repo");
    assert_eq!(folder.projects[0].workspaces[0].chats[0].id, "plain");
    owner.removed_projects.insert("/repo".into());
    assert!(
        tree::build(
            &folder_snapshot(vec![session("plain", "/repo", 1)], json!({})),
            &owner
        )
        .projects
        .is_empty()
    );
}
#[test]
fn live_no_chair_ended_order() {
    let mut sessions = Vec::new();
    let mut agents = json!({});
    for (id, at, state, provider) in [
        ("ended-new", 50, "ended", Some("codex")),
        ("live-old", 10, "working", Some("codex")),
        ("ended-old", 20, "ended", Some("codex")),
        ("live-new", 30, "working", Some("codex")),
    ] {
        let mut s = session(id, "/repo", at);
        s["chair_provider"] = json!(provider);
        sessions.push(s);
        let mut a = agent("orchestrator", state);
        a["provider"] = json!(provider);
        agents[id] = json!([a]);
    }
    let tree = tree::build(&folder_snapshot(sessions, agents), &OwnerChoices::default());
    assert_eq!(
        tree.chats().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["live-new", "live-old", "ended-new", "ended-old"]
    );
}
#[test]
fn provider_and_title_fallback() {
    let mut s = session("provider-session", "/repo", 1);
    s["title"] = Value::Null;
    let snapshot = folder_snapshot(
        vec![s],
        json!({"provider-session":[agent("orchestrator","working")]}),
    );
    let tree = tree::build(&snapshot, &OwnerChoices::default());
    let c = tree.chats().next().unwrap();
    assert_eq!(c.provider.as_deref(), Some("codex"));
    assert_eq!(c.title, "codex provider");
}
#[test]
fn resolved_names_and_titles() {
    let mut s = session("chat", "/repo", 1);
    s["title"] = json!("Fix menu");
    s["cli_name"] = json!(" CLI title ");
    let snap = folder_snapshot(vec![s], json!({}));
    let mut owner = OwnerChoices::default();
    assert_eq!(
        tree::build(&snap, &owner).chats().next().unwrap().title,
        "CLI title"
    );
    owner.chat_names.insert("chat".into(), " App name ".into());
    assert_eq!(
        tree::build(&snap, &owner).chats().next().unwrap().title,
        "App name"
    );
    owner.chat_names.insert("chat".into(), "  ".into());
    assert_eq!(
        tree::build(&snap, &owner).chats().next().unwrap().title,
        "CLI title"
    );
}
#[test]
fn row_ages() {
    for (timestamp, now, expected) in [
        (1, 7201, "2h"),
        (100, 99, "0s"),
        (1, 61, "1m"),
        (1, 86401, "1d"),
        (0, 59, "59s"),
    ] {
        assert_eq!(tree::age(timestamp, now), expected);
    }
}
#[test]
fn empty_and_archived_sessions() {
    let mut archived = session("archived", "/repo/wt/feature", 2);
    archived["archivedAt"] = json!(50);
    let snap = snapshot(
        vec![session("active", "/repo/wt/main", 1), archived],
        json!({"active":[agent("worker","done")]}),
    );
    let tree = tree::build(&snap, &OwnerChoices::default());
    assert_eq!(tree.projects[0].workspaces.len(), 1);
    assert!(tree.chat_in("/repo/wt/feature", "archived").is_none());
    let tree = tree::build(
        &folder_snapshot(vec![session("empty", "/repo", 1)], json!({"empty":[]})),
        &OwnerChoices::default(),
    );
    assert!(tree.projects.is_empty());
}
#[test]
fn chair_and_continuation_groups() {
    let mut old = session("older", "/repo", 1);
    old["chair_id"] = json!("same-chair");
    old["title"] = json!("Fix menu");
    old["last_message_at"] = json!(100);
    let mut next = session("next", "/repo", 50);
    next["chair_id"] = json!("same-chair");
    let snap = folder_snapshot(vec![old.clone(), next.clone()], json!({}));
    let tree = tree::build(&snap, &OwnerChoices::default());
    let c = tree.chats().next().unwrap();
    assert_eq!(tree.chats().count(), 1);
    assert_eq!(c.id, "next");
    assert_eq!(c.key, "older");
    assert!(c.contains("older"));
    assert_eq!(c.last_activity, 100);
    assert_eq!(c.title, "Fix menu");
    next["chair_provider"] = json!("claude");
    next["chair_id"] = json!("new-chair");
    next["continuation_of"] = json!("older");
    let tree = tree::build(
        &folder_snapshot(vec![old, next], json!({})),
        &OwnerChoices::default(),
    );
    assert_eq!(tree.chats().count(), 1);
    assert_eq!(tree.chats().next().unwrap().sessions[0]["id"], "next");
}
#[test]
fn cycle_in_continuations_is_bounded() {
    let mut first = session("first", "/repo", 1);
    let mut second = session("second", "/repo", 2);
    first["continuation_of"] = json!("second");
    second["continuation_of"] = json!("first");
    assert!(
        tree::build(
            &folder_snapshot(vec![first, second], json!({})),
            &OwnerChoices::default()
        )
        .chats()
        .count()
            <= 2
    );
}
#[test]
fn chat_cut_and_order() {
    let snap = folder_snapshot(
        (1..=7)
            .map(|i| session(&format!("s{i}"), "/repo", i))
            .collect(),
        json!({}),
    );
    let mut owner = OwnerChoices::default();
    owner.project_paths.push("/repo".into());
    let mut state = ViewState::default();
    let rows = sections(&snap, &owner, &state, false).remove(0).rows;
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        [
            "/repo",
            "chat:s7",
            "chat:s6",
            "chat:s5",
            "chat:s4",
            "chat:s3",
            "more:/repo"
        ]
    );
    assert_eq!(rows.last().unwrap().title, "… 2 more");
    assert_eq!(
        rows.iter().map(|r| r.depth).collect::<Vec<_>>(),
        [0, 1, 1, 1, 1, 1, 1]
    );
    assert!(
        rows[1..]
            .iter()
            .all(|r| r.parent_id.as_deref() == Some("/repo"))
    );
    state.expanded_lists.insert("/repo".into());
    assert_eq!(sections(&snap, &owner, &state, false)[0].rows.len(), 8);
    state.expanded_lists = ["/other".into()].into();
    assert_eq!(sections(&snap, &owner, &state, false)[0].rows.len(), 7);
}
#[test]
fn child_folds_and_hides_ended_children() {
    let snap = folder_snapshot(
        vec![session("chat", "/repo", 1)],
        json!({"chat":[agent("orchestrator","working"),agent("review","waiting"),json!({"id":"code","role":"code","pane":null,"alive":false,"state":"done"})]}),
    );
    let owner = OwnerChoices::default();
    let mut state = ViewState::default();
    let folded = sections(&snap, &owner, &state, false).remove(0).rows;
    assert_eq!(folded.len(), 2);
    assert!(!folded[1].expanded);
    assert_eq!(
        folded[1].children_summary.as_deref(),
        Some("1 agent · 1 waiting")
    );
    assert_eq!(folded[1].status, Some(Status::Waiting));
    state.fold("chat:chat", false);
    let opened = sections(&snap, &owner, &state, false).remove(0).rows;
    assert_eq!(
        opened.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["/repo", "chat:chat", "child:chat/review"]
    );
    assert_eq!(opened[1].status, Some(Status::Waiting));
    assert!(!opened[2].dimmed);
    assert_eq!(opened[2].detail, "· code");
    state.fold("/repo", true);
    let folded = sections(&snap, &owner, &state, false).remove(0).rows;
    assert_eq!(folded.len(), 1);
    assert_eq!(folded[0].status, Some(Status::Waiting));
}
#[test]
fn an_open_chat_is_done_only_when_its_chair_and_every_child_are_done() {
    let owner = OwnerChoices::default();
    let mut state = ViewState::default();
    state.fold("chat:chat", false);
    for (child, expected) in [("working", Status::Working), ("done", Status::Done)] {
        let snap = folder_snapshot(
            vec![session("chat", "/repo", 1)],
            json!({"chat":[agent("orchestrator","done"),agent("review",child)]}),
        );
        let rows = sections(&snap, &owner, &state, false).remove(0).rows;
        assert_eq!(rows[1].id, "chat:chat");
        assert!(rows[1].expanded);
        assert_eq!(rows[1].status, Some(expected));
    }
}
#[test]
fn hides_a_closed_chat_with_no_log() {
    let mut closed = session("closed", "/repo", 1);
    closed["chair_log"] = Value::Null;
    let mut starting = session("starting", "/repo", 2);
    starting["chair_log"] = Value::Null;
    let snap = folder_snapshot(
        vec![closed, starting, session("logged", "/repo", 3)],
        json!({"closed":[agent("orchestrator","ended")],"starting":[agent("orchestrator","working")],"logged":[agent("orchestrator","ended")]}),
    );
    let rows = sections(
        &snap,
        &OwnerChoices::default(),
        &ViewState::default(),
        false,
    )
    .remove(0)
    .rows;
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["/repo", "chat:logged", "chat:starting"]
    );
}
#[test]
fn lists_only_chats_that_the_app_started() {
    // A lane that a CLI in a Herdr or tmux pane opened is not the app's chat.
    let mut herdr = session("herdr", "/repo", 1);
    herdr["adapter"] = json!("herdr");
    let snap = folder_snapshot(
        vec![herdr, session("app", "/repo", 2)],
        json!({"herdr":[agent("orchestrator","working")],"app":[agent("orchestrator","working")]}),
    );
    let rows = sections(
        &snap,
        &OwnerChoices::default(),
        &ViewState::default(),
        false,
    )
    .remove(0)
    .rows;
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["/repo", "chat:app"]
    );
}
#[test]
fn hides_a_live_chat_whose_log_swarm_cannot_read() {
    // A lane that a pi chair opened names no chair CLI, so its log can never be read.
    let mut lane = session("lane", "/repo", 1);
    lane["chair_provider"] = Value::Null;
    let snap = folder_snapshot(
        vec![lane, session("logged", "/repo", 2)],
        json!({"lane":[agent("orchestrator","working"),agent("worker","working")],"logged":[agent("orchestrator","ended")]}),
    );
    let owner = OwnerChoices::default();
    let rows = sections(&snap, &owner, &ViewState::default(), false)
        .remove(0)
        .rows;
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["/repo", "chat:logged"]
    );
    // The workspace still holds it, so a workspace delete closes its live agents.
    let tree = tree::build(&snap, &owner);
    let workspace = &tree.projects[0].workspaces[0];
    assert_eq!(
        workspace
            .unlisted
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["lane"]
    );
}
#[test]
fn stable_chain_child_ids_and_current_urgency() {
    let old = session("root", "/repo", 1);
    let mut next = session("next", "/repo", 2);
    next["continuation_of"] = json!("root");
    let mut state = ViewState::default();
    state.fold("chat:root", false);
    for older_status in ["waiting", "failed"] {
        let snap = folder_snapshot(
            vec![old.clone(), next.clone()],
            json!({"root":[agent("previous-only",older_status)],"next":[agent("orchestrator","working"),agent("worker","done"),agent("finished","ended")]}),
        );
        let rows = sections(&snap, &OwnerChoices::default(), &state, false)
            .remove(0)
            .rows;
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["/repo", "chat:root", "child:next/worker"]
        );
        assert_eq!(rows[0].status, Some(Status::Working));
        assert_eq!(
            rows[2].selection.as_ref().unwrap().agent_session.as_deref(),
            Some("next")
        );
        assert!(rows[2].dimmed);
    }
    let rows = sections(
        &folder_snapshot(
            vec![old, next],
            json!({"root":[agent("previous-only","waiting")]}),
        ),
        &OwnerChoices::default(),
        &state,
        false,
    )
    .remove(0)
    .rows;
    assert_eq!(rows.len(), 2);
    assert!(!rows[1].has_children);
    assert!(!rows[1].fields.iter().any(|f| f.field == "children"));
}
#[test]
fn child_count_without_waiting() {
    for (agents, label) in [
        (vec![agent("worker", "working")], "1 agent"),
        (
            vec![agent("one", "done"), agent("two", "working")],
            "2 agents",
        ),
        (vec![agent("worker", "waiting")], "1 agent · 1 waiting"),
    ] {
        let rows = sections(
            &folder_snapshot(vec![session("chat", "/repo", 1)], json!({"chat":agents})),
            &OwnerChoices::default(),
            &ViewState::default(),
            false,
        )
        .remove(0)
        .rows;
        assert_eq!(rows[1].children_summary.as_deref(), Some(label));
    }
}
#[test]
fn pinned_archived_and_empty_section_visibility() {
    let snap = folder_snapshot(vec![session("one", "/repo", 1)], json!({}));
    let mut owner = OwnerChoices::default();
    let state = ViewState::default();
    assert_eq!(sections(&snap, &owner, &state, false)[0].kind, "project");
    owner.pinned.insert("/repo".into());
    let pinned = sections(&snap, &owner, &state, false);
    assert_eq!(pinned[0].kind, "pinned");
    assert_eq!(pinned[0].rows.len(), 2);
    assert!(pinned[1].rows.is_empty());
    owner.archived.insert("/repo".into());
    let active = sections(&snap, &owner, &state, false);
    assert_eq!(active.len(), 1);
    assert!(active[0].rows.is_empty());
    let archive = sections(&snap, &owner, &state, true);
    assert_eq!(archive.len(), 1);
    assert!(archive[0].rows.iter().all(|r| r.archived));
    assert!(!archive[0].rows[0].new_chat_enabled);
    owner.archived.clear();
    owner.pinned = ["/missing".into()].into();
    assert!(
        !sections(&snap, &owner, &state, false)
            .iter()
            .any(|s| s.kind == "pinned")
    );
}
#[test]
fn urgent_collapsed_headers_and_counts() {
    let snap = folder_snapshot(
        vec![session("chat", "/repo", 1)],
        json!({"chat":[agent("coder","working"),agent("tester","working"),agent("reviewer","waiting"),agent("old","ended")]}),
    );
    let mut state = ViewState::default();
    state.fold("project:/repo", true);
    let section = sections(&snap, &OwnerChoices::default(), &state, false).remove(0);
    assert!(!section.expanded);
    assert_eq!(section.status, Some(Status::Waiting));
    assert_eq!(
        section.rows[0]
            .counts
            .iter()
            .map(|c| (c.status, c.count))
            .collect::<Vec<_>>(),
        [(Status::Waiting, 1), (Status::Working, 2)]
    );
}
#[path = "view_tree/archive.rs"]
mod archive;
#[path = "view_tree/workspace_navigation.rs"]
mod workspace_navigation;
