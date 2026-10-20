// Chat import and archive display tests.
use super::*;

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
