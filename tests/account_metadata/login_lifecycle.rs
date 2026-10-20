// Login retry, account home, and pane lifecycle tests.
use super::*;

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

/// An entry that an older build wrote has its home in `$HOME`. A new login for that name must
/// open the login for that home, as the older build did.
#[test]
fn login_for_a_name_with_an_older_home_reuses_that_home() {
    let home = fixture("work-older-home");
    let older = home.join(".codex-work");
    std::fs::create_dir(&older).unwrap();
    let text = format!(
        "version = 1\n[[accounts]]\nprovider = 'codex'\nname = 'work'\nhome = '{}'\n",
        older.display()
    );
    std::fs::write(home.join(".swarm/accounts.toml"), &text).unwrap();
    let before = revision(&home);
    let opened = json(run(&home, &login("codex", "work", &before)));
    assert_eq!(opened["revision"], before);
    assert_eq!(
        std::fs::read_to_string(home.join("login-exec")).unwrap(),
        format!("login\n{}\n", older.display())
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".swarm/accounts.toml")).unwrap(),
        text
    );
    assert!(!home.join(".swarm/accounts/codex/work").exists());
}

/// A profile that swarm finds by reference keeps its home when the owner logs in to its name.
/// A new empty home would hide the profile that is signed in.
#[test]
fn login_for_a_found_profile_reuses_its_home_and_does_not_hide_it() {
    let home = fixture("work-found-profile");
    let found = home.join(".claude/.profiles/work");
    std::fs::create_dir_all(&found).unwrap();
    json(run(&home, &login("claude", "work", &revision(&home))));
    let exec = std::fs::read_to_string(home.join("login-exec")).unwrap();
    assert_eq!(exec.lines().nth(1), Some(found.to_str().unwrap()), "{exec}");
    assert!(!home.join(".swarm/accounts/claude/work").exists());
    let list = json(run(&home, &["accounts", "--provider", "claude", "--json"]));
    let work: Vec<_> = list["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|account| account["name"] == "work")
        .collect();
    assert_eq!(work.len(), 1, "{list}");
    assert_eq!(work[0]["home"], found.to_str().unwrap());
}

/// A second login for a registered name makes its folder again, with the private mode.
#[test]
fn a_second_login_repairs_a_removed_or_open_account_home() {
    let home = fixture("work-repair");
    json(run(&home, &login("codex", "work", &revision(&home))));
    let account_home = home.join(".swarm/accounts/codex/work");
    let mode = || {
        std::fs::metadata(&account_home)
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(), 0o700);

    std::fs::remove_dir_all(&account_home).unwrap();
    json(run(&home, &login("codex", "work", &revision(&home))));
    assert_eq!(mode(), 0o700, "a removed home");

    std::fs::set_permissions(&account_home, std::fs::Permissions::from_mode(0o755)).unwrap();
    json(run(&home, &login("codex", "work", &revision(&home))));
    assert_eq!(mode(), 0o700, "a home with an open mode");
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
    const SWIFT_PROCESS_LIMIT_SECONDS: u64 = 20;
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
    let initial = revision(&home);
    let holder = std::fs::File::create(home.join(".swarm/accounts.lock")).unwrap();
    holder.lock().unwrap();
    let lock_wait = std::time::Duration::from_secs(3);
    let release = std::thread::spawn(move || {
        std::thread::sleep(lock_wait);
        holder.unlock().unwrap();
    });
    let started = std::time::Instant::now();
    let output = run(&home, &login("codex", "spare", &initial));
    release.join().unwrap();
    assert!(!output.status.success());
    assert!(started.elapsed() >= lock_wait);
    assert!(started.elapsed() < std::time::Duration::from_secs(SWIFT_PROCESS_LIMIT_SECONDS));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot start native login"));
    assert_eq!(
        std::fs::read_to_string(home.join("closed-pane")).unwrap(),
        "pane-spare"
    );
}

#[test]
fn a_failed_ring_and_close_names_the_pane_that_stayed_open() {
    let home = fixture("personal-close-failure");
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = true\nspawn = printf pane-personal\nring = echo private-ring-error >&2; exit 8\nlist = true\nclose = echo private-close-error >&2; exit 9\ncapture = true\n",
    )
    .unwrap();
    let output = run(&home, &login("codex", "personal", &revision(&home)));
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("pane-personal"), "{error}");
    assert!(error.contains("stayed open"), "{error}");
    assert!(!error.contains("private-ring-error"), "{error}");
    assert!(!error.contains("private-close-error"), "{error}");
}

#[test]
fn allocation_changes_only_swarm_home_and_refuses_linked_account_dirs() {
    let home = fixture("private-allocation");
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nclose = true\nlist = true\ncapture = true\n",
    )
    .unwrap();
    std::fs::write(home.join("sentinel"), "keep").unwrap();
    let outside_before: std::collections::BTreeMap<_, _> = std::fs::read_dir(&home)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name != ".swarm")
        .map(|name| {
            (
                name.clone(),
                if name == "bin" {
                    Vec::new()
                } else {
                    std::fs::read(home.join(&name)).unwrap()
                },
            )
        })
        .collect();
    let tools_before =
        ["claude", "codex"].map(|name| std::fs::read(home.join("bin").join(name)).unwrap());
    for provider in ["claude", "codex"] {
        json(run(&home, &login(provider, "work", &revision(&home))));
        let path = home.join(".swarm/accounts").join(provider).join("work");
        assert!(path.is_dir());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        if provider == "claude" {
            let listed = json(
                command(&home, &["accounts", "--provider", "claude", "--json"])
                    .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", &path)
                    .env("CLAUDE_CONFIG_DIR", "/wrong")
                    .output()
                    .unwrap(),
            );
            assert_eq!(listed["auto"], "work");
            assert_eq!(listed["accounts"].as_array().unwrap().len(), 2);
            let account = &listed["accounts"][1];
            assert_eq!(
                account["env"]["CLAUDE_SECURESTORAGE_CONFIG_DIR"],
                path.to_string_lossy().as_ref()
            );
            assert!(account["env"].get("CLAUDE_CONFIG_DIR").is_none());
        }
    }
    let outside_after: std::collections::BTreeMap<_, _> = std::fs::read_dir(&home)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name != ".swarm")
        .map(|name| {
            (
                name.clone(),
                if name == "bin" {
                    Vec::new()
                } else {
                    std::fs::read(home.join(&name)).unwrap()
                },
            )
        })
        .collect();
    assert_eq!(outside_before, outside_after);
    assert_eq!(
        tools_before,
        ["claude", "codex"].map(|name| std::fs::read(home.join("bin").join(name)).unwrap())
    );
    symlink(
        home.join("bin"),
        home.join(".swarm/accounts/claude/personal"),
    )
    .unwrap();
    let before = std::fs::read(home.join(".swarm/accounts.toml")).unwrap();
    let refused = run(&home, &login("claude", "personal", &revision(&home)));
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("not a symlink"));
    assert_eq!(
        std::fs::read(home.join(".swarm/accounts.toml")).unwrap(),
        before
    );
}
