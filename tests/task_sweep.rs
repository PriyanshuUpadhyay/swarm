use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Hub {
    dir: PathBuf,
    main: PathBuf,
    root: PathBuf,
}

impl Hub {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("swarm-task-sweep-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(dir).unwrap();
        let main = dir.join("wt/main");
        git(&dir, &["init", "-q", "--bare", ".bare"]);
        git(
            &dir.join(".bare"),
            &["symbolic-ref", "HEAD", "refs/heads/main"],
        );
        git(
            &dir.join(".bare"),
            &[
                "worktree",
                "add",
                "-q",
                "--orphan",
                "-b",
                "main",
                main.to_str().unwrap(),
            ],
        );
        std::fs::write(main.join("a"), "a").unwrap();
        git(&main, &["add", "a"]);
        git(&main, &["commit", "-qm", "initial"]);
        git(&dir, &["clone", "-q", "--bare", ".bare", "origin.git"]);
        git(
            &main,
            &[
                "remote",
                "add",
                "origin",
                dir.join("origin.git").to_str().unwrap(),
            ],
        );
        let root = dir.join("home/.swarm");
        std::fs::create_dir_all(&root).unwrap();
        swarm::store::open(&root.join("swarm.db")).unwrap();
        std::fs::write(root.join("swarm-home"), "swarm\n").unwrap();
        Self { dir, main, root }
    }

    fn worktree(&self, name: &str) -> PathBuf {
        let path = self.dir.join("wt").join(name);
        git(
            &self.main,
            &[
                "worktree",
                "add",
                "-qb",
                name,
                path.to_str().unwrap(),
                "main",
            ],
        );
        path
    }

    fn sweep(&self, apply: bool, age: u64) -> Output {
        self.sweep_from(&self.main, apply, age)
    }

