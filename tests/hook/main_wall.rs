use super::*;

#[test]
fn shell_writes_warn_and_pass_while_edit_tools_stay_denied() {
    let home = scratch("main-wall");
    let bare = home.join(".bare");
    let main = home.join("wt/main");
    let git = |args: &[&str]| {
        let output = clean(Path::new("/usr/bin/git"), &home)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    };
    git(&["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(&[
        "-C",
        bare.to_str().unwrap(),
        "symbolic-ref",
        "HEAD",
        "refs/heads/main",
    ]);
    git(&[
        "--git-dir",
        bare.to_str().unwrap(),
        "worktree",
        "add",
        "-q",
        "--orphan",
        "-b",
        "main",
        main.to_str().unwrap(),
    ]);
    std::fs::write(main.join("a.txt"), "a\n").unwrap();
    git(&["-C", main.to_str().unwrap(), "add", "a.txt"]);
    git(&[
        "-C",
        main.to_str().unwrap(),
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "a",
    ]);
    let list = home.join(".swarm/guards.json");
    std::fs::write(&list, r#"{"rules": []}"#).unwrap();

    for provider in ["claude", "codex", "agy", "pi"] {
        let payload = |edit: bool| {
            if provider == "agy" {
                serde_json::json!({"cwd": main, "toolCall": {
                    "name": if edit { "write_to_file" } else { "run_command" },
                    "args": if edit { serde_json::json!({"TargetFile": main.join("a.txt"), "CodeContent": "b"}) }
                        else { serde_json::json!({"CommandLine": "echo b > a.txt"}) }
                }})
            } else if provider == "codex" {
                serde_json::json!({"cwd": home,
                    "tool_name": if edit { "apply_patch" } else { "exec_command" },
                    "tool_input": if edit { serde_json::json!({"input": format!("*** Begin Patch\n*** Update File: {}\n@@\n-a\n+b\n*** End Patch", main.join("a.txt").display())}) }
                        else { serde_json::json!({"cmd": ["bash", "-lc", "echo b > a.txt"], "workdir": main}) }
                })
            } else {
                serde_json::json!({"cwd": main,
                    "tool_name": if edit { "write" } else { "bash" },
                    "tool_input": if edit { serde_json::json!({"file_path": main.join("a.txt"), "content": "b"}) }
                        else { serde_json::json!({"command": "echo b > a.txt"}) }
                })
            }
        };
        let shell = payload(false).to_string();
        let allowed = swarm(&home, &[], &["guard", provider, "PreToolUse"], &shell);
        assert_eq!(allowed.status.code(), Some(0), "{provider}: {allowed:?}");
        assert!(allowed.stderr.is_empty(), "{provider}: {allowed:?}");
        match provider {
            "claude" | "codex" => {
                let output: serde_json::Value = serde_json::from_slice(&allowed.stdout).unwrap();
                assert!(
                    output["hookSpecificOutput"]["additionalContext"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty()),
                    "{provider}: {output}"
                );
                assert!(
                    output["hookSpecificOutput"]["permissionDecision"].is_null(),
                    "{provider}: {output}"
                );
            }
            "agy" => assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&allowed.stdout).unwrap()["decision"],
                "allow"
            ),
            _ => assert!(allowed.stdout.is_empty()),
        }
        let denied = swarm(
            &home,
            &[],
            &["guard", provider, "PreToolUse"],
            &payload(true).to_string(),
        );
        match provider {
            "claude" | "agy" => {
                let output: serde_json::Value = serde_json::from_slice(&denied.stdout).unwrap();
                let decision = if provider == "claude" {
                    &output["hookSpecificOutput"]["permissionDecision"]
                } else {
                    &output["decision"]
                };
                assert_eq!(decision, "deny", "{provider}: {denied:?}");
            }
            _ => assert_eq!(denied.status.code(), Some(2), "{provider}: {denied:?}"),
        }
        // A main-wall warning cannot bypass the owner's guard rules.
        if provider != "pi" {
            std::fs::write(&list, r#"{"rules": [{"name": "owner", "event": "PreToolUse", "command": ["/bin/sh", "-c", "exit 2"]}]}"#).unwrap();
            let denied = swarm(&home, &[], &["guard", provider, "PreToolUse"], &shell);
            if provider == "codex" {
                assert_eq!(denied.status.code(), Some(2), "{denied:?}");
            } else {
                let output: serde_json::Value = serde_json::from_slice(&denied.stdout).unwrap();
                let decision = if provider == "claude" {
                    &output["hookSpecificOutput"]["permissionDecision"]
                } else {
                    &output["decision"]
                };
                assert_eq!(decision, "deny", "{denied:?}");
            }
            std::fs::write(&list, r#"{"rules": []}"#).unwrap();
        }
    }
    std::fs::remove_dir_all(home).unwrap();
}
