// Launch trust consent and recorded changes tests.

use super::*;

/// With no consent (ADR 0043), a seat's launch writes no trust entry. It prints the bare
/// `trust-pending` line the chair reads (the app reads only `trusted` lines), the diff it would
/// write, and the command that approves it, and the seat still starts, at its CLI's own trust
/// prompt.
#[test]
fn a_seat_launch_with_no_consent_writes_no_trust_and_prints_the_plan() {
    let home = scratch("ask");
    let env = trust_session(&home);
    let repo = git_repo(&home, "my app");
    let config = home.join(".codex/config.toml");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(&config, "model = \"gpt\"\n").unwrap();

    let output = launch_in(&home, &env, "seat", "review.deep", &repo);
    let err = stderr(&output);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "model = \"gpt\"\n"
    );
    assert!(
        err.lines()
            .any(|line| line == format!("trust-pending codex {}", repo.display())),
        "{err}"
    );
    assert!(
        err.contains(&format!("+[projects.\"{}\"]", repo.display())),
        "{err}"
    );
    assert!(err.contains("+trust_level = \"trusted\""), "{err}");
    assert!(
        err.contains(&format!(
            "swarm setup trust --plan --cwd '{}'",
            repo.display()
        )),
        "{err}"
    );
    assert!(recorded_trust(&home).is_empty());

    // An unreadable consent file is no consent (R4), and swarm says so.
    std::fs::write(home.join(".swarm/consent.json"), "{not json").unwrap();
    let err = stderr(&launch_in(&home, &env, "seat-2", "review.deep", &repo));
    assert!(err.contains("consent"), "{err}");
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "model = \"gpt\"\n"
    );

    // Standing consent: the entry is written, recorded, and named.
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
    let err = stderr(&launch_in(&home, &env, "seat-3", "review.deep", &repo));
    assert!(
        err.lines()
            .any(|line| line == format!("trusted codex {}", repo.display())),
        "{err}"
    );
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("trust_level = \"trusted\"")
    );
    let recorded = recorded_trust(&home);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0]["state"], "present");
}

/// The app makes a chair launch in the folder the owner picked, and hides its pane, so with no
/// consent that pick is consent for that one folder (owner answer I1). The write is recorded and
/// shown, with its diff, never made silently.
#[test]
fn a_chair_launch_trusts_its_picked_folder_and_shows_the_write() {
    let home = scratch("chair-pick");
    let env = trust_session(&home);
    let repo = git_repo(&home, "picked");

    let err = stderr(&launch_in(&home, &env, "orchestrator", "chair", &repo));
    assert!(trusted(&home.join(".claude.json"), &repo));
    assert!(
        err.lines()
            .any(|line| line == format!("trusted claude {}", repo.display())),
        "{err}"
    );
    assert!(
        err.contains("+      \"hasTrustDialogAccepted\": true"),
        "{err}"
    );
    let recorded = recorded_trust(&home);
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0]["path"][1], repo.to_string_lossy().as_ref());

    // A Claude `false` is replaced, and the record keeps it for revert (Q4).
    let other = git_repo(&home, "seen");
    let claude = home.join(".claude.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude).unwrap()).unwrap();
    value["projects"][other.to_string_lossy().as_ref()] =
        serde_json::json!({"hasTrustDialogAccepted": false});
    std::fs::write(&claude, value.to_string()).unwrap();
    // Each chair gets its own session, as each app chat does.
    let env = trust_session(&home);
    launch_in(&home, &env, "orchestrator", "chair", &other);
    assert!(trusted(&claude, &other));
    let before: Vec<_> = recorded_trust(&home)
        .into_iter()
        .filter(|entry| entry["path"][1] == other.to_string_lossy().as_ref())
        .map(|entry| entry["before"].clone())
        .collect();
    assert_eq!(before, [serde_json::json!(false)]);
}

