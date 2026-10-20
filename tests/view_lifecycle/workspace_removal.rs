// Workspace removal, paths, and saved defaults tests.
isolated!(dirty_unpushed_and_clean_removal, |f| {
    f.commit();
    f.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    f.git(&[
        "config",
        "remote.origin.url",
        f.root.join("unused-origin").to_str().unwrap(),
    ]);
    f.git(&[
        "config",
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    let path = f.create("Feature", json!({}));
    let file = PathBuf::from(&path).join("dirty.txt");
    fs::write(&file, "keep").unwrap();
    assert!(
        lifecycle::removal_blocker(&path)
            .unwrap()
            .unwrap()
            .contains("uncommitted")
    );
    assert!(lifecycle::check_git(&f.repo, &["worktree", "remove", &path]).is_err());
    assert!(file.exists());
    fs::remove_file(&file).unwrap();
    assert!(lifecycle::removal_blocker(&path).unwrap().is_none());
    lifecycle::check_git(
        &path,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "--allow-empty",
            "-qm",
            "local",
        ],
    )
    .unwrap();
    assert!(
        lifecycle::removal_blocker(&path)
            .unwrap()
            .unwrap()
            .contains("no upstream")
    );
    f.git(&["branch", "--set-upstream-to=origin/main", "swarm/feature"]);
    assert!(
        lifecycle::removal_blocker(&path)
            .unwrap()
            .unwrap()
            .contains("unpushed")
    );
    f.git(&["update-ref", "refs/remotes/origin/main", "swarm/feature"]);
    assert!(lifecycle::removal_blocker(&path).unwrap().is_none());
    f.git(&["branch", "--unset-upstream", "swarm/feature"]);
    assert!(lifecycle::removal_blocker(&path).unwrap().is_none());
    lifecycle::check_git(&f.repo, &["worktree", "remove", "--", &path]).unwrap();
    assert!(!PathBuf::from(&path).exists());
    assert!(lifecycle::removal_blocker(&path).is_err());
});
isolated!(ignored_items_and_delete_policy, |f| {
    f.commit();
    f.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let path = f.create("Feature", json!({}));
    fs::write(PathBuf::from(&f.repo).join(".git/info/exclude"), "tmp/\n").unwrap();
    fs::create_dir(PathBuf::from(&path).join("tmp")).unwrap();
    fs::write(PathBuf::from(&path).join("tmp/task.md"), "task").unwrap();
    let project = f.project();
    let workspace = project.workspaces.iter().find(|w| w.path == path).unwrap();
    let impact = lifecycle::impact(&project, workspace, 2).unwrap();
    assert_eq!(impact["ignoredCount"], 1);
    assert_eq!(impact["firstIgnoredPath"], "tmp/");
    assert_eq!(impact["confirmationRequired"], true);
    assert_eq!(impact["dirtyFiles"], json!([]));
    assert_eq!(impact["deleteBlocker"], Value::Null);
    assert!(lifecycle::can_delete(&project, workspace));
    assert!(!lifecycle::can_delete(
        &project,
        project
            .workspaces
            .iter()
            .find(|w| w.path == f.repo)
            .unwrap()
    ));
    lifecycle::check_git(&f.repo, &["worktree", "remove", "--", &path]).unwrap();
    assert!(!PathBuf::from(&path).join("tmp").exists());
});
isolated!(prune_missing_keeps_locked, |f| {
    f.commit();
    let missing = f.create("Missing", json!({}));
    let other = f.create("Other missing", json!({}));
    let locked = f.create("Locked", json!({}));
    f.git(&["worktree", "lock", &locked]);
    for path in [&missing, &other, &locked] {
        fs::remove_dir_all(path).unwrap();
    }
    let listing = lifecycle::git(&f.repo, &["worktree", "list", "--porcelain", "-z"]).unwrap();
    let listing = discovery::parse_worktrees(&listing.stdout);
    assert!(listing.iter().any(|w| w.path == missing && w.missing));
    assert!(listing.iter().any(|w| w.path == locked && w.locked));
    let project = f.project();
    let workspace = project
        .workspaces
        .iter()
        .find(|w| w.path == missing)
        .unwrap();
    let impact = lifecycle::impact(&project, workspace, 0).unwrap();
    assert_eq!(impact["prunePaths"].as_array().unwrap().len(), 2);
    assert!(!lifecycle::can_delete(&project, workspace));
    f.git(&["worktree", "prune"]);
    let listing = lifecycle::git(&f.repo, &["worktree", "list", "--porcelain", "-z"]).unwrap();
    let listing = discovery::parse_worktrees(&listing.stdout);
    assert!(!listing.iter().any(|w| w.path == missing || w.path == other));
    assert!(listing.iter().any(|w| w.path == locked && w.locked));
    assert!(lifecycle::check_git(f.root.to_str().unwrap(), &["worktree", "prune"]).is_err());
});
isolated!(absolute_paths_symlinks_and_bare_identity, |f| {
    assert!(lifecycle::checked_path(&f.home, "relative", true).is_err());
    let ordinary = f.root.join("ordinary");
    fs::create_dir(&ordinary).unwrap();
    let file = f.root.join("file");
    fs::write(&file, "file").unwrap();
    assert!(lifecycle::checked_path(&f.home, file.to_str().unwrap(), true).is_err());
    for path in [
        f.root.join("missing"),
        PathBuf::from("https://example.com/folder"),
    ] {
        assert!(lifecycle::checked_path(&f.home, path.to_str().unwrap(), true).is_err());
    }
    std::os::unix::fs::symlink(&ordinary, f.root.join("folder-link")).unwrap();
    assert_eq!(
        lifecycle::checked_path(&f.home, f.root.join("folder-link").to_str().unwrap(), true)
            .unwrap(),
        ordinary
    );
    std::os::unix::fs::symlink(&f.home, f.root.join("home-link")).unwrap();
    for path in [
        f.home.clone(),
        f.root.join("home-link"),
        f.root.join("home-link/new/project"),
    ] {
        assert!(lifecycle::checked_path(&f.home, path.to_str().unwrap(), false).is_err());
    }
    f.commit();
    let bare = f.root.join("hub/.bare");
    fs::create_dir(bare.parent().unwrap()).unwrap();
    f.git(&["clone", "--bare", &f.repo, bare.to_str().unwrap()]);
    assert_eq!(
        discovery::resolve(bare.parent().unwrap().to_str().unwrap()).unwrap(),
        bare.to_string_lossy()
    );
    let linked = f.root.join("hub/wt/feature");
    lifecycle::check_git(
        bare.to_str().unwrap(),
        &[
            "worktree",
            "add",
            "-b",
            "feature",
            linked.to_str().unwrap(),
            "main",
        ],
    )
    .unwrap();
    assert_eq!(
        discovery::resolve(linked.to_str().unwrap()).unwrap(),
        bare.to_string_lossy()
    );
    assert_eq!(
        lifecycle::project_path(linked.to_str().unwrap()),
        bare.parent().unwrap().to_string_lossy()
    );
});
isolated!(defaults_plain_answers_and_remove, |f| {
    f.commit();
    let project = f.project();
    let options = lifecycle::options(&project, &f.owner).unwrap();
    assert_eq!(options["folder"], "../project-worktrees");
    assert_eq!(options["prefix"], "swarm/");
    let path = f.create("Named", json!({"folder":"  ../custom  ","prefix":" fix/ "}));
    assert!(path.ends_with("/custom/named"));
    assert_eq!(
        lifecycle::check_git(&path, &["branch", "--show-current"]).unwrap(),
        "fix/named"
    );
    f.owner.project_defaults.insert(
        project.path.clone(),
        json!({"worktreeFolder":"/custom/tasks","branchPrefix":""}),
    );
    assert_eq!(lifecycle::defaults(&project, &f.owner).0, "/custom/tasks");
    assert_eq!(lifecycle::defaults(&project, &f.owner).1, "");
    let nested = format!("{}/nested", f.repo);
    f.owner
        .plain_folders
        .extend([f.repo.clone(), nested.clone()]);
    f.owner.project_paths.push(path.clone());
    f.owner.names.insert(path.clone(), "Feature".into());
    f.owner.names.insert(nested.clone(), "Nested".into());
    let project = f.project();
    let mut state = ViewState {
        selected_workspace: Some(path.clone()),
        ..ViewState::default()
    };
    state.selected_chats.insert(path.clone(), "chat".into());
    lifecycle::remove_project(
        &mut f.owner,
        &mut state,
        &project,
        std::slice::from_ref(&path),
    );
    assert!(f.owner.removed_projects.contains(&project.path));
    assert!(!f.owner.project_paths.contains(&path));
    assert!(!f.owner.project_paths.contains(&f.repo));
    assert_eq!(f.owner.names.get(&nested).unwrap(), "Nested");
    assert!(f.owner.plain_folders.contains(&nested));
    assert!(!f.owner.plain_folders.contains(&f.repo));
    assert!(!f.owner.project_defaults.contains_key(&f.repo));
    assert!(state.selected_workspace.is_none());
    assert!(!state.selected_chats.contains_key(&path));
    lifecycle::remember(&mut f.owner, &path, &project.path);
    assert!(!f.owner.removed_projects.contains(&project.path));
    assert!(f.owner.project_paths.contains(&path));
});
isolated!(shared_defaults_decode_merge_and_seed_overrides, |f| {
    use swarm::view::choices_store;
    for value in [
        json!({}),
        json!({"projectDefaults":null,"plainFolders":null,"future":true}),
    ] {
        let owner: OwnerChoices = serde_json::from_value(value).unwrap();
        assert!(owner.project_defaults.is_empty() && owner.plain_folders.is_empty());
    }
    let owner: OwnerChoices = serde_json::from_value(json!({"projectDefaults":{"/project":{"worktreeFolder":"wt/","branchPrefix":"","future":true},"/empty":{}}})).unwrap();
    assert_eq!(
        serde_json::from_value::<OwnerChoices>(serde_json::to_value(&owner).unwrap()).unwrap(),
        owner
    );
    choices_store::update(&f.home, None, |owner, _| {
        owner
            .project_defaults
            .insert("/other".into(), json!({"branchPrefix":"other/"}));
        owner.plain_folders.insert("/other".into());
        Ok(())
    })
    .unwrap();
    f.commit();
    let project = f.project();
    for (folder, prefix, expected) in [
        (
            format!(" ../{}-worktrees \n", project.name),
            " swarm/ ",
            json!({"worktreeFolder":null,"branchPrefix":null}),
        ),
        (
            " ../custom \n".into(),
            " fix/ ",
            json!({"worktreeFolder":"../custom","branchPrefix":"fix/"}),
        ),
        (
            "~/worktrees".into(),
            "",
            json!({"worktreeFolder":"~/worktrees","branchPrefix":""}),
        ),
    ] {
        let name = format!("Task {}", f.git(&["branch", "--list"]).lines().count());
        let (path, settings) = lifecycle::create_workspace(
            &f.home,
            &project,
            &f.owner,
            &f.request(&name, json!({"folder":folder,"prefix":prefix})),
        )
        .unwrap();
        assert_eq!(settings, expected);
        assert!(PathBuf::from(path).is_dir());
        choices_store::update(&f.home, None, |owner, _| {
            owner.project_defaults.insert(f.repo.clone(), settings);
            Ok(())
        })
        .unwrap();
    }
    let saved = choices_store::load(&f.home, true).unwrap();
    assert_eq!(
        saved.owner.project_defaults["/other"]["branchPrefix"],
        "other/"
    );
    assert!(saved.owner.plain_folders.contains("/other"));
    assert_eq!(saved.owner.project_defaults[&f.repo]["branchPrefix"], "");
});
