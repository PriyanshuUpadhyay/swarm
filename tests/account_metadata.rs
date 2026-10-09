use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(role: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "swarm-account-metadata-{role}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(home.join(".swarm/swarm-home"), "swarm\n").unwrap();
    let home = std::fs::canonicalize(home).unwrap();
    tool(
        &home,
        "codex",
        &format!(
            r#"if [ "$*" = login ]; then
  mkdir -p "$CODEX_HOME"
  echo credential-sentinel > "$CODEX_HOME/auth.json"
  printf '%s\n' "$*" "$CODEX_HOME" > "$HOME/login-exec"
  exit 0
fi
{}"#,
            include_str!("fixtures/accounts/work-app-server.sh")
        ),
    );
    tool(
        &home,
        "claude",
        r#"if [ "$*" = 'auth login' ]; then
  mkdir -p "$CLAUDE_CONFIG_DIR"
  echo credential-sentinel > "$CLAUDE_CONFIG_DIR/token-store"
  printf '%s\n' "$*" "$CLAUDE_CONFIG_DIR" "$CLAUDE_SECURESTORAGE_CONFIG_DIR" "$AGENT_PROFILE_LABEL" > "$HOME/login-exec"
else
  echo '{"loggedIn":true,"email":"owner@example.test"}'
fi"#,
    );
    std::fs::write(home.join(".swarm/adapters/fake.conf"), "self = printf chair\nspawn = test -f \"$HOME/.swarm/accounts.toml\" && printf '%s' \"$PWD\" > \"$HOME/pane-cwd\" && printf pane-work\nring = printf '%s' \"$SWARM_TEXT\" > \"$HOME/login-line\" && /bin/sh -c \"$SWARM_TEXT\"\nlist = true\nclose = true\ncapture = true\n").unwrap();
    home
}

fn tool(home: &Path, name: &str, body: &str) {
    let path = home.join("bin").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn command(home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env("SWARM_ADAPTER", "fake")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .current_dir(home)
        .args(args);
    command
}

fn run(home: &Path, args: &[&str]) -> Output {
    command(home, args).output().unwrap()
}
fn json(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn revision(home: &Path) -> String {
    json(run(home, &["accounts", "--provider", "agy", "--json"]))["revision"]
        .as_str()
        .unwrap()
        .to_string()
}
fn login<'a>(provider: &'a str, name: &'a str, revision: &'a str) -> Vec<&'a str> {
    vec![
        "accounts",
        "login",
        "--provider",
        provider,
        "--name",
        name,
        "--revision",
        revision,
        "--json",
    ]
}

