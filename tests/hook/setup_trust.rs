// Tests for setup trust.
use super::*;

/// One `swarm setup` plan holds every pending write, each file with its group and its diff: the
/// hooks, the consent file, and the trust entries a launch in `--cwd` would write, also in a file
/// that the hooks change too. One digest applies them all, each trust entry is recorded, and a
/// second apply with nothing pending passes (ADR 0043).
#[test]
fn one_setup_plan_holds_every_pending_write_with_a_diff_per_file() {
    let home = scratch("setup-all");
    // A space in the folder must survive the printed apply command.
    let repo = git_repo(&home, "my app");
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(home.join(".codex/config.toml"), "model = \"o3\"\n").unwrap();
    let cwd = repo.to_string_lossy().into_owned();
    let status = || -> serde_json::Value {
        serde_json::from_slice(&setup(&home, &["status", "--json"]).stdout).unwrap()
    };
    assert_eq!(
        status(),
        serde_json::json!({"hooks": false, "guard": true, "trust": false})
    );

    let plan = setup(&home, &["--plan", "--json", "--cwd", &cwd]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    // No answer yet, so the plan offers standing consent.
    assert_eq!(plan["consent"], "standing");
    assert_eq!(plan["conflicts"], serde_json::json!([]));
    let files: Vec<(String, String)> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| {
            let path = file["path"].as_str().unwrap();
            let name = path.strip_prefix(&*home.to_string_lossy()).unwrap();
            (
                file["group"].as_str().unwrap().to_string(),
                name.to_string(),
            )
        })
        .collect();
    let expected = [
        ("hooks", "/.codex/config.toml"),
        ("hooks", "/.gemini/config/hooks.json"),
        ("trust", "/.swarm/consent.json"),
        ("trust", "/.codex/config.toml"),
        ("trust", "/.gemini/antigravity-cli/settings.json"),
        ("trust", "/.claude.json"),
    ]
    .map(|(group, path)| (group.to_string(), path.to_string()));
    assert_eq!(files, expected);
    let diffs: Vec<&str> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["diff"].as_str().unwrap())
        .collect();
    assert!(
        diffs[2].contains("+  \"trust\": \"standing\""),
        "{}",
        diffs[2]
    );
    assert!(
        diffs[3].contains(&format!("+[projects.\"{cwd}\"]")),
        "{}",
        diffs[3]
    );
    assert!(!diffs[3].contains("+[hooks.state"), "{}", diffs[3]);
    assert!(
        diffs[5].contains(&format!("{cwd}/.herdr/workers")),
        "{}",
        diffs[5]
    );

    let text = stdout(&setup(&home, &["--plan", "--cwd", &cwd]));
    let digest = plan["digest"].as_str().unwrap();
    assert!(
        text.ends_with(&format!(
            "Plan only. Nothing changed. Run `swarm setup hooks,trust --digest {digest} --cwd '{cwd}'` to apply.\n"
        )),
        "{text}"
    );

    let applied = setup(&home, &["--digest", digest, "--cwd", &cwd]);
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        status(),
        serde_json::json!({"hooks": true, "guard": true, "trust": true})
    );
    let config = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    assert!(config.contains("[hooks.state.") && config.contains(&format!("[projects.\"{cwd}\"]")));
    let list = swarm(&home, &[], &["managed", "list", "--json"], "");
    let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let trust = list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["writer"] == "launch.trust" && entry["recorded"] == true)
        .count();
    // The consent file, Codex, AGY, and Claude.
    assert_eq!(trust, 4, "{list}");

    let again = setup(&home, &["--digest", digest, "--cwd", &cwd]);
    assert!(again.status.success(), "{again:?}");
    assert!(stdout(&again).contains("already set up"), "{again:?}");

    // One named item plans only that item; a folder that fails the trust check is skipped with its reason.
    let only = setup(
        &home,
        &[
            "trust",
            "--plan",
            "--json",
            "--cwd",
            &*home.to_string_lossy(),
        ],
    );
    let only: serde_json::Value = serde_json::from_slice(&only.stdout).unwrap();
    assert_eq!(only["files"], serde_json::json!([]));
    assert_eq!(only["skipped"][0]["group"], "trust");
    assert!(
        only["skipped"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("not in a git repository"),
        "{only}"
    );
    let unknown = setup(&home, &["sound", "--plan"]);
    assert!(!unknown.status.success());
    std::fs::remove_dir_all(&home).unwrap();
}

