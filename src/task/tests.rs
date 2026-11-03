use super::*;

/// A bare hub with `wt/main` on `main` and one commit, in a fresh temp folder.
pub(crate) fn hub_fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-task-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(dir).unwrap();
    let sh = |script: &str| {
        let status = Command::new("/bin/sh")
            .args(["-c", script])
            .current_dir(&dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .unwrap();
        assert!(status.success(), "{script}");
    };
    sh(
        "git init -q --bare .bare && git -C .bare symbolic-ref HEAD refs/heads/main \
        && git --git-dir=.bare worktree add -q --orphan -b main wt/main \
        && echo a > wt/main/a.txt && git -C wt/main add a.txt \
        && git -C wt/main -c user.email=t@t -c user.name=t commit -q -m a",
    );
    dir
}

/// Worktrunk itself, not a wrapper on PATH that may refuse a hub in a temp folder.
fn real_wt() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    [
        home.join(".cargo/bin/wt"),
        PathBuf::from("/opt/homebrew/bin/wt"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn commit(dir: &Path, file: &str) {
    std::fs::write(dir.join(file), "x\n").unwrap();
    for args in [
        vec!["add", file],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            file,
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
fn a_task_starts_lands_into_main_and_is_removed() {
    let Some(wt) = real_wt() else {
        eprintln!("skipped: Worktrunk is not installed");
        return;
    };
    let dir = hub_fixture("land");
    let root = dir.join("home");
    let main = dir.join("wt/main");
    let (task, _) = start(&root, &wt, &main, "fix-x", (Some("s1".into()), None)).unwrap();
    assert_eq!(task.branch, "swarm/fix-x");
    assert_eq!(records(&root), vec![task.clone()]);
    let path = PathBuf::from(&task.path);
    commit(&path, "b.txt");

    let request = request_landing(&root, &main, Some("fix-x")).unwrap();
    serve_landings(&root, &wt);
    let answer = take_result(&request).unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert!(main.join("b.txt").is_file(), "main did not get the commit");
    assert!(records(&root).is_empty());
}

#[test]
fn drop_refuses_a_task_with_commits_main_lacks() {
    let Some(wt) = real_wt() else {
        eprintln!("skipped: Worktrunk is not installed");
        return;
    };
    let dir = hub_fixture("drop");
    let root = dir.join("home");
    let (task, _) = start(&root, &wt, &dir.join("wt/main"), "keep", (None, None)).unwrap();
    commit(Path::new(&task.path), "c.txt");
    let refused = drop(&root, &wt, &dir.join("wt/main"), Some("keep")).unwrap_err();
    assert!(refused.contains("1 commit(s)"), "{refused}");
    assert!(Path::new(&task.path).is_dir());
}

#[test]
fn the_ref_hook_stops_an_agent_shell_from_moving_main() {
    let dir = hub_fixture("hook");
    let hub = hub(&dir.join("wt/main")).unwrap();
    assert_eq!(install_hook(&hub), None);
    let move_main = |agent: bool| {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(dir.join("wt/main"))
            .args(["update-ref", "refs/heads/main", "HEAD"]);
        for name in AGENT_MARKERS {
            command.env_remove(name);
        }
        if agent {
            command.env("SWARM_AGENT_ID", "coder");
        }
        command.output().unwrap()
    };
    let refused = move_main(true);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("swarm task land"));
    assert!(move_main(false).status.success());
}
