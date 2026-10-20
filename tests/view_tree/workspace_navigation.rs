// Workspace navigation and display tests.
use super::*;

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
