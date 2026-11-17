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
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .arg("-C")
                .arg(dir)
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
}

/// A chat worktree on `swarm/<name>` from local main, with its record as the app writes it.
fn chat(root: &Path, main: &Path, name: &str, session: Option<&str>) -> Task {
    let path = main.parent().unwrap().join(name);
    let branch = format!("{PREFIX}{name}");

    git(
        main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &branch,
            path.to_str().unwrap(),
        ],
    )
    .unwrap();
    record_chat(root, &path, session.map(String::from))
        .unwrap()
        .0
}

#[test]
fn a_chat_worktree_lands_into_main_and_is_removed() {
    let Some(wt) = real_wt() else {
        eprintln!("skipped: Worktrunk is not installed");
        return;
    };
    let dir = hub_fixture("land");
    let root = dir.join("home");
    let main = dir.join("wt/main");
    let task = chat(&root, &main, "fix-x", Some("s1"));
    assert_eq!(task.branch, "swarm/fix-x");
    assert_eq!(records(&root), vec![task.clone()]);
    let path = PathBuf::from(&task.path);
    commit(&path, "b.txt");

    let (request, branch) = request_landing(&root, &main, Some("fix-x")).unwrap();
    assert_eq!(branch, "swarm/fix-x");
    // A server that stopped mid-landing left the request claimed; the next pass lands it.
    std::fs::rename(&request, request.with_extension("taking")).unwrap();
    serve_landings(&root, &wt);
    // A land run again after its wait ran out finds the answer, though the record is gone.
    assert_eq!(
        request_landing(&root, &main, Some("fix-x")),
        Ok((request.clone(), branch.clone()))
    );
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
    let task = chat(&root, &dir.join("wt/main"), "keep", None);
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

#[test]
fn a_chat_worktree_is_named_from_its_first_prompt_and_drops_from_its_folder() {
    let Some(wt) = real_wt() else {
        eprintln!("skipped: Worktrunk is not installed");
        return;
    };
    let dir = hub_fixture("name");
    let root = dir.join("home");
    let main = dir.join("wt/main");
    let task = chat(&root, &main, "chat-0123abcd", Some("s1"));
    let path = PathBuf::from(&task.path);

    // A local branch has the name and origin has -2, so this chat gets -3. Filler words go.
    let expected = "swarm/fix-login-redirect-bug-now-3";
    git(&main, &["branch", "swarm/fix-login-redirect-bug-now"]).unwrap();
    let origin_ref = "refs/remotes/origin/swarm/fix-login-redirect-bug-now-2";
    git(&main, &["update-ref", origin_ref, "HEAD"]).unwrap();
    let prompt = "Fix the login redirect bug now please";
    // The preview gives the same name and claims nothing.
    assert_eq!(chat_preview(&main, prompt).as_deref(), Some(expected));
    let named = name_chat(&root, &path, prompt).unwrap();
    assert_eq!(named.as_deref(), Some(expected));
    assert_eq!(name_chat(&root, &path, "a second prompt").unwrap(), None);

    let saved = records(&root);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].branch, expected);
    assert_eq!(saved[0].session.as_deref(), Some("s1"));
    let dropped = drop(&root, &wt, &path, None).unwrap();
    assert_eq!(dropped.branch, expected);
    assert!(!path.exists());
    assert!(records(&root).is_empty());

    // A prompt with no ASCII word still names the chat, so archive never reads it as unused.
    let other = chat(&root, &main, "chat-89abcdef", None);
    let named = name_chat(&root, Path::new(&other.path), "🙂").unwrap();
    assert_eq!(named.as_deref(), Some("swarm/untitled-89abcdef"));

    // A record that names another branch, as an agent could write it, renames nothing.
    let forged = chat(&root, &main, "chat-aaaabbbb", None);
    let record = record_path(&root, &forged.repo, &forged.branch);
    let other = Task {
        branch: "main".into(),
        ..forged.clone()
    };
    write_json(&record, &other).unwrap();
    let named = name_chat(&root, Path::new(&forged.path), "rename main please").unwrap();
    assert_eq!(named, None);
}
