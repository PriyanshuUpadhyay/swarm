mod sidebar_fixture;
use serde_json::{Value, json};
use sidebar_fixture::*;
use std::collections::BTreeMap;
use swarm::view::{
    choices::{self, ChoiceOp, OwnerChoices, ViewState},
    navigation,
    tree::{self, AgentStatus as Status},
};

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
#[test]
fn selection_uses_visible_parent_and_current_session() {
    let tree = tree();
    let owner = owner();
    let mut state = ViewState::default();
    state
        .selected_chats
        .insert("/repo/wt/main".into(), "design".into());
    let chat = tree.chat_in("/repo/wt/main", "design").unwrap();
    let rows = tree::sections(&tree, &owner, &state, false, 100, &Default::default())
        .into_iter()
        .flat_map(|s| s.rows)
        .collect::<Vec<_>>();
    assert_eq!(
        navigation::selected_row(
            &rows,
            Some("/repo/wt/main"),
            Some(chat),
            Some("child:old/worker")
        ),
        Some("chat:design".into())
    );
    state.fold("/repo/wt/main", true);
    let rows = tree::sections(&tree, &owner, &state, false, 100, &Default::default())
        .into_iter()
        .flat_map(|s| s.rows)
        .collect::<Vec<_>>();
    assert_eq!(
        navigation::selected_row(&rows, Some("/repo/wt/main"), Some(chat), None),
        Some("/repo/wt/main".into())
    );
}
#[test]
fn workspace_order_is_stable_natural_and_owner_set() {
    let tree = tree();
    let mut workspaces = tree.projects[0].workspaces.clone();
    workspaces[1].name = "task10".into();
    workspaces[2].name = "task2".into();
    tree::order_workspaces(&mut workspaces, &[], Some("/repo/wt/main"), "/repo");
    assert_eq!(
        workspaces
            .iter()
            .map(|w| w.name.as_str())
            .collect::<Vec<_>>(),
        ["main", "task2", "task10"]
    );
    tree::order_workspaces(
        &mut workspaces,
        &["/repo/wt/empty".into()],
        Some("/repo/wt/main"),
        "/repo",
    );
    assert_eq!(workspaces[0].path, "/repo/wt/empty");
}
#[test]
fn bare_hub_removed_and_worktree_states() {
    let mut snap = snapshot(
        vec![
            session("hub", "/repo", 1),
            session("gone-one", "/repo/wt/gone", 2),
            session("gone-two", "/repo/wt/gone/src", 3),
            session("main", "/repo/wt/main", 4),
        ],
        json!({}),
    );
    snap["sessions"][1]["folder_missing"] = json!(true);
    snap["sessions"][2]["folder_missing"] = json!(true);
    snap["projects"][0]["workspaces"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"path":"/repo/.bare","bare":true}),
            json!({"path":"/repo/wt/missing","branch":"missing","missing":true}),
            json!({"path":"/repo/wt/locked","branch":"locked","locked":true}),
            json!({"path":"/repo/wt/detached","detached":true}),
        ]);
    let tree = tree::build(&snap, &owner());
    let removed = tree.workspace("/repo#removed").unwrap().1;
    assert!(removed.removed);
    assert_eq!(removed.chats.len(), 2);
    assert_eq!(tree.workspace("/repo").unwrap().1.chats[0].id, "hub");
    assert!(tree.workspace("/repo/.bare").is_none());
    let rows = tree::sections(
        &tree,
        &owner(),
        &ViewState::default(),
        false,
        100,
        &Default::default(),
    )
    .remove(0)
    .rows;
    let row = |id| rows.iter().find(|r| r.id == id).unwrap();
    assert_eq!(row("/repo#removed").title, "Removed worktrees");
    assert!(!row("/repo#removed").missing);
    assert!(!row("/repo#removed").new_chat_enabled);
    assert!(row("/repo/wt/missing").detail.contains("folder missing"));
    assert!(!row("/repo/wt/missing").new_chat_enabled);
    assert!(row("/repo/wt/locked").detail.contains("locked"));
    assert!(row("/repo/wt/locked").new_chat_enabled);
    assert!(row("/repo/wt/detached").detail.contains("detached"));
}
#[test]
fn deepest_project_and_shared_path_ids() {
    let mut snap = snapshot(vec![session("main", "/repo/wt/main", 1)], json!({}));
    snap["projects"].as_array_mut().unwrap().extend([json!({"id":"folder:/repo","path":"/repo","launch_directory":"/repo","workspaces":[{"path":"/repo"}]}),json!({"id":"folder:/work","path":"/work","launch_directory":"/work","workspaces":[{"path":"/work"}]})]);
    let mut owner = owner();
    owner.project_paths.extend(["/repo".into(), "/work".into()]);
    let tree = tree::build(&snap, &owner);
    assert_eq!(
        tree.project_containing("/repo/wt/main/src").unwrap().id,
        "repository:/repo/.bare"
    );
    assert_eq!(
        tree.project_containing("/work/notes").unwrap().id,
        "folder:/work"
    );
    assert!(tree.project_containing("/elsewhere").is_none());
    let sections = tree::sections(
        &tree,
        &owner,
        &ViewState::default(),
        false,
        100,
        &Default::default(),
    );
    let ids: std::collections::BTreeSet<_> = sections.iter().map(|s| &s.id).collect();
    assert_eq!(ids.len(), sections.len());
}
#[test]
fn duplicate_project_names_and_saved_titles() {
    let snap = json!({"sessions":[],"agents":{},"projects":[{"id":"folder:/work/app","path":"/work/app","workspaces":[{"path":"/work/app"}]},{"id":"folder:/play/app","path":"/play/app","workspaces":[{"path":"/play/app"}]}]});
    let owner = OwnerChoices {
        project_paths: vec!["/play/app".into(), "/work/app".into()],
        ..Default::default()
    };
    let sections = sections(&snap, &owner, &ViewState::default(), false);
    assert_eq!(
        sections
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        ["app — play", "app — work"]
    );
}
#[test]
fn workspace_names_path_qualifiers_and_details() {
    let tree = tree();
    let project = &tree.projects[0];
    let workspace = &project.workspaces[0];
    let mut owner = owner();
    assert_eq!(
        navigation::workspace_title(project, workspace, &owner, false),
        "repo / main"
    );
    assert_eq!(
        navigation::workspace_title(project, workspace, &owner, true),
        "main"
    );
    assert_eq!(
        navigation::workspace_detail(project, workspace, &owner, false, &BTreeMap::new()),
        "3 chats"
    );
    owner
        .names
        .insert(workspace.path.clone(), " Fix login ".into());
    assert_eq!(
        navigation::workspace_title(project, workspace, &owner, false),
        "Fix login"
    );
    assert_eq!(
        navigation::workspace_detail(project, workspace, &owner, false, &BTreeMap::new()),
        "repo · main · 3 chats"
    );
    let ids = [(
        "fix login".into(),
        vec![workspace.path.clone(), "/repo/wt/review".into()],
    )]
    .into();
    assert_eq!(
        navigation::workspace_detail(project, workspace, &owner, false, &ids),
        "main · repo · 3 chats"
    );
    assert_eq!(
        navigation::path_qualifier(
            "/work/client-a/swarm/wt/main",
            &["/work/client-b/swarm/wt/main".into()]
        ),
        "client-a/swarm/wt/main"
    );
    assert_eq!(
        navigation::path_qualifier("/swarm/wt/main", &["/work/swarm/wt/main".into()]),
        "/swarm/wt/main"
    );
}
#[test]
fn independent_folds_and_case_keys() {
    let mut state = ViewState::default();
    assert!(state.is_collapsed("chat:chat"));
    state.fold("chat:chat", false);
    assert!(state.collapsed.contains("expanded:chat:chat"));
    state.fold("/repo", true);
    state.fold("project:/repo", true);
    assert!(state.collapsed.contains("workspace:/repo"));
    state.fold("project:/repo", false);
    assert!(state.is_collapsed("/repo"));
    assert!(!state.is_collapsed("project:/repo"));
    state.fold("/repo", false);
    assert!(!state.is_collapsed("/repo"));
    assert_eq!(
        navigation::title_key("Straße"),
        navigation::title_key("STRASSE")
    );
    assert_eq!(navigation::title_key("Docs"), navigation::title_key("docs"));
    assert_ne!(navigation::title_key("dogs"), navigation::title_key("docs"));
}

