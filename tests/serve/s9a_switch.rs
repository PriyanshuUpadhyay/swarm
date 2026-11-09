//! G3 keeps the existing route guards, then checks the captured Claude journey.
use super::s9a_frozen as frozen;
use super::*;

mod captured {
    use super::*;
    struct Server(super::super::Server);
    impl std::ops::Deref for Server {
        type Target = super::super::Server;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl Server {
        fn new() -> Self {
            Self(super::super::Server::without_zig())
        }
    }
    include!("../s9a/routes_switch.rs");
}

pub(super) fn journey() {
    captured::switch_journey();
    let mut expected = frozen::expected(&["claude-chair-run/", "follow-claude-chair-run/"]);
    // Reopen must keep the already captured final page, with the same session and source ids.
    expected["follow-claude-chair-run/page-03-reopen.txt"] =
        expected["follow-claude-chair-run/page-02-final.txt"].clone();
    assert_eq!(captured::result(), expected);
}

#[test]
fn another_provider_still_uses_the_real_zig_tool() {
    let server = Server::new();
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET chair_provider = 'codex' WHERE id = ?1",
            [&server.session],
        )
        .unwrap();
    // HAND id follows events.rs:235, then state.rs:174 source binding.
    let text = "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"codex control\"}]}}\n";
    fs::write(&server.log, text).unwrap();
    let path = format!("/api/rows?session={}&agent=chair&tail=2000", server.session);
    let (status, _, body) = server.request("GET", &path, "", "", true);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json!({"status":status,"rows":body["rows"],"images":body["images"]}),
        json!({"status":200,"rows":[{"id":"event-raw-0-0@raw-0-0","kind":"user","text":"codex control","source_ids":["raw-0-0"],"order":[0,0]}],"images":{}})
    );
}
