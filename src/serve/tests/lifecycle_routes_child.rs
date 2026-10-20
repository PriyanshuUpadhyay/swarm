/// Self-reexecuting server test.
#[tokio::test]
async fn lifecycle_routes_child() {
    if std::env::var("SWARM_LIFECYCLE_ROUTES").is_err() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let root = PathBuf::from(std::env::var("HOME").unwrap());
    let fixture = action_fixture_at(Fixture::at(root.join("surface")));
    let source = root.join("project").to_string_lossy().into_owned();
    let plain = root.join("plain").to_string_lossy().into_owned();
    std::fs::create_dir(&plain).unwrap();
    // Remove the inherited source session so this surface contains only fixture projects.
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET cwd = ?1, chair_log = NULL WHERE id = ?2",
            (&plain, &fixture.session),
        )
        .unwrap();
    fixture.app.directories.lock().unwrap().clear();
    let create = life_post(
        &fixture,
        "/api/project/create",
        json!({"path":source,"name":"Project title"}),
    )
    .await
    .unwrap();
    assert_eq!(create["firstCommit"]["state"], "made");
    assert_eq!(
        lifecycle::check_git(&source, &["log", "--format=%s"]).unwrap(),
        "Create project"
    );
    assert_eq!(
        choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .project_names[&source],
        "Project title"
    );
    let exists = life_post(
        &fixture,
        "/api/project/create",
        json!({"path":source,"name":"Exists"}),
    )
    .await
    .unwrap_err();
    assert_eq!(exists.0, StatusCode::CONFLICT);
    let options = lifecycle_read(
        State(fixture.app.clone()),
        "/api/workspace/new-options".parse().unwrap(),
        Query(HashMap::from([("project".into(), source.clone())])),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(options["defaultBranch"], "master"); // The isolated Git has no init.defaultBranch override.
    let base = options["defaultBranch"].as_str().unwrap();
    let made=life_post(&fixture,"/api/workspace/new",json!({"project":source,"name":"Feature","mode":"new-branch","base":base,"prefix":"","folder":"../tasks"})).await.unwrap();
    let path = made["workspace"].as_str().unwrap().to_owned();
    assert_eq!(
        lifecycle::check_git(&path, &["branch", "--show-current"]).unwrap(),
        "feature"
    );
    let initial_version = made["version"].as_u64().unwrap();
    let launch = call_action(&fixture, "/api/chat/new", json!({"cwd":path}))
        .await
        .unwrap()
        .0;
    assert_eq!(launch["ok"], true); // No session existed in the new workspace before this call.
    let impact = lifecycle_read(
        State(fixture.app.clone()),
        "/api/workspace/impact".parse().unwrap(),
        Query(HashMap::from([("workspace".into(), path.clone())])),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(impact["canDelete"], true);
    assert_eq!(impact["liveAgents"], 0);
    assert!(
        impact["deleteBlocker"]
            .as_str()
            .unwrap()
            .contains("unpushed")
    );
    life_post(
        &fixture,
        "/api/workspace/archive",
        json!({"workspace":path}),
    )
    .await
    .unwrap();
    assert!(
        choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .archived
            .contains(&path)
    );
    assert!(Path::new(&path).is_dir());
    assert_eq!(
        call_action(&fixture, "/api/chat/new", json!({"cwd":path}))
            .await
            .unwrap_err()
            .0,
        StatusCode::CONFLICT
    );
    life_post(
        &fixture,
        "/api/workspace/restore",
        json!({"workspace":path}),
    )
    .await
    .unwrap();
    assert!(
        !choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .archived
            .contains(&path)
    );
    assert!(fixture.app.hub.lock().unwrap().version > initial_version);
    // An unanswered import returns the choice without saving its answer.
    let ask = life_post(&fixture, "/api/project/import", json!({"path":plain}))
        .await
        .unwrap();
    assert_eq!(ask["needsChoice"], true);
    assert!(
        !choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .plain_folders
            .contains(&plain)
    );
    life_post(
        &fixture,
        "/api/project/import",
        json!({"path":plain,"as":"folder"}),
    )
    .await
    .unwrap();
    assert_eq!(
        life_post(&fixture, "/api/project/import", json!({"path":plain}))
            .await
            .unwrap()["needsChoice"],
        false
    );
    assert!(!Path::new(&plain).join(".git").exists());
    life_post(
        &fixture,
        "/api/project/import",
        json!({"path":plain,"as":"git"}),
    )
    .await
    .unwrap();
    assert!(Path::new(&plain).join(".git").exists());
    assert!(
        !choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .plain_folders
            .contains(&plain)
    );
    // Dirty and unpushed work must remain before any agent close.
    let marker = Path::new(&path).join("keep.txt");
    std::fs::write(&marker, "keep").unwrap();
    let calls = action_calls(&fixture);
    assert_eq!(
        life_post(&fixture, "/api/workspace/delete", json!({"workspace":path}))
            .await
            .unwrap_err()
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(action_calls(&fixture), calls);
    assert!(marker.exists());
    std::fs::remove_file(marker).unwrap();
    assert!(
        life_post(&fixture, "/api/workspace/delete", json!({"workspace":path}))
            .await
            .is_err()
    );
    lifecycle::check_git(&source, &["update-ref", "refs/remotes/origin/main", "HEAD"]).unwrap();
    let deleted = life_post(&fixture, "/api/workspace/delete", json!({"workspace":path}))
        .await
        .unwrap();
    assert_eq!(deleted["ok"], true);
    assert!(!Path::new(&path).exists());
    let missing = life_post(
        &fixture,
        "/api/workspace/new",
        json!({"project":source,"name":"Missing","mode":"new-branch","base":base}),
    )
    .await
    .unwrap()["workspace"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::remove_dir_all(&missing).unwrap();
    life_post(
        &fixture,
        "/api/workspace/prune",
        json!({"workspace":missing}),
    )
    .await
    .unwrap();
    assert!(
        !lifecycle::check_git(&source, &["worktree", "list", "--porcelain"])
            .unwrap()
            .contains(&missing)
    );
    // A full fresh listing supplies both child and chair live counts before closing.
    connection
        .execute(
            "UPDATE session SET cwd = ?1 WHERE id = ?2",
            (&source, &fixture.session),
        )
        .unwrap();
    let cli = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
'agents --json') printf '%s\n' '{{"{id}":{{"agents":[{{"id":"orchestrator","alive":true}},{{"id":"worker","alive":true}}]}}}}' ;;
'close worker') if [ -f "$HOME/change-during-close" ]; then printf keep > "$PWD/new-work.txt"; fi ;;
esac
"#,
        id = fixture.session
    );
    std::fs::write(&fixture.app.cli, cli).unwrap();
    std::fs::set_permissions(&fixture.app.cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let impact = lifecycle_read(
        State(fixture.app.clone()),
        "/api/workspace/impact".parse().unwrap(),
        Query(HashMap::from([("workspace".into(), source.clone())])),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(impact["liveAgents"], 2);
    assert_eq!(impact["confirmationRequired"], true);
    life_post(
        &fixture,
        "/api/workspace/archive",
        json!({"workspace":source}),
    )
    .await
    .unwrap();
    let calls = action_calls(&fixture);
    assert!(calls.rfind("close worker").unwrap() < calls.rfind("close orchestrator").unwrap());
    life_post(
        &fixture,
        "/api/workspace/restore",
        json!({"workspace":source}),
    )
    .await
    .unwrap();
    let clean = life_post(
        &fixture,
        "/api/workspace/new",
        json!({"project":source,"name":"Clean","mode":"new-branch","base":base}),
    )
    .await
    .unwrap()["workspace"]
        .as_str()
        .unwrap()
        .to_owned();
    connection
        .execute(
            "UPDATE session SET cwd = ?1 WHERE id = ?2",
            (&clean, &fixture.session),
        )
        .unwrap();
    std::fs::write(root.join("change-during-close"), "").unwrap();
    assert_eq!(
        life_post(
            &fixture,
            "/api/workspace/delete",
            json!({"workspace":clean})
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        std::fs::read_to_string(Path::new(&clean).join("new-work.txt")).unwrap(),
        "keep"
    );
    std::fs::remove_file(root.join("change-during-close")).unwrap();
    let before = choices_store::load(&fixture.app.root, true).unwrap();
    for route in ["new", "archive", "restore", "delete", "prune"] {
        let value = if route == "new" {
            json!({"project":root.to_string_lossy(),"name":"Outside","mode":"new-branch"})
        } else {
            json!({"workspace":root.to_string_lossy()})
        };
        assert_eq!(
            life_post(&fixture, &format!("/api/workspace/{route}"), value)
                .await
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        life_post(
            &fixture,
            "/api/project/remove",
            json!({"project":root.to_string_lossy()})
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::FORBIDDEN
    );
    std::os::unix::fs::symlink(&fixture.app.root, root.join("home-link")).unwrap();
    for (route, value) in [
        (
            "create",
            json!({"path":root.join("home-link/escape").to_string_lossy(),"name":"Escape"}),
        ),
        (
            "import",
            json!({"path":root.join("home-link").to_string_lossy(),"as":"git"}),
        ),
    ] {
        assert_eq!(
            life_post(&fixture, &format!("/api/project/{route}"), value)
                .await
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
    }
    let after = choices_store::load(&fixture.app.root, true).unwrap();
    assert_eq!(after.revision, before.revision);
    assert!(!fixture.app.root.join("escape").exists());
    life_post(&fixture, "/api/project/remove", json!({"project":source}))
        .await
        .unwrap();
    assert!(Path::new(&source).is_dir());
    assert!(
        choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .removed_projects
            .contains(&source)
    );
    life_post(&fixture, "/api/project/import", json!({"path":clean}))
        .await
        .unwrap();
    assert!(
        !choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .removed_projects
            .contains(&source)
    );
    // A choices failure removes the empty new folder before any Git init.
    let (tree, saved) = fresh_lifecycle_tree(&fixture.app).unwrap();
    let target = root.join("choices-failed").to_string_lossy().into_owned();
    let marker = fixture.app.root.join("swarm-home");
    std::fs::rename(&marker, fixture.app.root.join("marker.saved")).unwrap();
    assert!(
        apply_lifecycle(
            &fixture.app,
            LifecycleAction::Create(lifecycle::CreateProject {
                path: target.clone(),
                name: "Failed title".into()
            }),
            &tree,
            &saved,
            0
        )
        .is_err()
    );
    assert!(!Path::new(&target).exists());
    std::fs::rename(fixture.app.root.join("marker.saved"), marker).unwrap();
    // Git init failures remove only newly remembered paths and retain the first error.
    std::fs::write(root.join("fail-init"), "").unwrap();
    let remembered = root.join("remembered").to_string_lossy().into_owned();
    choices_store::update(&fixture.app.root, None, |owner, _| {
        owner.project_paths.push(remembered.clone());
        Ok(())
    })
    .unwrap();
    for path in [target.clone(), remembered.clone()] {
        let failure = life_post(
            &fixture,
            "/api/project/create",
            json!({"path":path,"name":"Failed title"}),
        )
        .await
        .unwrap_err();
        assert!(
            failure.1.0["error"]
                .as_str()
                .unwrap()
                .contains("Fixture git init refused")
        );
        assert!(!Path::new(&path).exists());
    }
    let owner = choices_store::load(&fixture.app.root, true).unwrap().owner;
    assert!(!owner.project_paths.contains(&target));
    assert!(!owner.project_names.contains_key(&target));
    assert!(owner.project_paths.contains(&remembered));
    std::fs::write(
        root.join("fail-rollback"),
        fixture.app.root.to_string_lossy().as_bytes(),
    )
    .unwrap();
    let failure = life_post(
        &fixture,
        "/api/project/create",
        json!({"path":target,"name":"Failed title"}),
    )
    .await
    .unwrap_err();
    assert!(
        failure.1.0["error"]
            .as_str()
            .unwrap()
            .contains("Fixture git init refused")
    );
    assert!(!Path::new(&target).exists());
    std::fs::remove_dir(fixture.app.root.join("choices.json")).unwrap();
    std::fs::rename(
        fixture.app.root.join("choices.saved"),
        fixture.app.root.join("choices.json"),
    )
    .unwrap();
    assert!(
        choices_store::load(&fixture.app.root, true)
            .unwrap()
            .owner
            .project_paths
            .contains(&target)
    );
}