    fn sweep_from(&self, cwd: &Path, apply: bool, age: u64) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
        command
            .current_dir(cwd)
            .env("SWARM_HOME", self.root.parent().unwrap())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.dir.join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("SWARM_TEST_SWEEP_MIN_AGE_SECONDS", age.to_string())
            .args(["task", "sweep"]);
        if apply {
            command.arg("--apply");
        }
        command.output().unwrap()
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn sweep_removes_only_merged_clean_worktrees_and_keeps_the_branches() {
    let hub = Hub::new();
    let merged = hub.worktree("merged");
    let unmerged = hub.worktree("unmerged");
    let dirty = hub.worktree("dirty");
    std::fs::write(merged.join("landed"), "work").unwrap();
    git(&merged, &["add", "landed"]);
    git(&merged, &["commit", "-qm", "merged work"]);
    git(&merged, &["push", "-q", "origin", "HEAD:main"]);
    std::fs::write(unmerged.join("new"), "work").unwrap();
    git(&unmerged, &["add", "new"]);
    git(&unmerged, &["commit", "-qm", "unmerged work"]);
    std::fs::write(dirty.join("unsaved"), "work").unwrap();
    git(
        &merged,
        &[
            "config",
            "core.excludesFile",
            hub.dir.join("ignore").to_str().unwrap(),
        ],
    );
    std::fs::write(hub.dir.join("ignore"), "cache\n").unwrap();
    std::fs::write(merged.join("cache"), "ignored").unwrap();
    let repo = hub.dir.join(".bare").to_string_lossy().into_owned();
    std::fs::create_dir_all(hub.root.join("tasks")).unwrap();
    let record = swarm::task::record_path(&hub.root, &repo, "merged");
    std::fs::write(
        &record,
        serde_json::to_vec(&swarm::task::Task {
            branch: "merged".into(),
            path: merged.to_string_lossy().into_owned(),
            repo,
            default: "main".into(),
            session: None,
            agent: None,
            created_unix_s: 0,
        })
        .unwrap(),
    )
    .unwrap();

    let dry = hub.sweep_from(&hub.dir, false, 0);
    assert!(dry.status.success(), "{dry:?}");
    let report: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    let entry = report["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["branch"] == "merged")
        .unwrap();
    assert_eq!(entry["verdict"], "remove");
    assert_eq!(entry["ignored_files"], 1);
    assert!(merged.is_dir());
    assert!(record.is_file());

    let applied = hub.sweep(true, 0);
    assert!(applied.status.success(), "{applied:?}");
    assert!(!merged.exists());
    assert!(!record.exists());
    assert!(unmerged.is_dir());
    assert!(dirty.join("unsaved").is_file());
    assert!(hub.main.is_dir());
    git(&hub.main, &["show-ref", "--verify", "refs/heads/merged"]);
}

#[test]
fn sweep_protects_new_locked_missing_current_and_live_worktrees() {
    let hub = Hub::new();
    let eligible = hub.worktree("eligible");
    assert!(hub.sweep(true, 3600).status.success());
    assert!(
        eligible.is_dir(),
        "a new checkout at an old commit was removed"
    );
    let locked = hub.worktree("locked");
    git(&hub.main, &["worktree", "lock", locked.to_str().unwrap()]);
    let current = hub.worktree("current");
    let missing = hub.worktree("missing");
    std::fs::remove_dir_all(&missing).unwrap();
    let live_cwd = hub.worktree("live-cwd");
    let live_task = hub.worktree("live-task");
    let unknown = hub.worktree("unknown");
    std::fs::create_dir_all(hub.root.join("adapters")).unwrap();
    for (name, list) in [("live", "printf live-pane"), ("unknown", "exit 1")] {
        std::fs::write(hub.root.join("adapters").join(format!("{name}.conf")),
            format!("self = true\nspawn = true\nring = true\nlist = {list}\nclose = true\ncapture = true\n")).unwrap();
    }
    let connection = swarm::store::open(&hub.root.join("swarm.db")).unwrap();
    let live =
        swarm::store::create_session(&connection, "lane", &live_cwd, None, Some("live")).unwrap();
    let uncertain =
        swarm::store::create_session(&connection, "lane", &unknown, None, Some("unknown")).unwrap();
    for session in [&live, &uncertain] {
        swarm::store::add_agent(&connection, session, "coder", "code").unwrap();
        swarm::store::set_pane(&connection, session, "coder", "live-pane").unwrap();
    }
    std::fs::create_dir_all(hub.root.join("tasks")).unwrap();
    let repo = hub.dir.join(".bare").to_string_lossy().into_owned();
    std::fs::write(
        swarm::task::record_path(&hub.root, &repo, "live-task"),
        serde_json::to_vec(&swarm::task::Task {
            branch: "live-task".into(),
            path: live_task.to_string_lossy().into_owned(),
            repo,
            default: "main".into(),
            session: Some(live),
            agent: None,
            created_unix_s: 0,
        })
        .unwrap(),
    )
    .unwrap();
    drop(connection);
    let applied = hub.sweep_from(&current, true, 0);
    assert!(applied.status.success(), "{applied:?}");
    assert!(!eligible.exists());
    assert!(locked.is_dir());
    assert!(current.is_dir());
    assert!(live_cwd.is_dir());
    assert!(live_task.is_dir());
    assert!(unknown.is_dir());
    assert!(hub.main.is_dir());
    assert!(
        git(&hub.main, &["worktree", "list", "--porcelain"]).contains(missing.to_str().unwrap())
    );
}

#[test]
fn sweep_requires_the_exact_merged_pr_head() {
    use std::os::unix::fs::PermissionsExt;
    let hub = Hub::new();
    let path = hub.worktree("squashed");
    std::fs::write(path.join("new"), "work").unwrap();
    git(&path, &["add", "new"]);
    git(&path, &["commit", "-qm", "squashed elsewhere"]);
    std::fs::create_dir_all(hub.dir.join("bin")).unwrap();
    let gh = hub.dir.join("bin/gh");
    let response = hub.dir.join("pr.json");
    std::fs::write(&gh, format!("#!/bin/sh\ncat '{}'\n", response.display())).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        &response,
        format!(
            r#"[{{"headRefOid":"{}"}}]"#,
            git(&hub.main, &["rev-parse", "HEAD"])
        ),
    )
    .unwrap();
    assert!(hub.sweep(true, 0).status.success());
    assert!(
        path.is_dir(),
        "a merged PR for an older head authorized removal"
    );
    std::fs::write(&gh, "#!/bin/sh\nexit 1\n").unwrap();
    assert!(hub.sweep(true, 0).status.success());
    assert!(path.is_dir(), "a failed PR lookup authorized removal");
    std::fs::write(&gh, format!("#!/bin/sh\ncat '{}'\n", response.display())).unwrap();
    std::fs::write(
        &response,
        format!(
            r#"[{{"headRefOid":"{}"}}]"#,
            git(&path, &["rev-parse", "HEAD"])
        ),
    )
    .unwrap();
    let applied = hub.sweep(true, 0);
    assert!(applied.status.success(), "{applied:?}");
    assert!(!path.exists());
    git(&hub.main, &["show-ref", "--verify", "refs/heads/squashed"]);
}

#[test]
fn sweep_keeps_worktrees_when_fetch_or_store_reads_fail_or_origin_is_absent() {
    let hub = Hub::new();
    let path = hub.worktree("keep");
    git(
        &hub.main,
        &[
            "remote",
            "set-url",
            "origin",
            hub.dir.join("missing.git").to_str().unwrap(),
        ],
    );
    assert!(!hub.sweep(true, 0).status.success());
    assert!(path.is_dir());
    git(&hub.main, &["remote", "remove", "origin"]);
    assert!(hub.sweep(true, 0).status.success());
    assert!(path.is_dir());
    git(
        &hub.main,
        &[
            "remote",
            "add",
            "origin",
            hub.dir.join("origin.git").to_str().unwrap(),
        ],
    );
    std::fs::write(hub.root.join("swarm.db"), b"unreadable store").unwrap();
    let applied = hub.sweep(true, 0);
    assert!(applied.status.success(), "{applied:?}");
    assert!(path.is_dir());
}
