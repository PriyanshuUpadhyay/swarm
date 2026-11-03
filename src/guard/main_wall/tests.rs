use super::*;

#[test]
fn writes_into_main_are_denied_and_writes_elsewhere_pass() {
    let dir = crate::task::tests::hub_fixture("wall");
    let main = dir.join("wt/main");
    let task = dir.join("wt/task");
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(&main)
            .args(["worktree", "add", "-q", "-b", "swarm/task"])
            .arg(&task)
            .status()
            .unwrap()
            .success()
    );
    // `link/..` is main itself, as the OS reads it, not the task folder.
    std::fs::create_dir(main.join("sub")).unwrap();
    std::os::unix::fs::symlink(main.join("sub"), task.join("link")).unwrap();
    let common = std::fs::canonicalize(dir.join(".bare")).unwrap();
    let common = common.to_string_lossy();
    let (main, task) = (main.to_string_lossy(), task.to_string_lossy());
    let claude = |tool: &str, input: serde_json::Value, cwd: &str| {
        let payload = serde_json::json!({"tool_name": tool, "tool_input": input, "cwd": cwd});
        main_wall(Provider::Claude, "PreToolUse", &payload.to_string())
    };
    let write = |path: String| serde_json::json!({"file_path": path, "content": "x"});
    let bash = |command: &str| serde_json::json!({"command": command});

    for (tool, input, cwd) in [
        ("Write", write(format!("{main}/src/a.rs")), task.as_ref()),
        (
            "Edit",
            write(format!("{main}/../main/a.txt")),
            task.as_ref(),
        ),
        ("Bash", bash("echo x > a.txt"), main.as_ref()),
        (
            "Bash",
            bash(&format!("cat <<'EOF' > {main}/b\nhi\nEOF")),
            task.as_ref(),
        ),
        ("Bash", bash("git commit -qam fix"), main.as_ref()),
        (
            "Bash",
            bash(&format!("git -C {main} reset --hard")),
            task.as_ref(),
        ),
        ("Bash", bash("git push . HEAD:main"), task.as_ref()),
        ("Bash", bash("git branch -f main HEAD"), task.as_ref()),
        (
            "Bash",
            bash("sed -i '' s/a/b/ a.txt && echo ok"),
            main.as_ref(),
        ),
        ("Bash", bash("wt merge"), task.as_ref()),
        (
            "Bash",
            bash(&format!("cd {main} && echo x >> a.txt")),
            task.as_ref(),
        ),
        ("Bash", bash(&format!("cd {main} && echo x > a.txt")), "/"),
        (
            "Write",
            write(format!("{task}/link/../f.txt")),
            task.as_ref(),
        ),
        ("Bash", bash("cat <<< x\necho y > a.txt"), main.as_ref()),
        ("Bash", bash("echo $((x<<n))\nrm a.txt"), main.as_ref()),
        ("Bash", bash("env -i rm a.txt"), main.as_ref()),
        ("Bash", bash("rm -r src"), main.as_ref()),
        ("Bash", bash("echo x >& a.txt"), main.as_ref()),
        (
            "Bash",
            bash(&format!("wt -C {task} merge; wt remove -D main")),
            task.as_ref(),
        ),
        (
            "Bash",
            bash(&format!("git worktree remove {main}")),
            task.as_ref(),
        ),
        ("Bash", bash(&format!("wt remove -D {main}")), task.as_ref()),
        ("Bash", bash("bash -lc 'rm a.txt'"), main.as_ref()),
        ("Bash", bash("chmod -x a.txt"), main.as_ref()),
        (
            "Bash",
            bash("git update-ref -m land refs/heads/main HEAD"),
            task.as_ref(),
        ),
        (
            "Bash",
            bash(&format!("git --work-tree={main} checkout -- .")),
            task.as_ref(),
        ),
        (
            "Bash",
            bash(&format!("git -C {common} symbolic-ref HEAD refs/heads/zzz")),
            task.as_ref(),
        ),
        (
            "Bash",
            bash(&format!("cp a.txt {main}/b \\\n && echo ok")),
            task.as_ref(),
        ),
        (
            "Write",
            write(format!("{common}/worktrees/main/HEAD")),
            task.as_ref(),
        ),
        (
            "Bash",
            bash("perl -pi -e s/a/b/ a.txt; sed -Ei s/a/b/ a.txt"),
            main.as_ref(),
        ),
        (
            "Bash",
            bash("git commit --allow-empty -m x"),
            &format!("{main}/tmp"),
        ),
        (
            "Bash",
            bash("git config core.hooksPath /dev/null"),
            task.as_ref(),
        ),
        (
            "Bash",
            bash(&format!("echo x > {common}/refs/heads/main")),
            task.as_ref(),
        ),
    ] {
        assert_eq!(
            claude(tool, input.clone(), cwd),
            Some(Verdict::Deny(MAIN_READ_ONLY.into())),
            "{tool} {input} in {cwd}"
        );
    }
    for (tool, input, cwd) in [
        ("Write", write(format!("{task}/a.txt")), main.as_ref()),
        (
            "Write",
            write(format!("{main}/tmp/flow/01-frame.md")),
            main.as_ref(),
        ),
        ("Read", write(format!("{main}/a.txt")), main.as_ref()),
        (
            "Bash",
            bash("rg -n x src 2>/dev/null > /dev/null; git status; git log -1"),
            main.as_ref(),
        ),
        (
            "Bash",
            bash("echo '> a.txt' && cp a.txt tmp/a.txt"),
            main.as_ref(),
        ),
        (
            "Bash",
            bash(&format!("cat <<'EOF' > {task}/b\nrm a.txt\nEOF")),
            main.as_ref(),
        ),
        (
            "Bash",
            bash("git commit -qam fix && swarm task land"),
            task.as_ref(),
        ),
        (
            "Bash",
            bash(&format!(
                "cd {task} && printf 'x' >> a.txt && git commit -qam x"
            )),
            main.as_ref(),
        ),
        ("Bash", bash("P=x; printf 'x' >> $P/a.txt"), main.as_ref()),
        (
            "Grep",
            serde_json::json!({"pattern": "fn .* -> Result", "path": "src"}),
            main.as_ref(),
        ),
        ("Bash", bash("rg core.hooksPath docs"), main.as_ref()),
        ("Bash", bash("chmod 644 /tmp/é.txt"), main.as_ref()),
        (
            "Bash",
            bash(&format!(
                "perl -Mstrict -e 'print 1' a.txt; sed -e s/a/b/ -e s/c/d/ -i {task}/f.txt"
            )),
            main.as_ref(),
        ),
        (
            "Bash",
            bash("git config --local core.hooksPath"),
            main.as_ref(),
        ),
        (
            "Bash",
            bash("rg -n foo src # result -> list"),
            main.as_ref(),
        ),
        (
            "Bash",
            bash("git stash list; git update-ref refs/heads/swarm/task main"),
            main.as_ref(),
        ),
        (
            "Bash",
            bash("chmod +x /tmp/run.sh; dd if=/dev/zero of=/tmp/x; touch ~/x"),
            main.as_ref(),
        ),
    ] {
        assert_eq!(
            claude(tool, input.clone(), cwd),
            None,
            "{tool} {input} in {cwd}"
        );
    }
    // The fixture is not under the home folder, so `~` is checked on `resolve` itself.
    let home = std::fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
    assert_eq!(
        crate::task::resolve(Path::new("/"), Path::new("~/swarm-no-such-file")),
        home.join("swarm-no-such-file")
    );
    // Codex sends an argument list and its own folder; its patch names files in headers.
    let codex = serde_json::json!({"tool_name": "apply_patch", "cwd": "/", "tool_input":
        {"command": format!("*** Begin Patch\n*** Update File: {main}/a.txt\n*** End Patch")}});
    assert!(main_wall(Provider::Codex, "PreToolUse", &codex.to_string()).is_some());
    let shell = serde_json::json!({"tool_name": "exec_command", "cwd": "/", "tool_input":
        {"cmd": ["bash", "-lc", "rm a.txt"], "workdir": main}});
    assert!(main_wall(Provider::Codex, "PreToolUse", &shell.to_string()).is_some());
    let argv = serde_json::json!({"tool_name": "exec_command", "cwd": "/", "tool_input":
        {"cmd": ["touch", "b.txt"], "workdir": main}});
    assert!(main_wall(Provider::Codex, "PreToolUse", &argv.to_string()).is_some());
}
