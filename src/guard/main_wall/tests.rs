use super::*;

#[test]
fn edit_tools_stay_denied_in_main_and_shell_writes_only_warn() {
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
        (
            "Write",
            write(format!("{task}/link/../f.txt")),
            task.as_ref(),
        ),
        (
            "Write",
            write(format!("{common}/worktrees/main/HEAD")),
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
    ] {
        assert_eq!(
            claude(tool, input.clone(), cwd),
            None,
            "{tool} {input} in {cwd}"
        );
    }
    for command in ["echo x > a.txt", "git commit -qam fix", "wt merge"] {
        assert!(matches!(
            claude("Bash", bash(command), main.as_ref()),
            Some(Verdict::Warn(_))
        ));
    }
    assert_eq!(claude("Bash", bash("git status"), main.as_ref()), None);
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
