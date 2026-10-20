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
