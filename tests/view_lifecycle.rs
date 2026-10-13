use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use swarm::view::{
    choices::{OwnerChoices, ViewState},
    discovery::{self, Discovery},
    lifecycle::{self, FirstCommit},
    tree::{self, Project},
};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    repo: String,
    owner: OwnerChoices,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
        let home = root.join(".swarm");
        fs::create_dir(&home).unwrap();
        fs::write(home.join("swarm-home"), "fixture").unwrap();
        let repo = root.join("project").to_string_lossy().into_owned();
        fs::create_dir(&repo).unwrap();
        lifecycle::check_git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        let mut owner = OwnerChoices::default();
        owner.project_paths.push(repo.clone());
        Self {
            root,
            home,
            repo,
            owner,
        }
    }
    fn git(&self, args: &[&str]) -> String {
        lifecycle::check_git(&self.repo, args).unwrap()
    }
    fn commit(&self) {
        self.git(&[
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
        ]);
    }
    fn project(&self) -> Project {
        let mut snapshot = json!({"sessions":[],"agents":{}});
        Discovery::default().augment(&mut snapshot, &self.owner, 100);
        tree::build(&snapshot, &self.owner).projects.remove(0)
    }
    fn request(&self, name: &str, extra: Value) -> lifecycle::NewWorkspace {
        let mut value = json!({"project":self.repo,"name":name,"folder":"../workspaces","prefix":"swarm/","mode":"new-branch","base":"main"});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }
    fn create(&self, name: &str, extra: Value) -> String {
        lifecycle::create_workspace(
            &self.home,
            &self.project(),
            &self.owner,
            &self.request(name, extra),
        )
        .unwrap()
        .0
    }
}
macro_rules! isolated {
    ($name:ident, |$fixture:ident| $body:block) => {
        #[test]
        fn $name() {
            if std::env::var("SWARM_LIFECYCLE_CASE").ok().as_deref() != Some(stringify!($name)) {
                let root = std::env::temp_dir().join(format!("swarm-lifecycle-{}",uuid::Uuid::now_v7()));
                fs::create_dir_all(root.join("bin")).unwrap();
                use std::os::unix::fs::PermissionsExt;
                fs::write(root.join("bin/gh"), "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/gh-calls\"\nif [ -f \"$HOME/gh-fail\" ]; then exit 1; fi\nprintf '{\"number\":7}\\n'\n").unwrap();
                fs::set_permissions(root.join("bin/gh"),fs::Permissions::from_mode(0o755)).unwrap();
                let output = Command::new(std::env::current_exe().unwrap()).args(["--exact",stringify!($name),"--nocapture"])
                    .env_clear().env("HOME",&root).env("PATH",format!("{}:/usr/bin:/bin",root.join("bin").display()))
                    .env("GIT_CEILING_DIRECTORIES",fs::canonicalize(&root).unwrap()).env("GIT_CONFIG_NOSYSTEM","1").env("GIT_CONFIG_GLOBAL",root.join(".gitconfig"))
                    .env("SWARM_LIFECYCLE_CASE",stringify!($name)).output().unwrap();
                fs::remove_dir_all(root).unwrap();
                assert!(output.status.success(),"{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
                return;
            }
            let mut $fixture = Fixture::new();
            let _ = &mut $fixture;
            $body
        }
    }
}

#[test]
fn validates_branch_names() {
    assert_eq!(
        lifecycle::branch_name("Fix Sidebar!", "swarm/").as_deref(),
        Some("swarm/fix-sidebar")
    );
    assert_eq!(lifecycle::branch_name("!!!", "").as_deref(), Some("task"));
    assert!(lifecycle::branch_name(" ", "swarm/").is_none());
    assert_eq!(
        lifecycle::branch_name(&"x".repeat(60), "").unwrap(),
        "x".repeat(40)
    );
    for prefix in [
        "swarm/",
        "",
        "-",
        "/",
        "bad//",
        ".hidden/",
        "name.lock/",
        "bad../",
        "bad@{/",
        "bad /",
        "bad\\/",
        "bad~/",
        "bad^/",
        "bad:/",
        "bad?/",
        "bad*/",
        "bad[/",
        "bad\n/",
        "é/",
    ] {
        let status = Command::new("git")
            .args([
                "check-ref-format",
                "--branch",
                &format!("{prefix}fix-sidebar"),
            ])
            .output()
            .unwrap()
            .status;
        assert_eq!(
            lifecycle::branch_name("Fix Sidebar", prefix).is_some(),
            status.success(),
            "{prefix}"
        );
    }
    assert_eq!(
        lifecycle::branch_name("A", " fix/ ").as_deref(),
        Some("fix/a")
    );
}

#[test]
fn listing_preserves_paths_branches_and_worktree_states() {
    let entries = discovery::parse_worktrees(b"worktree /hub/.bare\0bare\0\0worktree /tasks/space and\nnewline\0HEAD deadbeef\0branch refs/heads/fix/sidebar\0locked explanation\0\0worktree /tasks/missing\0detached\0prunable gone\0worktree /tasks/other\0branch refs/heads/main\0");
    assert_eq!(entries.len(), 4);
    assert!(entries[0].bare);
    assert_eq!(entries[1].path, "/tasks/space and\nnewline");
    assert_eq!(entries[1].branch.as_deref(), Some("fix/sidebar"));
    assert!(entries[1].locked);
    assert!(entries[2].detached && entries[2].missing);
    assert_eq!(entries[3].branch.as_deref(), Some("main"));
    for bytes in [b"".as_slice(), b"\0\0", b"HEAD deadbeef\0"] {
        assert!(discovery::parse_worktrees(bytes).is_empty());
    }
}

#[test]
fn first_commit_notices_include_the_required_next_action() {
    assert!(FirstCommit::Made.notice().is_none());
    assert_eq!(
        FirstCommit::SkippedNoIdentity.notice().unwrap(),
        "Created without a first commit, because git has no user.name and user.email. Set them, then commit once; until then workspaces start orphan branches."
    );
    assert_eq!(
        FirstCommit::Failed("Hook refused".into()).notice().unwrap(),
        "Created without a first commit. Hook refused"
    );
}
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
isolated!(initialization_and_shared_history, |f| {
    fs::write(
        f.root.join(".gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
    )
    .unwrap();
    assert_eq!(
        lifecycle::initialize_project(&f.repo).unwrap(),
        FirstCommit::Made
    );
    assert_eq!(f.git(&["log", "--format=%s"]), "Create project");
    assert_eq!(f.git(&["ls-tree", "--name-only", "HEAD"]), ".gitignore");
    fs::create_dir(PathBuf::from(&f.repo).join("tmp")).unwrap();
    fs::write(PathBuf::from(&f.repo).join("tmp/task"), "task").unwrap();
    assert!(f.git(&["status", "--porcelain"]).is_empty());
    let head = f.git(&["rev-parse", "HEAD"]);
    for name in ["First", "Second"] {
        let path = f.create(name, json!({}));
        assert_eq!(
            lifecycle::check_git(&path, &["rev-parse", "HEAD"]).unwrap(),
            head
        );
    }
});
isolated!(missing_identity_and_ignore_rules, |f| {
    for identity in ["", "name = Fixture", "email = fixture@example.com"] {
        fs::write(f.root.join(".gitconfig"), format!("[user]\n{identity}\n")).unwrap();
        assert_eq!(
            lifecycle::initialize_project(&f.repo).unwrap(),
            FirstCommit::SkippedNoIdentity
        );
        assert_eq!(
            fs::read_to_string(PathBuf::from(&f.repo).join(".gitignore")).unwrap(),
            "tmp/\n"
        );
        assert!(lifecycle::check_git(&f.repo, &["rev-parse", "--verify", "HEAD"]).is_err());
    }
    for original in ["build/", "build/\n", "build/\ntmp/\n", "tmp/\r\nbuild/\r\n"] {
        let path = PathBuf::from(&f.repo).join(".gitignore");
        fs::write(&path, original).unwrap();
        lifecycle::initialize_project(&f.repo).unwrap();
        let actual = fs::read_to_string(&path).unwrap();
        assert!(actual.starts_with(original));
        assert_eq!(actual.lines().filter(|s| *s == "tmp/").count(), 1);
    }
});
isolated!(rejected_first_commit_keeps_project, |f| {
    fs::write(
        f.root.join(".gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n",
    )
    .unwrap();
    let hook = PathBuf::from(&f.repo).join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'Hook refused' >&2\nexit 1\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(lifecycle::initialize_project(&f.repo).unwrap(),FirstCommit::Failed(message) if message.contains("Hook refused"))
    );
    assert!(PathBuf::from(&f.repo).join(".gitignore").exists());
});
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
#[test]
fn archive_reads_all_sessions_before_any_close_and_orders_children_first() {
    let sessions = vec![
        json!({"id":"current","adapter":"fixture","agents":2,"chair_id":"lead"}),
        json!({"id":"older","adapter":"fixture","agents":1}),
    ];
    let batch = json!({"current":[{"id":"lead","alive":true},{"id":"worker","alive":true},{"id":"dead","alive":false}]});
    let targets = lifecycle::live_targets(&sessions, Some(&batch), |s| {
        assert_eq!(s["id"], "older");
        Ok(vec![json!({"id":"orchestrator","alive":true})])
    })
    .unwrap();
    assert_eq!(
        targets,
        vec![
            ("current".into(), "worker".into()),
            ("current".into(), "lead".into()),
            ("older".into(), "orchestrator".into())
        ]
    );
    assert!(
        lifecycle::live_targets(&sessions, Some(&batch), |_| Err("unreadable".into()))
            .unwrap_err()
            .to_string()
            .contains("could not read")
    );
    let mut reads = 0;
    let targets = lifecycle::live_targets(&sessions, None, |_| {
        reads += 1;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(reads, 2);
    assert!(targets.is_empty());
    let zero = vec![
        json!({"id":"empty","adapter":"fixture","agents":0}),
        json!({"id":"legacy","agents":1}),
    ];
    let batch = json!({});
    let targets =
        lifecycle::live_targets(&zero, Some(&batch), |_| panic!("should not read")).unwrap();
    assert!(targets.is_empty());
    let mut reads = 0;
    lifecycle::live_targets(&zero, None, |_| {
        reads += 1;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(reads, 1);
    assert!(lifecycle::live_targets(&zero, None, |_| Err("failed".into())).is_err());
}
#[test]
fn raw_git_timeout_reaps_process() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 5 &"]);
    let start = std::time::Instant::now();
    assert!(discovery::run_command(&mut command, 1).is_err());
    assert!(start.elapsed().as_secs_f64() < 2.0);
}