#[test]
fn login_registers_before_open_and_uses_fixed_native_argv_and_home() {
    let home = fixture("work-login");
    let initial = revision(&home);
    let mut opened = json(run(&home, &login("codex", "work", &initial)));
    assert_eq!(opened["pane"], "pane-work");
    assert_ne!(opened["revision"], initial);
    opened["pane"] = "<pane id>".into();
    opened["revision"] = "<new revision>".into();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/accounts/work-login.json")).unwrap();
    assert_eq!(sorted(opened), sorted(expected));
    let metadata = std::fs::read_to_string(home.join(".swarm/accounts.toml")).unwrap();
    assert!(metadata.contains("version = 1"));
    assert!(metadata.contains("[[accounts]]"));
    assert!(metadata.contains("provider = \"codex\""));
    assert!(metadata.contains("name = \"work\""));
    assert!(!metadata.contains("credential"));
    assert!(!metadata.contains("owner@example.test"));
    assert_eq!(
        std::fs::read_to_string(home.join("login-exec")).unwrap(),
        format!("login\n{}\n", home.join(".codex-work").display())
    );
    assert_eq!(
        std::fs::read_to_string(home.join("pane-cwd")).unwrap(),
        home.to_string_lossy()
    );
    let list = json(run(&home, &["accounts", "--provider", "codex", "--json"]));
    assert_eq!(list["modified"], true);
    assert!(
        list["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| account["name"] == "work")
    );
}

#[test]
fn claude_login_clears_inherited_keys_and_uses_the_requested_directory() {
    let home = fixture("personal-login");
    let cwd = home.join("work project 'quoted'");
    std::fs::create_dir(&cwd).unwrap();
    let initial = revision(&home);
    let cwd_string = cwd.to_string_lossy().into_owned();
    let mut args = login("claude", "personal", &initial);
    args.extend(["--cwd", &cwd_string]);
    let output = command(&home, &args)
        .env("CLAUDE_CONFIG_DIR", "/wrong")
        .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/wrong-store")
        .env("AGENT_PROFILE_LABEL", "wrong")
        .output()
        .unwrap();
    let opened = json(output);
    assert_eq!(opened["account"], "personal");
    assert_eq!(
        std::fs::read_to_string(home.join("pane-cwd")).unwrap(),
        cwd_string
    );
    let native = home.join(".claude/.profiles/personal");
    assert_eq!(
        std::fs::read_to_string(home.join("login-exec")).unwrap(),
        format!(
            "auth login\n{}\n{}\npersonal\n",
            native.display(),
            home.join(".claude-personal").display()
        )
    );
}

#[test]
fn invalid_name_cwd_and_stale_revision_never_open_a_pane() {
    let home = fixture("spare-rejections");
    let initial = revision(&home);
    for name in [
        "",
        "auto",
        "default",
        "current",
        ".",
        "..",
        "work..personal",
        "Work",
        "../work",
        "work/personal",
        "work\\personal",
        "work personal",
        "work\n",
        "é",
        "work$(id)",
    ] {
        let output = run(&home, &login("codex", name, &initial));
        assert!(!output.status.success(), "accepted {name:?}");
        assert!(!home.join(".swarm/accounts.toml").exists());
        assert!(!home.join("pane-cwd").exists());
    }
    let executable_file = home.join("bin/codex").to_string_lossy().into_owned();
    for cwd in ["relative", "/missing-sw-arm-directory", &executable_file] {
        let mut args = login("codex", "work", &initial);
        args.extend(["--cwd", cwd]);
        assert!(!run(&home, &args).status.success());
        assert!(!home.join(".swarm/accounts.toml").exists());
    }
    let output = run(&home, &login("codex", "work", "stale"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("changed"));
    assert!(!home.join("pane-cwd").exists());
}

#[test]
fn pane_failure_keeps_registered_metadata_and_native_credentials() {
    let home = fixture("work-pane-failure");
    let native = home.join(".codex-work");
    std::fs::create_dir(&native).unwrap();
    std::fs::write(native.join("auth.json"), b"existing-credential-sentinel").unwrap();
    std::fs::write(home.join(".swarm/adapters/fake.conf"), "self = true\nspawn = echo private-pane-error >&2; exit 8\nring = true\nlist = true\nclose = true\ncapture = true\n").unwrap();
    let output = run(&home, &login("codex", "work", &revision(&home)));
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-pane-error"));
    assert!(
        std::fs::read_to_string(home.join(".swarm/accounts.toml"))
            .unwrap()
            .contains("name = \"work\"")
    );
    assert_eq!(
        std::fs::read(native.join("auth.json")).unwrap(),
        b"existing-credential-sentinel"
    );
    assert_eq!(
        json(run(&home, &["accounts", "--provider", "agy", "--json"]))["modified"],
        true
    );
}

#[test]
fn reset_changes_only_metadata_and_keeps_native_and_token_sentinels() {
    let home = fixture("personal-reset");
    json(run(&home, &login("codex", "work", &revision(&home))));
    json(run(&home, &login("claude", "personal", &revision(&home))));
    let paths = [
        home.join(".codex-work/auth.json"),
        home.join(".claude/.profiles/personal/token-store"),
        home.join(".swarm/codex-usage.json"),
        home.join("keychain-sentinel"),
        home.join(".claude-personal/token-store"),
    ];
    std::fs::create_dir_all(home.join(".claude-personal")).unwrap();
    for path in &paths {
        std::fs::write(path, b"credential-and-cache-sentinel").unwrap();
    }
    let modified = revision(&home);
    let mut reset = json(run(
        &home,
        &["accounts", "reset", "--revision", &modified, "--json"],
    ));
    assert_ne!(reset["revision"], modified);
    reset["revision"] = "<new revision>".into();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/accounts/personal-reset.json")).unwrap();
    assert_eq!(sorted(reset), sorted(expected));
    let list = json(run(&home, &["accounts", "--provider", "agy", "--json"]));
    assert_eq!(list["modified"], false);
    for path in paths {
        assert_eq!(
            std::fs::read(path).unwrap(),
            b"credential-and-cache-sentinel"
        );
    }
    assert!(home.join(".codex-work").is_dir());
    assert!(home.join(".claude/.profiles/personal").is_dir());
    assert!(
        !run(
            &home,
            &["accounts", "reset", "--revision", &modified, "--json"]
        )
        .status
        .success()
    );
}

#[test]
fn metadata_native_references_are_discovered_and_invalid_content_is_refused() {
    let home = fixture("work-reference");
    let external = home.join("external-native-home");
    std::fs::create_dir(&external).unwrap();
    let path = home.join(".swarm/accounts.toml");
    std::fs::write(
        &path,
        format!(
            "version = 1\n[[accounts]]\nprovider = \"codex\"\nname = \"work\"\nhome = \"{}\"\n",
            external.display()
        ),
    )
    .unwrap();
    let list = json(run(&home, &["accounts", "--provider", "codex", "--json"]));
    assert!(
        list["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "work" && row["home"] == external.to_string_lossy().as_ref())
    );
    for content in [
        "version = 2\n",
        "version = 1\nsecret = \"private-token\"\n",
        "version = 1\n[[accounts]]\nprovider = \"agy\"\nname = \"work\"\nhome = \"/tmp/work\"\n",
        "version = 1\n[[accounts]]\nprovider = \"codex\"\nname = \"work\"\nhome = \"relative\"\n",
    ] {
        std::fs::write(&path, content).unwrap();
        let output = run(&home, &["accounts", "--provider", "agy", "--json"]);
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-token"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
}

#[test]
fn linked_metadata_keeps_its_target_and_refuses_a_broken_link() {
    let home = fixture("personal-links");
    let path = home.join(".swarm/accounts.toml");
    let target = home.join("dotfiles-accounts.toml");
    std::fs::write(&target, "version = 1\n").unwrap();
    symlink(&target, &path).unwrap();
    json(run(&home, &login("codex", "work", &revision(&home))));
    assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
    assert!(
        std::fs::read_to_string(&target)
            .unwrap()
            .contains("name = \"work\"")
    );
    json(run(
        &home,
        &[
            "accounts",
            "reset",
            "--revision",
            &revision(&home),
            "--json",
        ],
    ));
    assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
    std::fs::remove_file(&target).unwrap();
    assert!(
        !run(&home, &["accounts", "--provider", "agy", "--json"])
            .status
            .success()
    );
    assert!(
        !run(
            &home,
            &["accounts", "reset", "--revision", "stale", "--json"]
        )
        .status
        .success()
    );
    assert!(!target.exists());
}

#[test]
fn usage_cache_also_stays_outside_registered_external_native_homes() {
    let home = fixture("spare-cache-boundary");
    for provider in ["codex", "claude"] {
        std::fs::write(
            home.join(".swarm/accounts.toml"),
            format!(
                "version = 1\n[[accounts]]\nprovider = '{provider}'\nname = 'work'\nhome = '{}'\n",
                home.display()
            ),
        )
        .unwrap();
        let output = run(
            &home,
            &["usage", "--refresh", "--provider", "codex", "--json"],
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("outside provider homes"));
        assert!(!home.join(".swarm/codex-usage.json").exists());
    }
}

fn sorted(value: serde_json::Value) -> String {
    fn sort(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(fields) => serde_json::Value::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| (key, sort(value)))
                    .collect::<std::collections::BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sort).collect())
            }
            value => value,
        }
    }
    serde_json::to_string(&sort(value)).unwrap()
}

