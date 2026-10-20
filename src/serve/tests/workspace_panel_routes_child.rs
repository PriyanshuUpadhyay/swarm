/// Self-reexecuting server test.
#[tokio::test]
async fn workspace_panel_routes_child() {
    if std::env::var("SWARM_PANEL_ROUTES").is_err() {
        return;
    }
    let root = PathBuf::from(std::env::var("HOME").unwrap());
    let fixture = action_fixture_at(Fixture::at(root.join("surface")));
    let source = root.join("project").to_string_lossy().into_owned();
    std::fs::create_dir(&source).unwrap();
    lifecycle::check_git(&source, &["init", "-q", "-b", "main"]).unwrap();
    lifecycle::check_git(
        &source,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "base",
        ],
    )
    .unwrap();
    lifecycle::check_git(
        &source,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/project.git",
        ],
    )
    .unwrap();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET cwd = ?1, chair_log = NULL WHERE id = ?2",
            (&source, &fixture.session),
        )
        .unwrap();
    fixture.app.directories.lock().unwrap().clear();
    choices_store::update(&fixture.app.root, None, |owner, _| {
        owner.project_paths.push(source.clone());
        owner.project_defaults.insert(
            source.clone(),
            json!({"setup_script":"printf 'setting up'; printf failure >&2; exit 9"}),
        );
        Ok(())
    })
    .unwrap();
    let issues = panel_read(
        State(fixture.app.clone()),
        "/api/issues".parse().unwrap(),
        Query(HashMap::from([("project".into(), source.clone())])),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(issues["issues"][0]["number"], 7);
    let made = life_post(&fixture, "/api/workspace/new", json!({"project":source,"mode":"issue","issue":7,"prefix":"swarm/","folder":"../tasks"})).await.unwrap();
    let path = made["workspace"].as_str().unwrap();
    assert_eq!(made["issue"]["title"], "Fix Sidebar");
    assert_eq!(made["issue"]["body"], "Use less space");
    assert_eq!(made["setup"]["state"], "failed");
    assert_eq!(made["setup"]["exit_code"], 9);
    assert!(Path::new(path).is_dir());
    assert_eq!(
        lifecycle::check_git(path, &["symbolic-ref", "--short", "HEAD"]).unwrap(),
        "swarm/7-fix-sidebar"
    );
    let saved = choices_store::load(&fixture.app.root, true).unwrap();
    assert_eq!(saved.state.selected_workspace.as_deref(), Some(path));
    assert!(saved.owner.project_defaults[&source]["setup_script"].is_string());
    std::fs::write(Path::new(path).join("hello.txt"), "hello\n").unwrap();
    let params = HashMap::from([
        ("workspace".into(), path.into()),
        ("path".into(), "hello.txt".into()),
    ]);
    assert_eq!(
        panel_read(
            State(fixture.app.clone()),
            "/api/changes/file".parse().unwrap(),
            Query(params.clone())
        )
        .await
        .unwrap()
        .0["preview"]["text"],
        "hello\n"
    );
    assert_eq!(
        panel_read(
            State(fixture.app.clone()),
            "/api/changes/diff".parse().unwrap(),
            Query(params)
        )
        .await
        .unwrap()
        .0["added"],
        1
    );
    for route in [
        "/api/changes",
        "/api/changes/file",
        "/api/changes/diff",
        "/api/pr",
        "/api/issues",
    ] {
        let key = if route == "/api/issues" {
            "project"
        } else {
            "workspace"
        };
        let params = HashMap::from([(key.into(), root.to_string_lossy().into_owned())]);
        assert_eq!(
            panel_read(
                State(fixture.app.clone()),
                route.parse().unwrap(),
                Query(params)
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
    }
}
