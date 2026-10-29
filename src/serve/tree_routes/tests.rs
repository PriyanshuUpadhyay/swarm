//! Tests for server tree routes.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn importable_route_lists_a_chat_from_another_adapter() {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    let listed = importable_chats(State(fixture.app.clone()))
        .await
        .unwrap()
        .0;
    assert_eq!(listed, json!({"chats": []}));
    connection
        .execute(
            "UPDATE session SET adapter = 'herdr' WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    let listed = importable_chats(State(fixture.app.clone()))
        .await
        .unwrap()
        .0;
    let chat = &listed["chats"][0];
    assert_eq!(chat["id"], fixture.session.as_str());
    assert_eq!(chat["workspace"], env!("CARGO_MANIFEST_DIR"));
    assert_eq!(chat["provider"], "claude");
}

#[tokio::test]
async fn tree_catalog_keeps_every_chat_when_sidebar_is_capped_or_folded() {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
    for i in 0..6 {
        let session = swarm::store::create_session(
            &connection,
            "lane",
            workspace,
            Some(("claude", &format!("catalog-{i}"))),
            Some("tmux-swarm"),
        )
        .unwrap();
        swarm::store::set_chair_log(&connection, &session, &fixture.log).unwrap();
        swarm::store::add_agent(&connection, &session, "orchestrator", "chat").unwrap();
    }
    let read = || tree_view(State(fixture.app.clone()), Query(TreeQuery { view: None }));
    let capped = read().await.unwrap().0;
    assert_eq!(capped["chats"].as_array().unwrap().len(), 7);
    let visible = capped["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["rows"].as_array().unwrap())
        .filter(|r| r["kind"] == "chat")
        .count();
    assert_eq!(visible, 5);
    choices_store::update(&fixture.app.root, None, |_, state| {
        state.fold(workspace.to_str().unwrap(), true);
        Ok(())
    })
    .unwrap();
    let folded = read().await.unwrap().0;
    assert_eq!(folded["chats"].as_array().unwrap().len(), 7);
    assert!(
        !folded["sections"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["rows"].as_array().unwrap())
            .any(|r| r["kind"] == "chat")
    );
}
