// Provider launch argument tests.
use super::*;

#[test]
fn model_refusal_lists_accounts_and_models_in_order() {
    assert_eq!(
        model_refusal(
            "codex",
            "missing",
            Some("work"),
            &["personal".into(), "backup".into()],
            &["model-b".into(), "model-a".into()],
        ),
        "swarm: model missing is not in the codex model list for account work\n\
fixes that might work:\n  - another account: --account personal, --account backup\n  - another model: model-b, model-a"
    );
}

#[test]
fn model_refusal_uses_cli_login_and_omits_empty_accounts() {
    assert_eq!(
        model_refusal("claude", "missing", None, &[], &["opus".into()]),
        "swarm: model missing is not in the claude model list for the CLI's own login\n\
fixes that might work:\n  - another model: opus"
    );
}

mod tests {
    use super::*;

    #[test]
    fn builds_each_provider_and_optional_flags() {
        let mut claude = role("claude");
        claude.permission = Some("acceptEdits".into());
        let claude_args = argv(
            "orchestrator",
            "code.complex",
            &claude,
            "/home",
            std::path::Path::new("/"),
        )
        .unwrap();
        assert_eq!(
            claude_args[..7],
            [
                "claude",
                "--model",
                "model-1",
                "--effort",
                "high",
                "--permission-mode",
                "acceptEdits"
            ]
        );
        assert_eq!(claude_args[7], "--settings");
        let settings: serde_json::Value = serde_json::from_str(&claude_args[8]).unwrap();
        assert!(
            settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("session chair claude:")
        );

        let mut codex = role("codex");
        codex.sandbox = Some("workspace-write".into());
        codex.approval = Some("never".into());
        let codex_args = argv(
            "coder",
            "coder",
            &codex,
            "/home x",
            std::path::Path::new("/"),
        )
        .unwrap();
        assert_eq!(
            codex_args[..15],
            [
                "codex",
                "--model",
                "model-1",
                "-c",
                "model_reasoning_effort=\"high\"",
                "-c",
                "check_for_update_on_startup=false",
                "-c",
                "allow_login_shell=false",
                "-c",
                "default_permissions=\"swarm-code\"",
                "-c",
                &format!(
                    r#"permissions.swarm-code={{extends=":workspace",filesystem={{":workspace_roots"={{".git"="write",".git/hooks"="read",".git/config"="read",".git/modules"="read",".codex"="read",".agents"="read"}},"/home x/.swarm"="write"}},network={{enabled=true,unix_sockets={{{}="allow"}}}}}}"#,
                    serde_json::to_string(&tmux_socket_dir(
                        std::env::var("TMUX_TMPDIR").ok().as_deref(),
                        unsafe { crate::hud::getuid() }
                    ))
                    .unwrap()
                ),
                "--ask-for-approval",
                "never"
            ]
        );
        assert_eq!(codex_args.len(), 15 + 2 * CODEX_STATE_EVENTS.len());

        let mut agy = role("agy");
        agy.permission = Some("skip".into());
        assert_eq!(
            argv("coder", "coder", &agy, "/home", std::path::Path::new("/")),
            Ok(vec![
                "agy",
                "--model",
                "model-1",
                "--effort",
                "high",
                "--dangerously-skip-permissions"
            ]
            .into_iter()
            .map(String::from)
            .collect())
        );
        agy.model = Some("default".into());
        agy.permission = Some("plan".into());
        assert_eq!(
            argv("coder", "coder", &agy, "/home", std::path::Path::new("/")),
            Ok(vec!["agy", "--effort", "high", "--mode", "plan"]
                .into_iter()
                .map(String::from)
                .collect())
        );
        agy.model = None;
        agy.permission = None;
        let agy_args = argv("coder", "coder", &agy, "/home", std::path::Path::new("/")).unwrap();
        assert_eq!(
            agy_args,
            vec!["agy", "--effort", "high"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert!(!agy_args.iter().any(|arg| arg.contains("SessionStart")));
        assert!(agy_takes_effort(Some("gemini-3.8-flash")));
        assert!(!agy_takes_effort(Some("claude-sonnet-4-6")));
        agy.model = Some("gemini-3.8-flash-high".into());
        assert_eq!(
            argv("coder", "coder", &agy, "/home", std::path::Path::new("/")),
            Ok(vec!["agy", "--model", "gemini-3.8-flash-high"]
                .into_iter()
                .map(String::from)
                .collect())
        );
    }

    #[test]
    fn pi_reports_its_state_by_osc_7501() {
        let mut pi = role("pi");
        pi.model = Some("anthropic/claude-haiku-5-5".into());
        assert_eq!(
            argv("coder", "coder", &pi, "/home", std::path::Path::new("/")),
            Ok(vec![
                "env",
                "PI_PROGRAM_STATUS=1",
                "pi",
                "--model",
                "anthropic/claude-haiku-5-5",
                "--thinking",
                "high"
            ]
            .into_iter()
            .map(String::from)
            .collect())
        );
        pi.model = Some("default".into());
        assert_eq!(
            argv("coder", "coder", &pi, "/home", std::path::Path::new("/")).unwrap()[3..],
            ["--thinking", "high"]
        );
        assert!(extra_args(Provider::Pi, &strings(&["--thinking", "max"])).is_err());
        assert!(extra_args(Provider::Pi, &strings(&["--session-id", "x"])).is_err());
        // The app finds a pi log by its fixed id under pi's own session folder (ADR 0086).
        for flag in [
            "--no-session",
            "--session-dir=/x",
            "--continue",
            "-c",
            "--resume",
            "-r",
            "--fork",
        ] {
            assert!(
                extra_args(Provider::Pi, &strings(&[flag])).is_err(),
                "{flag}"
            );
        }
        assert!(extra_args(Provider::Pi, &strings(&["--name", "x"])).is_ok());
        assert_eq!(
            extra_args(Provider::Pi, &strings(&["--name", "x", "--no-session"])).unwrap_err(),
            "swarm: the role owns the pi flag --no-session; remove it from the extra args"
        );
    }

    #[test]
    fn a_codex_writable_root_has_no_symlink_in_its_path() {
        let dir = std::env::temp_dir().join(format!("swarm-writable-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("real/.swarm")).unwrap();
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();
        let mut codex = role("codex");
        codex.sandbox = Some("workspace-write".into());
        let args = argv(
            "coder",
            "coder",
            &codex,
            &dir.join("link").to_string_lossy(),
            std::path::Path::new("/"),
        )
        .unwrap();
        let real = std::fs::canonicalize(dir.join("real/.swarm")).unwrap();
        let root = format!(
            "{}=\"write\"",
            serde_json::to_string(&real.to_string_lossy()).unwrap()
        );
        assert!(args.iter().any(|arg| arg.contains(&root)), "{args:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_codex_seat_in_a_linked_worktree_can_write_its_git_dir() {
        let dir = std::env::temp_dir().join(format!("swarm-worktree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        let repo = dir.join("repo");
        let tree = dir.join("tree");
        git(&["init", "-q", &repo.to_string_lossy()]);
        // An orphan worktree needs no commit.
        let (repo_arg, tree_arg) = (repo.to_string_lossy(), tree.to_string_lossy());
        git(&[
            "-C", &repo_arg, "worktree", "add", "-q", "--orphan", &tree_arg,
        ]);
        let mut codex = role("codex");
        codex.sandbox = Some("workspace-write".into());
        let profile = |cwd: &std::path::Path| {
            let args = argv("coder", "coder", &codex, "/home", cwd).unwrap();
            args.into_iter()
                .find(|arg| arg.starts_with("permissions.swarm-code="))
                .unwrap()
        };
        let common = repo.join(".git").to_string_lossy().into_owned();
        assert!(profile(&tree).contains(&format!(
            r#","{common}"="write","{common}/hooks"="read","{common}/config"="read","{common}/modules"="read","{common}/worktrees/tree"="write","{common}/worktrees/tree/config.worktree"="read"}}"#
        )));
        // The main checkout's git dir is inside the workspace, which `.git` already covers.
        assert!(!profile(&repo).contains(&common));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_building_codex_seat_may_reach_the_package_hosts() {
        let mut codex = role("codex");
        codex.sandbox = Some("workspace-write".into());
        let profile = |role: &str| {
            argv("seat", role, &codex, "/home", std::path::Path::new("/"))
                .unwrap()
                .into_iter()
                .find(|arg| arg.starts_with("permissions.swarm-code="))
                .unwrap()
        };
        let uv = |role: &str| {
            argv("seat", role, &codex, "/home", std::path::Path::new("/"))
                .unwrap()
                .iter()
                .any(|arg| arg.starts_with("shell_environment_policy.set.UV_CACHE_DIR="))
        };
        assert!(uv("code.routine") && !uv("review.deep"));
        for role in ["code.routine", "code.complex", "build", "checker"] {
            assert!(
                profile(role).contains(r#""allow"},domains={"pypi.org"="allow","#),
                "{role}"
            );
        }
        for role in ["review.deep", "council.gpt", "search.web"] {
            assert!(!profile(role).contains("domains="), "{role}");
        }
    }

    #[test]
    fn a_codex_seat_may_reach_the_tmux_socket_folder_tmux_names() {
        assert_eq!(
            tmux_socket_dir(Some("/no/such/dir"), 7),
            "/no/such/dir/tmux-7"
        );
        let tmp = std::fs::canonicalize("/tmp").unwrap();
        let tmp = format!("{}/tmux-7", tmp.to_string_lossy());
        assert_eq!(tmux_socket_dir(None, 7), tmp);
        assert_eq!(tmux_socket_dir(Some(""), 7), tmp);
    }

    #[test]
    fn every_launched_agent_reports_state_through_swarm_hook() {
        let claude_command = state_hook_command("claude").unwrap();
        assert!(claude_command.starts_with('\'') && claude_command.ends_with("' hook claude"));
        for agent in ["orchestrator", "coder"] {
            let args = argv(
                agent,
                "coder",
                &role("claude"),
                "/home",
                std::path::Path::new("/"),
            )
            .unwrap();
            assert_eq!(args.iter().filter(|arg| *arg == "--settings").count(), 1);
            let at = args.iter().position(|arg| arg == "--settings").unwrap();
            let settings: serde_json::Value = serde_json::from_str(&args[at + 1]).unwrap();
            for event in CLAUDE_STATE_EVENTS {
                let timeout = if event == "PermissionRequest" {
                    crate::permission::HOOK_TIMEOUT_SECONDS
                } else {
                    STATE_HOOK_TIMEOUT
                };
                assert_eq!(
                    settings["hooks"][event],
                    serde_json::json!([{"hooks": [{"type": "command", "command": claude_command, "timeout": timeout}]}]),
                    "{agent} {event}"
                );
            }
            let start: Vec<&str> = settings["hooks"]["SessionStart"]
                .as_array()
                .unwrap()
                .iter()
                .map(|group| group["hooks"][0]["command"].as_str().unwrap())
                .collect();
            let chair = chair_hook_command("claude").unwrap();
            let expected = if agent == "orchestrator" {
                vec![chair.as_str(), claude_command.as_str()]
            } else {
                vec![claude_command.as_str()]
            };
            assert_eq!(start, expected, "{agent} SessionStart");
        }

        let hook_args = |agent: &str| {
            let args = argv(
                agent,
                "coder",
                &role("codex"),
                "/home",
                std::path::Path::new("/"),
            )
            .unwrap();
            args.windows(2)
                .filter(|pair| pair[0] == "-c" && pair[1].starts_with("hooks."))
                .map(|pair| pair[1].clone())
                .collect::<Vec<_>>()
        };
        let coder = hook_args("coder");
        assert_eq!(coder, hook_args("reviewer"));
        let codex_command = serde_json::to_string(&shared_hook_command("codex")).unwrap();
        assert_eq!(
            coder,
            CODEX_STATE_EVENTS
                .map(|event| format!(
                    "hooks.{event}=[{{hooks=[]}},{{hooks=[{{type=\"command\",command={codex_command},timeout=3}}]}}]"
                ))
                .to_vec()
        );
    }

    #[test]
    fn reports_required_fields_and_unsupported_provider() {
        let mut missing = role("claude");
        missing.provider = None;
        assert_eq!(
            argv(
                "reviewer",
                "reviewer",
                &missing,
                "/home",
                std::path::Path::new("/")
            )
            .unwrap_err(),
            "swarm: role reviewer has no provider"
        );
        missing.provider = Some("claude".into());
        missing.model = None;
        assert_eq!(
            argv(
                "reviewer",
                "reviewer",
                &missing,
                "/home",
                std::path::Path::new("/")
            )
            .unwrap_err(),
            "swarm: role reviewer has no model"
        );
        missing.model = Some("model".into());
        missing.effort = None;
        assert_eq!(
            argv(
                "reviewer",
                "reviewer",
                &missing,
                "/home",
                std::path::Path::new("/")
            )
            .unwrap_err(),
            "swarm: role reviewer has no effort"
        );
        let unsupported = role("other");
        assert_eq!(
            argv(
                "reviewer",
                "reviewer",
                &unsupported,
                "/home",
                std::path::Path::new("/")
            )
            .unwrap_err(),
            "swarm: role reviewer uses unsupported provider other"
        );
    }

    #[test]
    fn validates_agent_ids() {
        assert!(valid_agent_id("coder-1"));
        assert!(!valid_agent_id("Coder-1"));
        assert!(!valid_agent_id("-coder"));
        assert!(!valid_agent_id(&"a".repeat(41)));
    }

    #[test]
    fn finds_the_command_model_and_checks_it_against_the_catalog() {
        let command = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(
            command_model(&command(&[
                "claude", "--model", "opus[1m]", "--effort", "high"
            ])),
            Some("opus")
        );
        assert_eq!(
            command_model(&command(&["codex", "-m", "gpt-6-sol"])),
            Some("gpt-6-sol")
        );
        assert_eq!(
            command_model(&command(&["codex", "--model=gpt-6-luna"])),
            Some("gpt-6-luna")
        );
        assert_eq!(
            command_model(&command(&["agy", "--", "--model", "x"])),
            None
        );
    }
}
