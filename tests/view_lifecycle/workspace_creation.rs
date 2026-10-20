// Workspace creation and branch selection tests.
isolated!(creates_worktree, |f| {
    f.commit();
    let main = f.git(&["rev-parse", "HEAD"]);
    f.git(&["checkout", "-qb", "feature"]);
    fs::write(PathBuf::from(&f.repo).join("feature.txt"), "feature").unwrap();
    f.git(&["add", "feature.txt"]);
    f.commit();
    let path = f.create("Fix sidebar", json!({}));
    assert_eq!(
        lifecycle::check_git(&path, &["rev-parse", "HEAD"]).unwrap(),
        main
    );
    assert_eq!(
        lifecycle::check_git(&path, &["branch", "--show-current"]).unwrap(),
        "swarm/fix-sidebar"
    );
    assert!(!PathBuf::from(&path).join("feature.txt").exists());
    assert!(f.project().workspaces.iter().any(|w| w.path == path));
    assert!(
        lifecycle::create_workspace(
            &f.home,
            &f.project(),
            &f.owner,
            &f.request("Fix sidebar", json!({}))
        )
        .is_err()
    );
    let bare = f.root.join("hub/.bare");
    fs::create_dir(bare.parent().unwrap()).unwrap();
    f.git(&["clone", "--bare", &f.repo, bare.to_str().unwrap()]);
    f.owner.project_paths = vec![bare.parent().unwrap().to_string_lossy().into_owned()];
    let project = f.project();
    assert!(project.id.starts_with("repository:"));
    let mut request = f.request("Review", json!({"folder":"wt/"}));
    request.project = project.path.clone();
    let path = lifecycle::create_workspace(&f.home, &project, &f.owner, &request)
        .unwrap()
        .0;
    assert_eq!(
        lifecycle::check_git(&path, &["rev-parse", "HEAD"]).unwrap(),
        main
    );
});
isolated!(creates_orphan_and_refuses_unborn_starts, |f| {
    let refs = lifecycle::references(&format!("{}/.git", f.repo)).unwrap();
    assert!(refs.default_branch.is_none());
    assert!(refs.bases.is_empty());
    for extra in [
        json!({"mode":"existing-branch","branch":"main"}),
        json!({"mode":"pr","pr":7}),
        json!({"prefix":"bad//","base":""}),
        json!({"base":"main"}),
    ] {
        assert!(
            lifecycle::create_workspace(
                &f.home,
                &f.project(),
                &f.owner,
                &f.request("Refused", extra)
            )
            .is_err()
        );
        assert!(!f.root.join("workspaces").exists());
    }
    let path = f.create("First task", json!({"base":""}));
    assert_eq!(
        lifecycle::check_git(&path, &["branch", "--show-current"]).unwrap(),
        "swarm/first-task"
    );
    assert!(lifecycle::check_git(&path, &["rev-parse", "--verify", "HEAD"]).is_err());
});
isolated!(existing_local_branch_and_tag, |f| {
    f.commit();
    let head = f.git(&["rev-parse", "HEAD"]);
    for branch in ["fix/local", "v1"] {
        f.git(&["branch", branch]);
        f.git(&["tag", branch]);
        let refs = lifecycle::references(&format!("{}/.git", f.repo)).unwrap();
        assert!(refs.local.iter().any(|b| b == branch));
        let path = f.create(branch, json!({"mode":"existing-branch","branch":branch}));
        assert_eq!(
            lifecycle::check_git(&path, &["branch", "--show-current"]).unwrap(),
            branch
        );
        assert_eq!(
            lifecycle::check_git(&path, &["rev-parse", "HEAD"]).unwrap(),
            head
        );
    }
});
isolated!(held_branches_and_selected_base, |f| {
    f.commit();
    f.git(&["branch", "fix/free"]);
    f.git(&["update-ref", "refs/remotes/origin/fix/free", "HEAD"]);
    let refs = lifecycle::references(&format!("{}/.git", f.repo)).unwrap();
    assert_eq!(refs.held, BTreeSet::from(["main".into()]));
    assert_eq!(refs.branches, vec!["fix/free"]);
    assert_eq!(refs.bases[0], "main");
    for extra in [
        json!({"mode":"existing-branch","branch":"main"}),
        json!({"base":"unknown"}),
        json!({"mode":"pr","pr":0}),
        json!({"folder":""}),
        json!({"prefix":"bad//"}),
    ] {
        assert!(
            lifecycle::create_workspace(
                &f.home,
                &f.project(),
                &f.owner,
                &f.request("Refused", extra)
            )
            .is_err()
        );
    }
    f.git(&["checkout", "-qb", "feature"]);
    f.commit();
    let head = f.git(&["rev-parse", "HEAD"]);
    let path = f.create("From feature", json!({"base":"feature","prefix":""}));
    assert_eq!(
        lifecycle::check_git(&path, &["rev-parse", "HEAD"]).unwrap(),
        head
    );
    assert_eq!(
        lifecycle::check_git(&path, &["branch", "--show-current"]).unwrap(),
        "from-feature"
    );
    assert!(
        lifecycle::create_workspace(&f.home, &f.project(), &f.owner, &f.request(" ", json!({})))
            .is_err()
    );
});
use std::collections::BTreeSet;
isolated!(remote_starts_and_default_references, |f| {
    f.commit();
    let origin = f.root.join("origin").to_string_lossy().into_owned();
    f.git(&["clone", &f.repo, &origin]);
    f.git(&["remote", "add", "origin", &origin]);
    lifecycle::check_git(&origin, &["checkout", "-qb", "fix/remote"]).unwrap();
    lifecycle::check_git(
        &origin,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "--allow-empty",
            "-qm",
            "remote",
        ],
    )
    .unwrap();
    f.git(&["fetch", "origin"]);
    f.git(&["remote", "set-head", "origin", "main"]);
    let tracked = f.git(&["rev-parse", "origin/fix/remote"]);
    lifecycle::check_git(
        &origin,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "--allow-empty",
            "-qm",
            "latest",
        ],
    )
    .unwrap();
    let head = lifecycle::check_git(&origin, &["rev-parse", "HEAD"]).unwrap();
    lifecycle::check_git(&origin, &["update-ref", "refs/pull/7/head", &head]).unwrap();
    f.git(&[
        "remote",
        "set-url",
        "origin",
        f.root.join("offline").to_str().unwrap(),
    ]);
    let remote = f.create(
        "Remote work",
        json!({"mode":"existing-branch","branch":"origin/fix/remote"}),
    );
    assert_eq!(
        lifecycle::check_git(&remote, &["rev-parse", "HEAD"]).unwrap(),
        tracked
    );
    assert_eq!(
        lifecycle::check_git(&remote, &["rev-parse", "--abbrev-ref", "@{upstream}"]).unwrap(),
        "origin/fix/remote"
    );
    f.git(&["remote", "set-url", "origin", &origin]);
    fs::write(f.root.join("gh-fail"), "").unwrap();
    assert!(
        lifecycle::create_workspace(
            &f.home,
            &f.project(),
            &f.owner,
            &f.request("PR refused", json!({"mode":"pr","pr":7}))
        )
        .is_err()
    );
    assert!(!f.root.join("workspaces/pr-refused").exists());
    fs::remove_file(f.root.join("gh-fail")).unwrap();
    let pr = f.create("Review seven", json!({"mode":"pr","pr":7}));
    assert_eq!(
        lifecycle::check_git(&pr, &["rev-parse", "HEAD"]).unwrap(),
        head
    );
    assert_eq!(
        lifecycle::check_git(&pr, &["branch", "--show-current"]).unwrap(),
        "swarm/review-seven"
    );
    assert!(
        fs::read_to_string(f.root.join("gh-calls"))
            .unwrap()
            .contains("pr view 7 --json number")
    );
    f.git(&["branch", "-m", "main", "trunk"]);
    f.git(&["remote", "set-head", "origin", "fix/remote"]);
    assert_eq!(
        lifecycle::references(&format!("{}/.git", f.repo))
            .unwrap()
            .default_branch
            .as_deref(),
        Some("fix/remote")
    );
    lifecycle::check_git(&origin, &["branch", "release"]).unwrap();
    f.git(&["fetch", "origin"]);
    f.git(&["remote", "set-head", "origin", "release"]);
    let refs = lifecycle::references(&format!("{}/.git", f.repo)).unwrap();
    assert_eq!(refs.default_branch.as_deref(), Some("origin/release"));
    assert_eq!(refs.remote[0], "origin/release");
    assert_eq!(refs.bases[0], "origin/release");
    assert!(!refs.remote.iter().any(|s| s == "origin/HEAD"));
});
isolated!(refuses_existing_folder, |f| {
    f.commit();
    fs::create_dir_all(f.root.join("workspaces/occupied")).unwrap();
    fs::write(f.root.join("workspaces/occupied/keep.txt"), "keep").unwrap();
    assert!(
        lifecycle::create_workspace(
            &f.home,
            &f.project(),
            &f.owner,
            &f.request("Occupied", json!({}))
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(f.root.join("workspaces/occupied/keep.txt")).unwrap(),
        "keep"
    );
    assert!(f.git(&["branch", "--list", "swarm/occupied"]).is_empty());
});