/// A Codex project the owner set to another trust level is left as it is, and the plan says why
/// the pane will ask (ADR 0043, C3).
#[test]
fn a_codex_folder_the_owner_left_untrusted_is_skipped_with_its_reason() {
    let home = scratch("setup-untrusted");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let config = format!("[projects.\"{cwd}\"]\ntrust_level = \"untrusted\"\n");
    std::fs::write(home.join(".codex/config.toml"), &config).unwrap();

    let plan = setup(&home, &["trust", "--plan", "--json", "--cwd", &cwd]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let codex = home.join(".codex/config.toml");
    assert!(
        plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["path"] != codex.to_string_lossy().as_ref()),
        "{plan}"
    );
    let skipped = &plan["skipped"];
    assert_eq!(skipped.as_array().unwrap().len(), 1, "{plan}");
    assert_eq!(skipped[0]["group"], "trust");
    let reason = skipped[0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("you marked this folder untrusted; the pane asks"),
        "{reason}"
    );
    assert!(reason.contains(&*codex.to_string_lossy()), "{reason}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// A spare Claude profile whose config swarm cannot edit does not block setup: as a launch with no
/// picked account does, setup names it under skipped and plans every other file.
#[test]
fn a_read_only_spare_claude_profile_is_skipped_and_the_rest_is_planned() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("setup-spare-claude");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let spare = home.join(".claude/.profiles/spare/.claude.json");
    std::fs::create_dir_all(spare.parent().unwrap()).unwrap();
    std::fs::write(&spare, "{}\n").unwrap();
    std::fs::set_permissions(&spare, std::fs::Permissions::from_mode(0o444)).unwrap();

    let plan = setup(&home, &["--plan", "--json", "--cwd", &cwd]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["conflicts"], serde_json::json!([]), "{plan}");
    let paths: Vec<&str> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    let main = home.join(".claude.json");
    assert!(paths.contains(&&*main.to_string_lossy()), "{paths:?}");
    assert!(!paths.contains(&&*spare.to_string_lossy()), "{paths:?}");
    let skipped = plan["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{plan}");
    assert_eq!(skipped[0]["group"], "trust");
    let reason = skipped[0]["reason"].as_str().unwrap();
    assert!(reason.contains(&*spare.to_string_lossy()), "{reason}");

    let applied = setup(
        &home,
        &["--digest", plan["digest"].as_str().unwrap(), "--cwd", &cwd],
    );
    assert!(applied.status.success(), "{applied:?}");
    assert!(std::fs::read_to_string(&main).unwrap().contains(&cwd));
    assert_eq!(std::fs::read_to_string(&spare).unwrap(), "{}\n");
    std::fs::set_permissions(&spare, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
}

/// An apply plans the trust group once and each trust write plans again only its own file, so a
/// write under the trust lock runs no second `git rev-parse` (L-1).
#[test]
fn an_apply_checks_the_folder_with_git_once() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("setup-one-git");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let (bin, log) = (home.join("bin"), home.join("git.log"));
    std::fs::create_dir_all(&bin).unwrap();
    let shim = bin.join("git");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{}'\nexec /usr/bin/git \"$@\"\n",
        log.display()
    );
    std::fs::write(&shim, script).unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = |args: &[&str]| {
        let mut command = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), &home);
        let path = format!("{}:/usr/bin:/bin", bin.display());
        command
            .env("PATH", path)
            .current_dir(&home)
            .arg("setup")
            .args(args);
        piped(command, "")
    };

    let plan = run(&["trust", "--plan", "--json", "--cwd", &cwd]);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert!(plan["files"].as_array().unwrap().len() > 1, "{plan}");
    std::fs::remove_file(&log).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    let applied = run(&["trust", "--digest", digest, "--cwd", &cwd]);
    assert!(applied.status.success(), "{applied:?}");
    let calls = std::fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().count(), 1, "{calls}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// A Codex config.toml that swarm cannot read is a hooks conflict, and the trust group plans
/// nothing on it, so the owner sees no false diff; it names the conflict instead.
#[test]
fn a_broken_codex_config_gets_no_trust_diff_only_the_reason() {
    let home = scratch("setup-broken-trust");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let codex = home.join(".codex/config.toml");
    std::fs::write(&codex, "model = \"gpt-5.5\"\nbroken = = \n").unwrap();

    let plan = setup(&home, &["--plan", "--json", "--cwd", &cwd]);
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert!(
        plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["path"] != codex.to_string_lossy().as_ref()),
        "{plan}"
    );
    let conflicts = plan["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{plan}");
    assert_eq!(conflicts[0]["group"], "hooks");
    let skipped = &plan["skipped"][0];
    assert_eq!(skipped["group"], "trust", "{plan}");
    let reason = skipped["reason"].as_str().unwrap();
    assert!(
        reason.contains(&*codex.to_string_lossy()) && reason.contains("not valid TOML"),
        "{reason}"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// A running Claude rewrites `~/.claude.json` at any moment, so the digest of a trust entry covers
/// only the entry the plan adds and the value it replaces: a hand-run apply after another key
/// changed still passes, and a change to the entry itself is refused (ADR 0043, L-4).
#[test]
fn a_trust_digest_covers_only_the_entries_the_plan_writes() {
    let home = scratch("setup-trust-digest");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let claude = home.join(".claude.json");
    std::fs::write(&claude, "{\"numStartups\": 1}\n").unwrap();
    let digest = |home: &Path| -> String {
        let plan = setup(home, &["trust", "--plan", "--json", "--cwd", &cwd]);
        let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
        plan["digest"].as_str().unwrap().to_string()
    };

    let planned = digest(&home);
    let pool = format!("{cwd}/.herdr/workers");
    let mut value = serde_json::json!({"numStartups": 2});
    value["projects"][&pool] = serde_json::json!({"hasTrustDialogAccepted": false});
    std::fs::write(&claude, value.to_string()).unwrap();
    let refused = setup(&home, &["trust", "--digest", &planned, "--cwd", &cwd]);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("a file changed after the plan"),
        "{refused:?}"
    );

    let planned = digest(&home);
    value["numStartups"] = 3.into();
    value["tipsHistory"] = serde_json::json!({"x": 1});
    std::fs::write(&claude, value.to_string()).unwrap();
    let applied = setup(&home, &["trust", "--digest", &planned, "--cwd", &cwd]);
    assert!(applied.status.success(), "{applied:?}");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude).unwrap()).unwrap();
    assert_eq!(written["projects"][&pool]["hasTrustDialogAccepted"], true);
    assert_eq!(written["numStartups"], 3);
    std::fs::remove_dir_all(&home).unwrap();
}