/// AGY has one settings file, and no account to fall back on, so a standing-consent launch that
/// cannot write its trust entry stops, as before consent (03-contracts section 7).
#[test]
fn an_agy_trust_write_that_fails_stops_the_launch() {
    let home = scratch("agy-fail");
    let env = trust_session(&home);
    tool(&home, "agy", "true");
    std::fs::write(
        home.join(".config/agent-routing/roles.json"),
        r#"{"routes": {"agy.seat": ["agy-flash"]},
            "runners": {"agy-flash": {"provider": "agy", "model": "flash"}}}"#,
    )
    .unwrap();
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
    let settings = home.join(".gemini/antigravity-cli/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "[]\n").unwrap();
    let repo = git_repo(&home, "app");
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();

    let cwd = repo.to_string_lossy().into_owned();
    let output = swarm(
        &home,
        &env,
        &["launch", "agy-seat", "agy.seat", "--cwd", &cwd],
    );
    assert!(!output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("not a JSON object"),
        "{}",
        stderr(&output)
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), "[]\n");
}

/// Standing consent covers only a folder that passes the trust check (ADR 0043), so a Claude
/// seat in a plain folder gets no entry and asks in its pane.
#[test]
fn a_claude_seat_in_a_folder_that_fails_the_trust_check_gets_no_entry() {
    let home = scratch("seat-loose");
    let env = trust_session(&home);
    std::fs::write(home.join(".swarm/consent.json"), r#"{"trust": "standing"}"#).unwrap();
    let loose = home.join("loose");
    std::fs::create_dir_all(&loose).unwrap();

    let err = stderr(&launch_in(&home, &env, "seat", "chair", &loose));
    assert!(
        !trusted(&home.join(".claude.json"), &loose.join(".herdr/workers")),
        "{err}"
    );
    assert!(recorded_trust(&home).is_empty(), "{err}");
    assert!(err.contains("not pre-trusting for claude"), "{err}");
}

/// The app reads a launch's stderr for bare `model`, `account`, and `trusted` lines. A chair's
/// trust diff prints next to them, and each of its lines keeps its diff mark, so a context line
/// such as ` model = "gpt-5.5"` is never a key line (L-12).
#[test]
fn a_chair_trust_diff_never_prints_a_bare_key_line() {
    let home = scratch("chair-diff");
    let env = trust_session(&home);
    let repo = git_repo(&home, "picked");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"gpt-5.5\"\n").unwrap();

    let err = stderr(&launch_in(
        &home,
        &env,
        "orchestrator",
        "review.deep",
        &repo,
    ));
    assert!(err.contains("\n model = \"gpt-5.5\"\n"), "{err}");
    let keys: Vec<&str> = err
        .lines()
        .filter(|line| line.starts_with("model ") || line.starts_with("trusted "))
        .collect();
    assert_eq!(
        keys,
        [
            format!("trusted codex {}", repo.display()).as_str(),
            "model gpt-6-luna"
        ],
        "{err}"
    );
}

/// A resuming Claude seat runs in cwd itself (`claude_child`), so with no consent its launch
/// reports cwd as pending, and the approve command it prints plans the Claude entry for that same
/// folder; once applied, the next resumed launch has nothing pending.
#[test]
fn a_resumed_claude_seats_approve_command_plans_the_folder_it_reports() {
    let home = scratch("resume-ask");
    let env = trust_session(&home);
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let launch = |seat: &str| {
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let args = [
            "launch", seat, "chair", "--cwd", &cwd, "--", "--resume", "abc",
        ];
        let output = swarm(&home, &env, &args);
        assert!(output.status.success(), "{}", stderr(&output));
        stderr(&output)
    };

    let err = launch("seat");
    assert!(
        err.lines()
            .any(|line| line == format!("trust-pending claude {cwd}")),
        "{err}"
    );
    let approve = format!("swarm setup trust --plan --cwd '{cwd}' --resume");
    assert!(err.contains(&approve), "{err}");

    let plan = swarm(
        &home,
        &[],
        &[
            "setup", "trust", "--plan", "--json", "--cwd", &cwd, "--resume",
        ],
    );
    assert!(plan.status.success(), "{}", stderr(&plan));
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    let applied = swarm(
        &home,
        &[],
        &[
            "setup", "trust", "--digest", digest, "--cwd", &cwd, "--resume",
        ],
    );
    assert!(applied.status.success(), "{}", stderr(&applied));
    assert!(trusted(&home.join(".claude.json"), &repo));
    assert!(!trusted(
        &home.join(".claude.json"),
        &repo.join(".herdr/workers")
    ));

    let err = launch("seat-2");
    assert!(!err.contains("trust-pending claude"), "{err}");
}