#[test]
fn hard_links_and_read_only_metadata_follow_existing_write_rules() {
    use std::os::unix::fs::MetadataExt;
    let home = fixture("spare-file-rules");
    let path = home.join(".swarm/accounts.toml");
    let linked = home.join("hard-linked-accounts.toml");
    std::fs::write(&path, "version = 1\n").unwrap();
    std::fs::hard_link(&path, &linked).unwrap();
    let original_inode = std::fs::metadata(&path).unwrap().ino();
    json(run(&home, &login("codex", "work", &revision(&home))));
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), original_inode);
    assert_eq!(std::fs::metadata(&linked).unwrap().ino(), original_inode);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&linked).unwrap()
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
    let before = std::fs::read(&path).unwrap();
    let output = run(
        &home,
        &[
            "accounts",
            "reset",
            "--revision",
            &revision(&home),
            "--json",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn concurrent_logins_with_one_revision_cannot_overwrite_each_other() {
    let home = fixture("work-lock");
    let initial = revision(&home);
    let work = command(&home, &login("codex", "work", &initial))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let personal = command(&home, &login("codex", "personal", &initial))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let outcomes = [
        work.wait_with_output().unwrap(),
        personal.wait_with_output().unwrap(),
    ];
    assert_eq!(
        outcomes
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1
    );
    let failed = outcomes
        .iter()
        .find(|output| !output.status.success())
        .unwrap();
    assert!(String::from_utf8_lossy(&failed.stderr).contains("changed"));
    let text = std::fs::read_to_string(home.join(".swarm/accounts.toml")).unwrap();
    assert_eq!(text.matches("[[accounts]]").count(), 1);
    let leftovers: Vec<_> = std::fs::read_dir(home.join(".swarm"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".swarm-"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_failed_login_start_keeps_metadata_and_worker_cannot_open_login() {
    let home = fixture("personal-ring-failure");
    std::fs::write(home.join(".swarm/adapters/fake.conf"), "self = true\nspawn = printf pane-work\nring = echo private-login-error >&2; exit 8\nlist = true\nclose = printf '%s' \"$SWARM_PANE\" > \"$HOME/closed-pane\"\ncapture = true\n").unwrap();
    let initial = revision(&home);
    let blocked = command(&home, &login("codex", "work", &initial))
        .env("SWARM_AGENT_ID", "worker")
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    assert!(!home.join(".swarm/accounts.toml").exists());
    let output = run(&home, &login("codex", "work", &initial));
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-login-error"));
    assert!(
        std::fs::read_to_string(home.join(".swarm/accounts.toml"))
            .unwrap()
            .contains("name = \"work\"")
    );
    assert!(!home.join(".codex-work").exists());
    assert!(!home.join(".swarm/swarm.db").exists());
    assert_eq!(
        std::fs::read_to_string(home.join("closed-pane")).unwrap(),
        "pane-work"
    );
}

#[test]
fn login_retry_after_pane_failure_reuses_equal_registration() {
    let home = fixture("work-retry");
    let adapter_path = home.join(".swarm/adapters/fake.conf");
    let adapter = std::fs::read(&adapter_path).unwrap();
    std::fs::write(
        &adapter_path,
        "self = true\nspawn = exit 8\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    assert!(
        !run(&home, &login("codex", "work", &revision(&home)))
            .status
            .success()
    );
    let before = std::fs::read(home.join(".swarm/accounts.toml")).unwrap();
    let retry_revision = revision(&home);
    std::fs::write(&adapter_path, adapter).unwrap();
    let opened = json(run(&home, &login("codex", "work", &retry_revision)));
    assert_eq!(opened["state"], "opened");
    assert_eq!(opened["revision"], retry_revision);
    assert_eq!(
        std::fs::read(home.join(".swarm/accounts.toml")).unwrap(),
        before
    );
    let again = json(run(&home, &login("codex", "work", &retry_revision)));
    assert_eq!(again["revision"], retry_revision);
    let different = home.join("different-native-home");
    let text = format!(
        "version = 1\n[[accounts]]\nprovider = 'codex'\nname = 'work'\nhome = '{}'\n",
        different.display()
    );
    std::fs::write(home.join(".swarm/accounts.toml"), &text).unwrap();
    let conflict = run(&home, &login("codex", "work", &revision(&home)));
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("already registered"));
    assert_eq!(
        std::fs::read_to_string(home.join(".swarm/accounts.toml")).unwrap(),
        text
    );
}

#[test]
fn login_typed_line_closes_only_after_native_login_succeeds() {
    let home = fixture("personal-pane-exit");
    for provider in ["codex", "claude"] {
        json(run(&home, &login(provider, "personal", &revision(&home))));
        let text = std::fs::read_to_string(home.join("login-line")).unwrap();
        assert!(text.ends_with(" && exit"), "{text}");
        assert_eq!(text.matches(" && exit").count(), 1);
        let success = Command::new("/bin/sh")
            .env("HOME", &home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .args(["-c", &format!("{text}; printf shell-stays-open")])
            .output()
            .unwrap();
        assert!(success.status.success());
        assert!(success.stdout.is_empty());
        tool(&home, provider, "echo login-failed >&2; exit 1");
        let failure = Command::new("/bin/sh")
            .env("HOME", &home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .args(["-c", &format!("{text}; printf shell-stays-open")])
            .output()
            .unwrap();
        assert_eq!(failure.stdout, b"shell-stays-open");
        assert!(String::from_utf8_lossy(&failure.stderr).contains("login-failed"));
    }
}

#[test]
fn account_name_length_accepts_64_and_refuses_65_before_registration() {
    let home = fixture("spare-name-bound");
    let maximum = "w".repeat(64);
    let too_long = "w".repeat(65);
    assert!(
        !run(&home, &login("codex", &too_long, &revision(&home)))
            .status
            .success()
    );
    assert!(!home.join(".swarm/accounts.toml").exists());
    assert!(!home.join("pane-cwd").exists());
    let opened = json(run(&home, &login("codex", &maximum, &revision(&home))));
    assert_eq!(opened["account"], maximum);
}

#[test]
fn each_login_attempt_uses_a_unique_valid_agent_id() {
    let home = fixture("work-pane-names");
    std::fs::write(home.join(".swarm/adapters/fake.conf"), r#"self = true
spawn = mkdir -p "$HOME/panes" && mkdir "$HOME/panes/$SWARM_AGENT_ID" && printf '%s\n' "$SWARM_AGENT_ID" >> "$HOME/pane-names" && printf '%s' "$SWARM_AGENT_ID"
ring = true
list = true
close = true
capture = true
"#).unwrap();
    let maximum = "work".repeat(16);
    for name in ["work", "work", "work.dev", "work_dev", &maximum] {
        let opened = json(run(&home, &login("codex", name, &revision(&home))));
        assert!(swarm::bus::valid_agent_id(opened["pane"].as_str().unwrap()));
    }
    let names = std::fs::read_to_string(home.join("pane-names")).unwrap();
    assert_eq!(names.lines().count(), 5);
    assert_eq!(
        names
            .lines()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        5
    );
}

#[test]
fn a_timed_out_ring_still_closes_its_spawned_pane_before_the_process_limit() {
    let home = fixture("spare-ring-timeout");
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        r#"self = true
spawn = printf pane-spare
ring = exec /bin/sleep 25
list = true
close = printf '%s' "$SWARM_PANE" > "$HOME/closed-pane"
capture = true
"#,
    )
    .unwrap();
    let started = std::time::Instant::now();
    let output = run(&home, &login("codex", "spare", &revision(&home)));
    assert!(!output.status.success());
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot start native login"));
    assert_eq!(
        std::fs::read_to_string(home.join("closed-pane")).unwrap(),
        "pane-spare"
    );
}
