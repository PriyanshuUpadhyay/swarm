// Account selection and session ownership tests.

use super::*;

#[test]
fn accounts_pick_prints_the_account_that_run_would_use() {
    let home = fixture("pick");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for provider in ["claude", "codex"] {
        let path = home.join(if provider == "claude" {
            ".claude/.profiles/work"
        } else {
            ".codex-work"
        });
        let cache = serde_json::json!({"accounts":[{"home":path,"meters":[{"provider":provider,"account":"work","label":"work","used_pct":10,"state":"fresh","as_of_seconds":now}]}]});
        std::fs::write(
            home.join(format!(".swarm/{provider}-usage.json")),
            cache.to_string(),
        )
        .unwrap();
        let (key, value) = if provider == "codex" {
            ("CODEX_HOME", path.display().to_string())
        } else {
            (
                "CLAUDE_SECURESTORAGE_CONFIG_DIR",
                format!("{}/.claude-work", home.display()),
            )
        };
        for selection in [None, Some("auto"), Some("work")] {
            let mut args = vec!["accounts", "pick", "--provider", provider];
            if let Some(selection) = selection {
                args.extend(["--account", selection]);
            }
            args.push("--json");
            let output = command(&home, &args).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let picked: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(picked["schema"], "swarm.account-pick.v1");
            assert_eq!(picked["name"], "work", "{picked}");
            assert_eq!(picked["home"], path.display().to_string());
            assert_eq!(picked["env"][key], value, "{picked}");
            assert!(picked["clear"].as_array().unwrap().iter().any(|k| k == key));
            assert!(!home.join("child-env").exists(), "pick runs no CLI session");
        }
        let output = command(
            &home,
            &[
                "accounts",
                "pick",
                "--provider",
                provider,
                "--account",
                "absent",
                "--json",
            ],
        )
        .output()
        .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unknown"));
    }
    tool(
        &home,
        "claude",
        "if [ \"$*\" = 'auth status' ]; then exit 1; fi",
    );
    std::fs::remove_file(home.join(".swarm/claude-usage.json")).unwrap();
    let output = command(
        &home,
        &["accounts", "pick", "--provider", "claude", "--json"],
    )
    .output()
    .unwrap();
    assert!(output.status.success());
    let picked: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(picked["name"].is_null(), "{picked}");
    assert_eq!(picked["env"], serde_json::json!({}));
    assert!(
        picked["clear"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k == "AGENT_PROFILE_LABEL")
    );
}

#[test]
fn accounts_pick_finds_the_codex_home_that_holds_a_session() {
    let home = fixture("session-owner");
    let rollout = |codex: &str, folder: &str, started: &str, id: &str, cwd: &Path| {
        let dir = home.join(codex).join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let meta = serde_json::json!({"type":"session_meta","payload":{"id":id,"cwd":cwd}});
        std::fs::write(
            dir.join(format!("rollout-{started}-{id}.jsonl")),
            format!("{meta}\n"),
        )
        .unwrap();
    };
    let (project, other, empty) = (home.join("project"), home.join("other"), home.join("empty"));
    for dir in [&project, &other, &empty] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let first = "0199a000-0000-7000-8000-000000000001";
    let second = "0199a000-0000-7000-8000-000000000002";
    let archived = "0199a000-0000-7000-8000-000000000003";
    rollout(
        ".codex-work",
        "sessions/2026/10/01",
        "2026-10-01T10-00-00",
        first,
        &project,
    );
    rollout(
        ".codex-b",
        "sessions/2026/10/02",
        "2026-10-02T10-00-00",
        second,
        &other,
    );
    rollout(
        ".codex-b",
        "archived_sessions",
        "2026-09-01T10-00-00",
        archived,
        &project,
    );
    let pick = |cwd: &Path, args: &[&str]| {
        let mut all = vec!["accounts", "pick", "--provider", "codex"];
        all.extend(args);
        all.push("--json");
        command(&home, &all).current_dir(cwd).output().unwrap()
    };
    let name = |output: std::process::Output| {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["name"].clone()
    };
    assert_eq!(name(pick(&home, &["--session", first])), "work");
    assert_eq!(
        name(pick(&home, &["--session", &first.to_uppercase()])),
        "work"
    );
    assert_eq!(name(pick(&home, &["--session", archived])), "b");
    assert_eq!(name(pick(&project, &["--last"])), "work");
    assert_eq!(name(pick(&project, &["--last", "--all"])), "b");
    for (cwd, args, message) in [
        (
            &home,
            vec!["--session", "0199a000-0000-7000-8000-00000000000f"],
            "not in any codex account",
        ),
        (&empty, vec!["--last"], "no codex session"),
        (
            &home,
            vec!["--session", first, "--account", "work"],
            "usage",
        ),
    ] {
        let output = pick(cwd, &args);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = command(
        &home,
        &[
            "accounts",
            "pick",
            "--provider",
            "claude",
            "--last",
            "--json",
        ],
    )
    .output()
    .unwrap();
    assert!(!output.status.success());
}
