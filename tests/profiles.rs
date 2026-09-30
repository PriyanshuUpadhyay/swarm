use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const OLD_CONFIG: &str = r#"{
  "routes": {"code": ["codex-sol-high-agent", "claude-opus-high-agent"]},
  "runners": {
    "codex-sol-high-agent": {"provider": "codex", "model": "gpt-sol", "effort": "high"},
    "claude-opus-high-agent": {"provider": "claude", "model": "opus"}
  }
}"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-profiles-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn old_config(home: &Path, text: &str) -> PathBuf {
    let path = home.join(".config/agent-routing/roles.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

/// A bin dir with an executable stand-in for each named CLI.
fn clis(home: &Path, names: &[&str]) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in names {
        std::fs::write(bin.join(name), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(bin.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// Runs the built binary with only HOME set, so no user config or routing env leaks in. SWARM_HOME
/// pins the data to HOME, so a branch build (ADR 0027) reads the same place as a `main` one.
fn swarm(home: &Path, env: &[(&str, &Path)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .args(args);
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn profiles_file(home: &Path) -> PathBuf {
    home.join(".swarm/profiles.json")
}

#[test]
fn the_first_read_imports_the_old_config_once_and_never_writes_it() {
    let home = scratch("import");
    let old = old_config(&home, OLD_CONFIG);

    let listing = json(&swarm(&home, &[], &["roles", "--json"]));

    assert_eq!(listing["profiles"][0]["name"], "chat");
    assert_eq!(listing["profiles"][1]["name"], "code");
    let code = &listing["profiles"][1]["runners"];
    assert_eq!(code[0]["provider"], "codex");
    assert_eq!(code[1]["provider"], "claude");
    // An old runner with no effort takes the provider's default.
    assert_eq!(code[1]["effort"], "medium");
    assert_eq!(listing["imported"]["from"], old.to_string_lossy().as_ref());
    assert_eq!(std::fs::read_to_string(&old).unwrap(), OLD_CONFIG);
    assert!(profiles_file(&home).exists());

    // A later edit to the old file does not import again, and each read says so.
    old_config(&home, &OLD_CONFIG.replace("gpt-sol", "gpt-other"));
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
        .unwrap();
    let output = swarm(&home, &[], &["roles", "--json"]);
    let again = json(&output);
    assert_eq!(again["profiles"][1]["runners"][0]["model"], "gpt-sol");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("changed after import"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn roles_get_keeps_the_keys_that_orchestrators_read() {
    let home = scratch("get");
    old_config(&home, OLD_CONFIG);
    let bin = clis(&home, &["claude", "codex"]);

    let resolved = json(&swarm(
        &home,
        &[("PATH", &bin)],
        &["roles", "get", "code", "--provider", "claude"],
    ));

    assert_eq!(resolved["role"], "code");
    assert_eq!(resolved["runnerId"], "code#2");
    assert_eq!(resolved["provider"], "claude");
    assert_eq!(resolved["model"], "opus");
    assert_eq!(resolved["fallbackRunnerIds"], serde_json::json!(["code#1"]));
    assert!(resolved.get("substitutedFor").is_none());

    // A provider the profile has no runner of is refused, not swapped for another provider.
    let absent = swarm(
        &home,
        &[("PATH", &bin)],
        &["roles", "get", "code", "--provider", "agy"],
    );
    assert!(!absent.status.success());
    assert!(
        String::from_utf8_lossy(&absent.stderr).contains("swarm: code: profile has no agy runner"),
        "{}",
        String::from_utf8_lossy(&absent.stderr)
    );
}

#[test]
fn a_route_that_breaks_a_profile_rule_is_not_imported_and_the_rest_are() {
    let home = scratch("import-partial");
    old_config(
        &home,
        r#"{"routes": {"code": ["codex-sol-high-agent"], "Review.Deep": ["codex-sol-high-agent"],
                       "plan": ["claude-opus-none-agent"]},
            "runners": {
              "codex-sol-high-agent": {"provider": "codex", "model": "gpt-sol", "effort": "high"},
              "claude-opus-none-agent": {"provider": "claude", "model": "opus", "effort": "none"}
            }}"#,
    );

    let listing = json(&swarm(&home, &[], &["roles", "--json"]));

    let names: Vec<&str> = listing["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|profile| profile["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["chat", "code"]);
    assert_eq!(
        listing["imported"]["unmapped"],
        serde_json::json!(["Review.Deep", "plan"])
    );
}

#[test]
fn a_runner_whose_cli_is_not_on_path_is_skipped_and_named() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("skip");
    old_config(
        &home,
        r#"{"routes": {"review.gate": ["claude-opus-xhigh-agent"]},
            "runners": {
              "claude-opus-xhigh-agent": {"provider": "claude", "model": "opus", "effort": "xhigh"},
              "codex-sol-xhigh-agent": {"provider": "codex", "model": "gpt-sol", "effort": "xhigh"}
            },
            "substitutes": {"claude-opus-xhigh-agent": ["codex-sol-xhigh-agent"]}}"#,
    );
    let bin = clis(&home, &["codex"]);
    // A claude file that cannot run is not an installed CLI.
    std::fs::write(bin.join("claude"), "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o644)).unwrap();

    let output = swarm(&home, &[("PATH", &bin)], &["roles", "get", "review.gate"]);

    let resolved = json(&output);
    assert_eq!(resolved["runnerId"], "review.gate#2");
    assert_eq!(resolved["substitutedFor"], "review.gate#1");
    assert_eq!(resolved["skipped"][0]["code"], "cli_missing");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "swarm: review.gate: skipped claude/opus/xhigh: claude CLI not found on PATH"
        )
    );

    let none = swarm(
        &home,
        &[("PATH", &home.join("empty"))],
        &["roles", "get", "review.gate"],
    );
    assert!(!none.status.success());
    let stderr = String::from_utf8_lossy(&none.stderr);
    assert!(
        stderr.contains("swarm: review.gate: no runner can run"),
        "{stderr}"
    );
    assert!(
        stderr.contains("  2 codex/gpt-sol/xhigh: codex CLI not found on PATH"),
        "{stderr}"
    );
}

#[test]
fn a_missing_explicit_old_config_is_an_error() {
    let home = scratch("missing");
    old_config(&home, OLD_CONFIG);

    let output = swarm(
        &home,
        &[("AGENT_ROUTING_CONFIG", &home.join("nope.json"))],
        &["roles", "--json"],
    );

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("AGENT_ROUTING_CONFIG names a missing file")
    );
}

#[test]
fn save_writes_through_a_link_keeps_a_backup_and_refuses_a_stale_revision() {
    let home = scratch("save");
    old_config(&home, OLD_CONFIG);
    json(&swarm(&home, &[], &["roles", "--json"]));
    // The owner links profiles.json into dotfiles (ADR 0030).
    let real = home.join("dotfiles-profiles.json");
    std::fs::rename(profiles_file(&home), &real).unwrap();
    std::os::unix::fs::symlink(&real, profiles_file(&home)).unwrap();
    let before = json(&swarm(&home, &[], &["roles", "--json"]));
    let revision = before["revision"].as_str().unwrap();
    let edited = r#"{"name": "code", "runners": [
        {"provider": "claude", "model": "opus", "effort": "high", "permission": "auto"},
        {"provider": "codex", "model": "gpt-sol", "effort": "high"}]}"#;

    let saved = json(&swarm(
        &home,
        &[],
        &["roles", "save", "--revision", revision, edited],
    ));

    assert_ne!(saved["revision"], revision);
    assert!(
        std::fs::symlink_metadata(profiles_file(&home))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&real).unwrap()).unwrap();
    assert_eq!(on_disk["profiles"][1]["runners"][0]["provider"], "claude");
    let backups = std::fs::read_dir(home.join(".swarm"))
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".bak")
        })
        .count();
    assert_eq!(backups, 1);

    let stale = swarm(
        &home,
        &[],
        &["roles", "save", "--revision", revision, edited],
    );
    assert!(!stale.status.success());
    assert!(
        String::from_utf8_lossy(&stale.stderr)
            .contains("profiles changed on disk; reload and try again")
    );

    let current = saved["revision"].as_str().unwrap();
    let bad = r#"{"name": "code", "runners": [{"provider": "claude", "model": "--x", "effort": "high"}]}"#;
    let refused = swarm(&home, &[], &["roles", "save", "--revision", current, bad]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("invalid model '--x'"));
}

