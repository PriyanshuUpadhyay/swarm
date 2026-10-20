//! Tests for server choice routes.

use super::*;
use crate::serve::test_support::*;
#[tokio::test]
async fn choices_handler_uses_shared_lane_rejects_stale_and_bumps_version() {
    let fixture = action_fixture();
    let workspace = env!("CARGO_MANIFEST_DIR");
    refresh_tree(&fixture.app).unwrap();
    let first = choices_store::load(&fixture.app.root, true).unwrap();
    let version = fixture.app.hub.lock().unwrap().version;
    let body: choices::ChoiceWrite = serde_json::from_value(json!({"revision": first.revision, "op":"rename_workspace", "workspace":workspace, "name":"New title"})).unwrap();
    assert!(Action::Choices(body.clone()).lane().is_none());
    let reply = choice_write(State(fixture.app.clone()), Ok(Json(body.clone())))
        .await
        .unwrap()
        .0;
    assert!(reply["version"].as_u64().unwrap() > version);
    let bytes = std::fs::read(fixture.app.root.join("choices.json")).unwrap();
    let stale = choice_write(State(fixture.app.clone()), Ok(Json(body)))
        .await
        .unwrap_err();
    assert_eq!(stale.0, StatusCode::CONFLICT);
    assert_eq!(
        std::fs::read(fixture.app.root.join("choices.json")).unwrap(),
        bytes
    );
    let Json(tree) = tree_view(State(fixture.app.clone()), Query(TreeQuery { view: None }))
        .await
        .unwrap();
    assert!(tree.to_string().contains("New title"));
    let Json(tabs) = tabs_view(
        State(fixture.app.clone()),
        Query(TabsQuery {
            workspace: workspace.into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(tabs["revision"], reply["revision"]);
    let version = fixture.app.hub.lock().unwrap().version;
    let modified = std::fs::metadata(fixture.app.root.join("choices.json"))
        .unwrap()
        .modified()
        .unwrap();
    let before_choices = choices_store::load(&fixture.app.root, true).unwrap().owner;
    refresh_tree(&fixture.app).unwrap();
    assert_eq!(
        choices_store::load(&fixture.app.root, true).unwrap().owner,
        before_choices
    );
    assert_eq!(
        std::fs::metadata(fixture.app.root.join("choices.json"))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    assert_eq!(fixture.app.hub.lock().unwrap().version, version);
}
#[tokio::test]
async fn an_archived_chat_folds_under_its_workspace_in_the_tree() {
    let fixture = Fixture::new();
    let mut connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    swarm::store::archive_sessions(&mut connection, std::slice::from_ref(&fixture.session))
        .unwrap();
    // A workspace with no live chat shows only when the owner opened its project.
    choices_store::update(&fixture.app.root, None, |owner, _| {
        owner.project_paths.push(env!("CARGO_MANIFEST_DIR").into());
        Ok(())
    })
    .unwrap();
    refresh_tree(&fixture.app).unwrap();
    let Json(tree) = tree_view(State(fixture.app.clone()), Query(TreeQuery { view: None }))
        .await
        .unwrap();
    let rows: Vec<_> = tree["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["rows"].as_array().unwrap())
        .collect();
    let fold = rows
        .iter()
        .find(|r| r["kind"] == "archived")
        .unwrap_or_else(|| panic!("no archived row in {tree}"));
    assert_eq!(fold["title"], "Archived · 1");
    assert!(!rows.iter().any(|r| r["kind"] == "chat"));
}
#[tokio::test]
async fn tree_handler_keeps_last_good_choices_on_busy_or_invalid_state() {
    let fixture = Fixture::new();
    refresh_tree(&fixture.app).unwrap();
    choices_store::update(&fixture.app.root, None, |owner, _| {
        owner
            .names
            .insert(env!("CARGO_MANIFEST_DIR").into(), "External name".into());
        Ok(())
    })
    .unwrap();
    let request = || tree_view(State(fixture.app.clone()), Query(TreeQuery { view: None }));
    assert!(
        request()
            .await
            .unwrap()
            .0
            .to_string()
            .contains("External name")
    );
    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(fixture.app.root.join("choices.lock"))
        .unwrap();
    held.lock().unwrap();
    let busy = request().await.unwrap().0;
    assert!(busy["choicesError"].is_string());
    assert!(busy.to_string().contains("External name"));
    drop(held);
    std::fs::write(fixture.app.root.join("view-state.json"), "{broken").unwrap();
    let broken = request().await.unwrap().0;
    assert!(broken["choicesError"].is_string());
    assert!(broken.to_string().contains("External name"));
    std::fs::remove_file(fixture.app.root.join("view-state.json")).unwrap();
    assert!(request().await.unwrap().0["choicesError"].is_null());
    std::fs::write(fixture.app.root.join("choices.json"), "{broken").unwrap();
    assert!(
        !request().await.unwrap().0["sections"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!fixture.app.root.join("choices.json").exists());
}