#[test]
fn a_chat_from_another_adapter_with_a_log_can_be_imported() {
    let mut herdr = session("herdr", "/repo", 1);
    herdr["adapter"] = json!("herdr");
    let mut no_log = session("nolog", "/repo", 2);
    no_log["adapter"] = json!("herdr");
    no_log["chair_log"] = Value::Null;
    let snap = folder_snapshot(
        vec![herdr, no_log],
        json!({"herdr":[agent("orchestrator","ended")],"nolog":[agent("orchestrator","ended")]}),
    );
    let mut owner = OwnerChoices::default();
    let tree = swarm::view::tree::build(&snap, &owner);
    // The project lists no chat, so it is not in the tree, but its chat can still be imported.
    assert!(tree.workspace("/repo").is_none());
    let importable: Vec<_> = tree
        .importable
        .iter()
        .map(|(workspace, c)| (workspace.as_str(), c.id.as_str()))
        .collect();
    assert_eq!(importable, [("/repo", "herdr")]);
    let mut state = ViewState::default();
    let import = |chat: &str| ChoiceOp::ImportChat {
        workspace: "/repo".into(),
        chat: chat.into(),
    };
    assert!(choices::apply(&mut owner, &mut state, &tree, import("nolog"), 1).is_err());
    choices::apply(&mut owner, &mut state, &tree, import("herdr"), 1).unwrap();
    let rows = sections(&snap, &owner, &ViewState::default(), false)
        .remove(0)
        .rows;
    assert!(rows.iter().any(|r| r.id == "chat:herdr"));

    // Once the chat is archived, the owner's choices forget its key; a chat still there keeps it.
    let key = tree.importable[0].1.key.clone();
    owner.tab_chats.insert(key.clone());
    let listed = swarm::view::tree::build(&snap, &owner);
    assert!(!owner.forget_gone_chats(&listed));
    assert!(owner.imported.contains(&key) && owner.tab_chats.contains(&key));
    let mut archived = snap.clone();
    archived["sessions"][0]["archivedAt"] = json!(42);
    let gone = swarm::view::tree::build(&archived, &owner);
    assert!(owner.forget_gone_chats(&gone));
    assert!(owner.imported.is_empty() && owner.tab_chats.is_empty());
}
#[test]
fn archived_chats_fold_in_the_last_row_of_their_project() {
    let mut snap = folder_snapshot(vec![session("live", "/repo", 5)], json!({}));
    snap["archived"] = json!([
        {"id":"old-chat-1","cwd":"/repo","workspace":"/repo","chair_provider":"claude","archivedAt":40,"title":"Skills work"},
        {"id":"old-chat-2","cwd":"/repo","workspace":"/repo","chair_provider":"pi","archivedAt":90},
        {"id":"elsewhere","cwd":"/other","workspace":"/other","chair_provider":"claude","archivedAt":95}
    ]);
    let mut owner = OwnerChoices::default();
    owner.project_paths.push("/repo".into());
    let mut state = ViewState::default();
    let rows = sections(&snap, &owner, &state, false).remove(0).rows;
    // Folded by default: one row that counts them, after the live chats.
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["/repo", "chat:live", "archived:project:/repo"]
    );
    let fold = rows.last().unwrap();
    assert_eq!(
        (fold.kind, fold.title.as_str()),
        ("archived", "Archived · 2")
    );
    assert!(!fold.expanded && fold.has_children && fold.selection.is_none());
    let tree = tree::build(&snap, &owner);
    let expand = |id: &str| ChoiceOp::Expand { id: id.into() };
    assert!(
        choices::apply(
            &mut owner.clone(),
            &mut ViewState::default(),
            &tree,
            expand("archived:/elsewhere"),
            0
        )
        .is_err()
    );
    choices::apply(
        &mut owner.clone(),
        &mut state,
        &tree,
        expand("archived:project:/repo"),
        0,
    )
    .unwrap();
    let rows = sections(&snap, &owner, &state, false).remove(0).rows;
    let archived: Vec<_> = rows.iter().filter(|r| r.kind == "archived-chat").collect();
    // Newest first; a chat with no title gets its provider and short id.
    assert_eq!(
        archived
            .iter()
            .map(|r| (r.id.as_str(), r.title.as_str()))
            .collect::<Vec<_>>(),
        [
            ("archived-chat:old-chat-2", "pi old-chat-2"),
            ("archived-chat:old-chat-1", "Skills work")
        ]
    );
    assert!(archived.iter().all(|r| r.depth == 1
        && r.dimmed
        && r.parent_id.as_deref() == Some("archived:project:/repo")));
}
#[test]
fn the_archived_fold_comes_after_every_workspace_and_names_each_chat_workspace() {
    let mut snap = snapshot(
        vec![
            session("hub", "/repo", 5),
            session("main", "/repo/wt/main", 6),
        ],
        json!({}),
    );
    snap["archived"] = json!([
        {"id":"old","workspace":"/repo","chair_provider":"pi","archivedAt":9,"title":"Old"},
        {"id":"newer","workspace":"/repo/wt/main","chair_provider":"pi","archivedAt":12,"title":"Newer"}
    ]);
    let owner = owner();
    let rows = sections(&snap, &owner, &ViewState::default(), false)
        .remove(0)
        .rows;
    let fold = rows.last().unwrap();
    assert_eq!(
        (
            fold.id.as_str(),
            fold.title.as_str(),
            fold.depth,
            fold.parent_id.as_deref()
        ),
        ("archived:project:/repo", "Archived · 2", 0, None)
    );
    assert!(rows.iter().filter(|r| r.kind == "archived").count() == 1);
    let mut state = ViewState::default();
    choices::apply(
        &mut owner.clone(),
        &mut state,
        &tree::build(&snap, &owner),
        ChoiceOp::Expand {
            id: "archived:project:/repo".into(),
        },
        0,
    )
    .unwrap();
    let rows = sections(&snap, &owner, &state, false).remove(0).rows;
    let titles: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == "archived-chat")
        .map(|r| (r.title.as_str(), r.detail.is_empty()))
        .collect();
    assert_eq!(titles, [("Newer", false), ("Old", false)]);
}
#[test]
fn an_archived_continuation_shows_the_name_given_to_its_first_session() {
    let mut snap = snapshot(vec![session("hub", "/repo", 5)], json!({}));
    snap["archived"] = json!([
        {"id":"next","continuation_of":"first","workspace":"/repo","chair_provider":"pi","archivedAt":9},
        {"id":"first","workspace":"/repo","chair_provider":"pi","archivedAt":8}
    ]);
    let mut owner = owner();
    owner.chat_names.insert("first".into(), "Named".into());
    let tree = tree::build(&snap, &owner);
    let archived = &tree
        .projects
        .iter()
        .flat_map(|p| &p.workspaces)
        .find(|w| w.path == "/repo")
        .unwrap()
        .archived;
    assert_eq!(
        archived
            .iter()
            .map(|c| (c.id.as_str(), c.title.as_str()))
            .collect::<Vec<_>>(),
        [("next", "Named"), ("first", "Named")]
    );
}

#[test]
fn two_chats_from_the_same_minute_get_ids_long_enough_to_differ() {
    let snapshot = json!({
        "sessions": [{"id": "01a14392-4044-7c82"}, {"id": "01a14392-5055-7ea2"}],
        "archived": [{"id": "01a14393-0000-7000"}],
    });
    assert_eq!(
        swarm::view::tree::short_id("01a14392-4044-7c82", &snapshot),
        "01a14392-4"
    );
    assert_eq!(
        swarm::view::tree::short_id("01a14393-0000-7000", &snapshot),
        "01a14393"
    );
}
