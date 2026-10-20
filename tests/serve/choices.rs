// Tests for choices.

use super::*;

fn tree_choices(server: &Server) -> Value {
    let (status, _, body) = server.request("GET", "/api/tree?view=workspaces", "", "", true);
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

fn write_choices(server: &Server, mut operation: Value) -> Value {
    for _ in 0..5 {
        operation["revision"] = tree_choices(server)["revision"].clone();
        let (status, _, body) = server.request(
            "POST",
            "/api/choices",
            "Content-Type: application/json\r\n",
            &operation.to_string(),
            true,
        );
        if status == 409 {
            continue;
        }
        assert_eq!(status, 200, "{body}");
        return serde_json::from_str(&body).unwrap();
    }
    panic!("choices did not settle after discovery");
}
#[test]
fn stale_choices_http_changes_nothing_and_routes_are_guarded() {
    let server = Server::new();
    let workspace = env!("CARGO_MANIFEST_DIR");
    assert_eq!(server.request("GET", "/api/tree", "", "", false).0, 401);
    assert_eq!(
        server
            .request("GET", "/api/tabs?workspace=x", "", "", false)
            .0,
        401
    );
    assert_eq!(
        server
            .request(
                "POST",
                "/api/choices",
                "Content-Type: application/json\r\n",
                "{}",
                false
            )
            .0,
        401
    );
    assert_eq!(
        server
            .request("GET", "/api/tree?view=other", "", "", true)
            .0,
        400
    );
    let old = tree_choices(&server)["revision"].clone();
    write_choices(
        &server,
        json!({"op":"rename_workspace","workspace":workspace,"name":"Saved"}),
    );
    let root = server.dir.join("home/.swarm");
    let choices = fs::read(root.join("choices.json")).unwrap();
    let state = fs::read(root.join("view-state.json")).ok();
    let (status, _, body) = server.request(
        "POST",
        "/api/choices",
        "Content-Type: application/json\r\n",
        &json!({"revision":old,"op":"rename_workspace","workspace":workspace,"name":"Stale"})
            .to_string(),
        true,
    );
    assert_eq!(status, 409, "{body}");
    assert_eq!(fs::read(root.join("choices.json")).unwrap(), choices);
    assert_eq!(fs::read(root.join("view-state.json")).ok(), state);
    assert_eq!(
        server
            .request("GET", "/api/tabs?workspace=%2Fmissing", "", "", true)
            .0,
        404
    );
    assert_eq!(
        server
            .request("GET", "/api/tree?view=archived", "", "", true)
            .0,
        200
    );
}
#[test]
fn choices_external_refresh_busy_and_corrupt_state_keep_tree_visible() {
    let server = Server::new();
    let workspace = env!("CARGO_MANIFEST_DIR");
    let first = tree_choices(&server);
    let root = server.dir.join("home/.swarm");
    let mut owner: Value = fs::read(root.join("choices.json"))
        .ok()
        .map(|b| serde_json::from_slice(&b).unwrap())
        .unwrap_or(json!({}));
    owner["names"] = json!({workspace:"External name"});
    fs::write(root.join("choices.json"), owner.to_string()).unwrap();
    let second = tree_choices(&server);
    assert_ne!(first["revision"], second["revision"]);
    assert!(second.to_string().contains("External name"));
    let held = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("choices.lock"))
        .unwrap();
    held.lock().unwrap();
    let busy = tree_choices(&server);
    assert!(busy["choicesError"].as_str().is_some());
    assert!(busy.to_string().contains("External name"));
    drop(held);
    fs::write(root.join("view-state.json"), "{broken").unwrap();
    let failed = tree_choices(&server);
    assert!(failed["choicesError"].as_str().is_some());
    assert!(failed.to_string().contains("External name"));
}
