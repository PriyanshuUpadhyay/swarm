use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Hub {
    dir: PathBuf,
    main: PathBuf,
    root: PathBuf,
    wt: PathBuf,
}

impl Hub {
    /// None when Worktrunk is not installed, as on the CI runner; the test then passes without running.
    fn new() -> Option<Self> {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let wt = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo/bin/wt");
        if !wt.is_file() {
            eprintln!("skipped: Worktrunk is not installed at {}", wt.display());
            return None;
        }
        let dir = std::env::temp_dir().join(format!("swarm-land-origin-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(dir).unwrap();
        git(&dir, &["init", "-q", "--bare", ".bare"]);
        let common = dir.join(".bare");
        git(&common, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(&common, &["config", "user.name", "Test"]);
        git(&common, &["config", "user.email", "test@example.invalid"]);
        let main = dir.join("wt/main");
        git(
            &common,
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
        commit(&main, "initial");
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
        git(&main, &["fetch", "-q", "origin"]);
        git(&main, &["remote", "set-head", "origin", "main"]);
        let root = dir.join("home/.swarm");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        symlink(&wt, dir.join("bin/wt")).unwrap();
        let gh = dir.join("bin/gh");
        std::fs::write(&gh, format!(
            "#!/bin/sh\nif [ \"$2\" = list ]; then exit 0; fi\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' 'https://github.com/test/repo/pull/1'\n", dir.join("gh-args").display()
        )).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        Some(Self {
            dir,
            main,
            root,
            wt,
        })
    }

    fn cli(&self, args: &[&str], serve: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
        command
            .current_dir(&self.main)
            .args(args)
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
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for marker in swarm::task::AGENT_MARKERS {
            command.env_remove(marker);
        }
        let mut child = command.spawn().unwrap();
        loop {
            if serve {
                swarm::task::serve_landings(&self.root, &self.wt);
            }
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    }

    fn task(&self, name: &str) -> PathBuf {
        let output = self.cli(&["task", "start", name], false);
        assert!(output.status.success(), "{output:?}");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        PathBuf::from(value["path"].as_str().unwrap())
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    for marker in swarm::task::AGENT_MARKERS {
        command.env_remove(marker);
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().into()
}

fn commit(path: &Path, name: &str) {
    std::fs::write(path.join(name), name).unwrap();
    git(path, &["add", name]);
    git(path, &["commit", "-qm", name]);
}

#[test]
fn direct_land_updates_origin_and_local_main_together() {
    let Some(hub) = Hub::new() else { return };
    let path = hub.task("direct");
    commit(&path, "landed");
    // Origin advances while this task is open, so land must fetch and rebase it.
    let publisher = hub.task("publisher");
    commit(&publisher, "published-first");
    git(
        &publisher,
        &["push", "-q", "origin", "HEAD:refs/heads/main"],
    );
    let output = hub.cli(&["task", "land", "direct"], true);
    assert!(output.status.success(), "{output:?}");
    let head = git(&hub.main, &["rev-parse", "HEAD"]);
    assert_eq!(
        git(&hub.dir.join("origin.git"), &["rev-parse", "main"]),
        head
    );
    assert_eq!(git(&hub.main, &["rev-parse", "HEAD"]), head);
    assert!(hub.main.join("landed").is_file());
    assert!(hub.main.join("published-first").is_file());
    assert!(!path.exists());
    assert!(
        git(
            &hub.main,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads/swarm-landing/"
            ]
        )
        .is_empty()
    );
}

#[test]
fn pr_land_pushes_the_task_and_opens_a_pr_into_the_default() {
    // The server needs a process-local fake gh; other tests keep their original PATH.
    let Some(dir) = std::env::var_os("SWARM_TEST_PR_HUB") else {
        let Some(hub) = Hub::new() else { return };
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "pr_land_pushes_the_task_and_opens_a_pr_into_the_default",
                "--nocapture",
            ])
            .env("SWARM_TEST_PR_HUB", &hub.dir)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    hub.dir.join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    };
    let dir = PathBuf::from(dir);
    let hub = std::mem::ManuallyDrop::new(Hub {
        main: dir.join("wt/main"),
        root: dir.join("home/.swarm"),
        wt: PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo/bin/wt"),
        dir,
    });
    let path = hub.task("review");
    commit(&path, "reviewed");
    let head = git(&path, &["rev-parse", "HEAD"]);
    let output = hub.cli(&["task", "land", "review", "--pr"], true);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        git(&hub.dir.join("origin.git"), &["rev-parse", "swarm/review"]),
        head
    );
    let args = std::fs::read_to_string(hub.dir.join("gh-args")).unwrap();
    let args: Vec<_> = args.lines().collect();
    assert_eq!(&args[..2], &["pr", "create"]);
    assert!(args.windows(2).any(|pair| pair == ["--base", "main"]));
    assert!(path.is_dir());
    assert_eq!(swarm::task::records(&hub.root).len(), 1);
}

#[test]
fn task_start_fast_forwards_local_main_after_a_pr_merge() {
    let Some(hub) = Hub::new() else { return };
    let updater = hub.task("updater");
    commit(&updater, "merged-on-github");
    git(&updater, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    let expected = git(&updater, &["rev-parse", "HEAD"]);
    hub.task("next");
    assert_eq!(git(&hub.main, &["rev-parse", "HEAD"]), expected);
    assert!(hub.main.join("merged-on-github").is_file());
}

#[test]
fn a_refused_origin_push_keeps_main_and_the_task() {
    use std::os::unix::fs::PermissionsExt;
    let Some(hub) = Hub::new() else { return };
    let path = hub.task("refused");
    commit(&path, "keep-this-work");
    let initial = git(&hub.main, &["rev-parse", "HEAD"]);
    let hook = hub.dir.join("origin.git/hooks/pre-receive");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = hub.cli(&["task", "land", "refused"], true);
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(git(&hub.main, &["rev-parse", "HEAD"]), initial);
    assert_eq!(
        git(&hub.dir.join("origin.git"), &["rev-parse", "main"]),
        initial
    );
    assert!(path.join("keep-this-work").is_file());
    assert_eq!(swarm::task::records(&hub.root).len(), 1);
    assert!(
        git(
            &hub.main,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads/swarm-landing/"
            ]
        )
        .is_empty()
    );
}