#[test]
fn with_no_file_the_built_in_profiles_show_and_nothing_is_written() {
    let home = scratch("default");

    let listing = json(&swarm(&home, &[], &["roles", "--json"]));

    assert_eq!(listing["profiles"][0]["name"], "chat");
    assert!(listing.get("imported").is_none());
    assert!(!profiles_file(&home).exists());

    // The first save writes the defaults plus the change.
    let revision = listing["revision"].as_str().unwrap();
    let chat = r#"{"name": "chat", "runners": [{"provider": "codex", "model": "gpt-6.1-sol", "effort": "xhigh"}]}"#;
    json(&swarm(
        &home,
        &[],
        &["roles", "save", "--revision", revision, chat],
    ));
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(profiles_file(&home)).unwrap()).unwrap();
    assert_eq!(on_disk["profiles"][0]["runners"][0]["effort"], "xhigh");
    assert_eq!(
        on_disk["profiles"].as_array().unwrap().len(),
        listing["profiles"].as_array().unwrap().len()
    );
}

#[test]
fn a_broken_link_is_an_error_not_the_default() {
    let home = scratch("dangling");
    std::fs::create_dir_all(home.join(".config/agent-routing")).unwrap();
    std::os::unix::fs::symlink(
        home.join("gone.json"),
        home.join(".config/agent-routing/roles.json"),
    )
    .unwrap();

    let output = swarm(&home, &[], &["roles", "--json"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read"));
}

/// The owner's roles.json as it was on 2026-09-30. Every route must come through with its
/// primary runner first, so an import cannot change what a role starts with.
#[test]
fn the_owners_config_imports_with_no_route_lost() {
    let home = scratch("owner");
    let text = include_str!("fixtures/owner-roles.json");
    old_config(&home, text);
    let old: serde_json::Value = serde_json::from_str(text).unwrap();

    let listing = json(&swarm(&home, &[], &["roles", "--json"]));

    let profiles = listing["profiles"].as_array().unwrap();
    let routes = old["routes"].as_object().unwrap();
    assert_eq!(profiles.len(), routes.len() + 1);
    assert_eq!(profiles[0]["name"], "chat");
    assert_eq!(listing["imported"]["unmapped"], serde_json::json!([]));
    for ((route, ids), profile) in routes.iter().zip(&profiles[1..]) {
        assert_eq!(profile["name"], route.as_str());
        let primary = &old["runners"][ids[0].as_str().unwrap()];
        let first = &profile["runners"][0];
        for key in [
            "provider",
            "model",
            "effort",
            "sandbox",
            "approval",
            "permission",
        ] {
            assert_eq!(first.get(key), primary.get(key), "{route} {key}");
        }
    }
    assert!(
        profiles
            .iter()
            .any(|profile| profile["name"] == "ink.build")
    );
}

/// A yelo stand-in that prints `claude_rows` for Claude and one healthy account for any other CLI.
fn yelo(bin: &Path, claude_rows: &str) {
    use std::os::unix::fs::PermissionsExt;
    let healthy = r#"[{"name":"spare","dir":"/p/spare","signed_in":true,"remaining":60}]"#;
    std::fs::write(
        bin.join("yelo"),
        format!("#!/bin/sh\ncase \"$*\" in *claude*) echo '{claude_rows}' ;; *) echo '{healthy}' ;; esac\n"),
    )
    .unwrap();
    std::fs::set_permissions(bin.join("yelo"), std::fs::Permissions::from_mode(0o755)).unwrap();
}

const CLAUDE_FIRST: &str = r#"{
  "routes": {"code": ["claude-opus-high-agent", "codex-sol-high-agent"]},
  "runners": {
    "claude-opus-high-agent": {"provider": "claude", "model": "opus", "effort": "high"},
    "codex-sol-high-agent": {"provider": "codex", "model": "gpt-sol", "effort": "high"}
  }
}"#;

#[test]
fn a_provider_with_every_account_low_or_signed_out_is_skipped() {
    let home = scratch("usage");
    old_config(&home, CLAUDE_FIRST);
    let bin = clis(&home, &["claude", "codex"]);
    let path = [("PATH", bin.as_path())];

    yelo(
        &bin,
        r#"[{"name":"nearly-spent","dir":"/p/nearly-spent","signed_in":true,"remaining":2},{"name":"signed-out","dir":"/p/signed-out","signed_in":false,"remaining":90}]"#,
    );
    let low = swarm(&home, &path, &["roles", "get", "code"]);
    let resolved = json(&low);
    assert_eq!(resolved["runnerId"], "code#2");
    assert_eq!(resolved["skipped"][0]["code"], "low_usage");
    assert!(
        String::from_utf8_lossy(&low.stderr)
            .contains("swarm: code: skipped claude/opus/high: usage 2% left (threshold 5%)")
    );

    yelo(
        &bin,
        r#"[{"name":"signed-out","dir":"/p/signed-out","signed_in":false,"remaining":80}]"#,
    );
    let signed_out = json(&swarm(&home, &path, &["roles", "get", "code"]));
    assert_eq!(signed_out["runnerId"], "code#2");
    assert_eq!(signed_out["skipped"][0]["code"], "signed_out");
    assert_eq!(
        signed_out["skipped"][0]["text"],
        "no claude account is signed in"
    );

    yelo(
        &bin,
        r#"[{"name":"personal","dir":"/p/personal","signed_in":true,"remaining":40}]"#,
    );
    let healthy = json(&swarm(&home, &path, &["roles", "get", "code"]));
    assert_eq!(healthy["runnerId"], "code#1");

    let check = json(&swarm(&home, &path, &["roles", "check", "--json"]));
    let code = check["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["name"] == "code")
        .unwrap();
    assert_eq!(code["pick"], 0);
    assert_eq!(code["skipped"], serde_json::json!([]));
}

#[test]
fn a_usage_read_that_hangs_counts_as_can_run_after_its_deadline() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("slow");
    old_config(&home, CLAUDE_FIRST);
    let bin = clis(&home, &["claude", "codex"]);
    // A full path: a copy of /bin/sleep in the test's bin dir is killed by code signing.
    std::fs::write(bin.join("yelo"), "#!/bin/sh\n/bin/sleep 30\n").unwrap();
    std::fs::set_permissions(bin.join("yelo"), std::fs::Permissions::from_mode(0o755)).unwrap();

    let started = std::time::Instant::now();
    let resolved = json(&swarm(&home, &[("PATH", &bin)], &["roles", "get", "code"]));

    assert_eq!(resolved["runnerId"], "code#1");
    let waited = started.elapsed();
    assert!(waited >= std::time::Duration::from_secs(2), "{waited:?}");
    assert!(waited < std::time::Duration::from_secs(5), "{waited:?}");
}
