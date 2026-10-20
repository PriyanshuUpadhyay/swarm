// Claude account and folder trust tests.

use super::*;

#[test]
fn a_claude_launch_trusts_the_config_that_the_pane_reads() {
    let home = scratch("trust");
    // The owner gave standing consent for folder trust (ADR 0043).
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
    let profiles = ["work", "personal"].map(|name| home.join(".claude/.profiles").join(name));
    for dir in &profiles {
        std::fs::create_dir_all(dir).unwrap();
    }
    let hidden = home.join(".claude/.profiles/.session-map");
    std::fs::create_dir_all(&hidden).unwrap();
    tool(
        &home,
        "claude",
        r#"if [ "$*" = 'auth status' ]; then /bin/echo "$CLAUDE_CONFIG_DIR" >> "$HOME/native-reads"; echo '{"loggedIn":true}'; fi"#,
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let accounts: Vec<_> = profiles.iter().zip([30,75]).map(|(path,used)| serde_json::json!({"home":path,"meters":[{"provider":"claude","account":path.file_name().unwrap().to_str().unwrap(),"label":"account","window":"5h","window_minutes":300,"used_pct":used,"reset_time_seconds":null,"state":"fresh","source":"claude_api","reason":null,"as_of_seconds":now}]})).collect();
    std::fs::write(
        home.join(".swarm/claude-usage.json"),
        serde_json::json!({"accounts":accounts}).to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"review.deep": ["claude-opus"]},
            "runners": {"claude-opus": {"provider": "claude", "model": "opus", "effort": "high"}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(&home, &fake, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    // Consent covers only a folder that passes the trust check, such as a git repo.
    let launch = |seat: &str, repo: &str, account: Option<&str>| {
        let cwd = git_repo(&home, repo).to_string_lossy().into_owned();
        let mut args = vec!["launch", seat, "review.deep", "--cwd", &cwd];
        if let Some(account) = account {
            args.extend(["--account", account]);
        }
        let output = swarm(&home, &env, &args);
        assert!(output.status.success(), "{seat}: {}", stderr(&output));
        if account.is_none() {
            assert!(
                stderr(&output).contains("account work"),
                "{}",
                stderr(&output)
            );
        }
        home.join(repo).join(".herdr/workers")
    };

    let any = launch("seat-any", "any", None);
    assert!(trusted(&home.join(".claude.json"), &any));
    assert!(!hidden.join(".claude.json").exists());
    let only_b = launch("seat-personal", "only-personal", Some("personal"));
    assert!(trusted(&home.join(".claude.json"), &only_b));
    for dir in &profiles {
        assert!(!dir.join(".claude.json").exists());
    }

    let native_default = launch("seat-default", "only-default", Some("default"));
    assert!(trusted(&home.join(".claude.json"), &native_default));
    assert!(!trusted(
        &home.join(".claude/.claude.json"),
        &native_default
    ));
    let default_script =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/seat-default.sh")))
            .unwrap();
    assert!(
        default_script.contains("'env' '-u' 'CLAUDE_CONFIG_DIR' "),
        "{default_script}"
    );
    assert!(
        !default_script.contains("CLAUDE_CONFIG_DIR="),
        "{default_script}"
    );

    // Auto and the pane use the same native account home.
    std::fs::write(home.join("native-reads"), "").unwrap();
    let cwd = git_repo(&home, "auto");
    let output = swarm(
        &home,
        &env,
        &[
            "launch",
            "seat-auto",
            "review.deep",
            "--cwd",
            &cwd.to_string_lossy(),
            "--account",
            "auto",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let native_reads = std::fs::read_to_string(home.join("native-reads")).unwrap();
    assert_eq!(
        native_reads.lines().count(),
        3,
        "the role probe, Auto and pane must share one native account read"
    );
    assert!(
        stderr(&output).contains("account work"),
        "{}",
        stderr(&output)
    );
    // The app shows the role's model before the chair writes its first log.
    assert!(
        stderr(&output).contains("model opus"),
        "{}",
        stderr(&output)
    );
    assert!(trusted(
        &home.join(".claude.json"),
        &cwd.join(".herdr/workers")
    ));

    // A checked-out repo can commit `.herdr` or `.herdr/workers` as a link, and trusting where
    // it points could trust any folder, such as `/`.
    let target = home.join("target");
    std::fs::create_dir_all(target.join("workers")).unwrap();
    for (repo, link, points_to) in [
        ("link-herdr", ".herdr", target.clone()),
        ("link-workers", ".herdr/workers", target.join("workers")),
    ] {
        let cwd = git_repo(&home, repo);
        std::fs::create_dir_all(cwd.join(link).parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&points_to, cwd.join(link)).unwrap();
        let cwd = cwd.to_string_lossy().into_owned();
        let output = swarm(&home, &env, &["launch", repo, "review.deep", "--cwd", &cwd]);
        assert!(!output.status.success(), "{repo} launched");
        assert!(
            stderr(&output).contains("symlink"),
            "{repo}: {}",
            stderr(&output)
        );
        assert!(
            !trusted(&home.join(".claude.json"), &target.join("workers")),
            "{repo}"
        );
    }

    // A resuming child runs in cwd itself, so a linked `.herdr` below it does not matter.
    let resumed = home.join("link-herdr");
    let cwd = resumed.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &[
            "launch",
            "seat-resume",
            "review.deep",
            "--cwd",
            &cwd,
            "--",
            "--resume",
            "x",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(trusted(&home.join(".claude.json"), &resumed));

    // The app hides the chair's pane, so nobody answers a trust dialog there; it runs in cwd.
    // Each chair gets its own session, as each app chat does.
    let chair_in = |cwd: &Path| {
        let session = swarm(&home, &fake, &["session", "new", "lane"]);
        assert!(session.status.success(), "{}", stderr(&session));
        let session = String::from_utf8(session.stdout).unwrap();
        let env = [
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", session.trim()),
            ("SWARM_AGENT_ID", "orchestrator"),
        ];
        let cwd = cwd.to_string_lossy().into_owned();
        let output = swarm(
            &home,
            &env,
            &["launch", "orchestrator", "review.deep", "--cwd", &cwd],
        );
        assert!(output.status.success(), "{}", stderr(&output));
        stderr(&output)
    };
    let chair = git_repo(&home, "chair");
    chair_in(&chair);
    assert!(trusted(&home.join(".claude.json"), &chair));
    assert!(!profiles[0].join(".claude.json").exists());
    assert!(!chair.join(".herdr/workers").exists());

    // A chair in $HOME, or in a folder another account can write, gets no entry and shows
    // Claude's own trust screen.
    let shared = git_repo(&home, "shared");
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o777)).unwrap();
    for cwd in [&home, &shared] {
        let warning = chair_in(cwd);
        assert!(warning.contains("not pre-trusting for claude"), "{warning}");
        assert!(
            !trusted(&home.join(".claude.json"), cwd),
            "{}",
            cwd.display()
        );
        assert!(
            !trusted(&profiles[0].join(".claude.json"), cwd),
            "{}",
            cwd.display()
        );
    }
}
