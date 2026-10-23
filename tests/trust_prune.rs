use std::path::{Path, PathBuf};
use swarm::{bus, managed};

// Keep subprocess cases apart from direct zero-wait lock checks, so they do not depend on
// concurrent process starts or detached child completion.
static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Fixture {
    home: PathBuf,
    store: rusqlite::Connection,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Fixture {
    fn new() -> Self {
        let guard = FIXTURE_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let home = std::env::temp_dir().join(format!("swarm-trust-prune-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(home.join(".swarm")).unwrap();
        let home = std::fs::canonicalize(home).unwrap();
        std::fs::write(home.join(".swarm/swarm-home"), "swarm\n").unwrap();
        let store = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
        std::fs::write(
            home.join(".swarm/trust-prune-version"),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
        Self {
            home,
            store,
            _guard: guard,
        }
    }

    fn plans(&self, dir: &Path) -> Vec<managed::Plan> {
        vec![
            bus::codex_trust_plan(&self.home.join(".codex"), dir)
                .unwrap()
                .into(),
            bus::claude_trust_plan(&self.home.join(".claude.json"), dir)
                .unwrap()
                .into(),
            bus::agy_trust_plan(&self.home.join(".gemini/settings.json"), dir)
                .unwrap()
                .into(),
        ]
    }

    fn trust(&self, dir: &Path) {
        managed::apply(&self.store, &self.plans(dir)).unwrap();
    }

    fn prune(&self) {
        managed::prune_stale_trust(&self.store, &self.home.join("trust.lock")).unwrap();
    }

    fn states(&self) -> Vec<managed::State> {
        managed::list(&self.store, &[])
            .unwrap()
            .into_iter()
            .map(|entry| entry.state)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[test]
fn gone_folders_are_reverted_for_all_three_clis() {
    let fixture = Fixture::new();
    let dir = fixture.home.join("gone");
    fixture.trust(&dir);
    fixture.prune();
    assert_eq!(fixture.states(), vec![managed::State::Off; 3]);
    for plan in fixture.plans(&dir) {
        assert!(plan.changes(), "the removed trust can be added again");
    }
}

#[test]
fn present_folders_are_kept() {
    let fixture = Fixture::new();
    let dir = fixture.home.join("present");
    std::fs::create_dir(&dir).unwrap();
    fixture.trust(&dir);
    fixture.prune();
    assert_eq!(fixture.states(), vec![managed::State::Present; 3]);
}

#[test]
fn cli_edited_trust_is_kept_while_other_stale_entries_are_removed() {
    let fixture = Fixture::new();
    let dir = fixture.home.join("gone");
    fixture.trust(&dir);
    let config = fixture.home.join(".codex/config.toml");
    let before = std::fs::read_to_string(&config)
        .unwrap()
        .replace("\"trusted\"", "\"untrusted\"");
    std::fs::write(&config, &before).unwrap();
    fixture.prune();
    assert_eq!(std::fs::read_to_string(config).unwrap(), before);
    let states = fixture.states();
    assert_eq!(
        states
            .iter()
            .filter(|state| **state == managed::State::Off)
            .count(),
        2
    );
    assert!(states.contains(&managed::State::Changed("untrusted".into())));
}

#[test]
fn unrecorded_cli_trust_is_kept() {
    let fixture = Fixture::new();
    let dir = fixture.home.join("gone");
    for plan in fixture.plans(&dir) {
        let managed::Plan::File(plan) = plan else {
            unreachable!()
        };
        std::fs::create_dir_all(plan.path.parent().unwrap()).unwrap();
        std::fs::write(plan.path, plan.after).unwrap();
    }
    fixture.prune();
    assert!(fixture.states().is_empty());
    assert!(fixture.plans(&dir).iter().all(|plan| !plan.changes()));
}

#[test]
fn a_dangling_symlink_is_still_a_present_folder_entry() {
    let fixture = Fixture::new();
    let dir = fixture.home.join("dangling");
    std::os::unix::fs::symlink(fixture.home.join("missing"), &dir).unwrap();
    fixture.trust(&dir);
    fixture.prune();
    assert_eq!(fixture.states(), vec![managed::State::Present; 3]);
}

#[test]
fn metadata_errors_other_than_not_found_keep_trust() {
    let fixture = Fixture::new();
    let parent = fixture.home.join("file");
    std::fs::write(&parent, "not a directory").unwrap();
    let dir = parent.join("project");
    assert_eq!(
        std::fs::symlink_metadata(&dir).unwrap_err().kind(),
        std::io::ErrorKind::NotADirectory
    );
    fixture.trust(&dir);
    fixture.prune();
    assert_eq!(fixture.states(), vec![managed::State::Present; 3]);
}

#[test]
fn trust_readded_by_a_cli_after_revert_is_not_claimed_again() {
    let fixture = Fixture::new();
    let dir = fixture.home.join("gone");
    fixture.trust(&dir);
    let plans = managed::revert_plan(&fixture.store, &[], &managed::Target::All).unwrap();
    managed::revert(&fixture.store, &plans).unwrap();
    for plan in fixture.plans(&dir) {
        let managed::Plan::File(plan) = plan else {
            unreachable!()
        };
        std::fs::write(plan.path, plan.after).unwrap();
    }
    fixture.prune();
    assert_eq!(fixture.states(), vec![managed::State::Present; 3]);
}

#[test]
fn claude_revert_restores_the_value_before_swarm_trusted_it() {
    let fixture = Fixture::new();
    let dir = fixture.home.join("gone");
    let config = fixture.home.join(".claude.json");
    let before = serde_json::json!({"projects": {dir.to_str().unwrap(): {
        "hasTrustDialogAccepted": false, "history": ["keep"]
    }}});
    std::fs::write(&config, before.to_string()).unwrap();
    fixture.trust(&dir);
    fixture.prune();
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config).unwrap()).unwrap();
    assert_eq!(after, before);
}

fn cli(fixture: &Fixture, args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &fixture.home)
        .env("SWARM_HOME", &fixture.home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(&fixture.home)
        .args(args)
        .output()
        .unwrap()
}

fn wait_for(mut done: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "background cleanup did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn an_ordinary_cli_command_never_schedules_upgrade_cleanup() {
    let fixture = Fixture::new();
    fixture.trust(&fixture.home.join("gone"));
    let marker = fixture.home.join(".swarm/trust-prune-version");
    std::fs::write(&marker, "older-version").unwrap();
    assert!(cli(&fixture, &["--version"]).status.success());
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "older-version");
    assert!(!fixture.home.join(".swarm/trust-prune.log").exists());
    assert_eq!(fixture.states(), vec![managed::State::Present; 3]);
}

#[test]
fn maintenance_does_not_write_in_an_unclaimed_home() {
    let fixture = Fixture::new();
    let root = fixture.home.join(".swarm");
    std::fs::remove_file(root.join("swarm-home")).unwrap();
    std::fs::remove_file(root.join("trust-prune-version")).unwrap();
    for reason in ["schedule-upgrade", "upgrade", "ui-exit"] {
        assert!(
            cli(&fixture, &["__prune-stale-trust", reason])
                .status
                .success()
        );
    }
    assert!(!root.join("trust-prune.log").exists());
    assert!(!root.join("trust-prune.lock").exists());
    assert!(!root.join("trust-prune-version").exists());
}

#[test]
fn the_explicit_upgrade_hook_prunes_once_in_a_detached_child() {
    let fixture = Fixture::new();
    fixture.trust(&fixture.home.join("gone"));
    let marker = fixture.home.join(".swarm/trust-prune-version");
    std::fs::write(&marker, "older-version").unwrap();
    let output = cli(&fixture, &["__prune-stale-trust", "schedule-upgrade"]);
    assert!(output.status.success(), "{output:?}");
    wait_for(|| {
        std::fs::read_to_string(&marker).ok().as_deref() == Some(env!("CARGO_PKG_VERSION"))
    });
    assert_eq!(fixture.states(), vec![managed::State::Off; 3]);
    fixture.trust(&fixture.home.join("gone"));
    assert!(
        cli(&fixture, &["__prune-stale-trust", "upgrade"])
            .status
            .success()
    );
    assert_eq!(fixture.states(), vec![managed::State::Present; 3]);
}

#[test]
fn a_busy_write_lock_does_not_hold_the_caller_and_cleanup_skips_it_quietly() {
    let fixture = Fixture::new();
    fixture.trust(&fixture.home.join("gone"));
    let marker = fixture.home.join(".swarm/trust-prune-version");
    std::fs::remove_file(&marker).unwrap();
    let lock = std::fs::File::create(fixture.home.join(".swarm/trust.lock")).unwrap();
    lock.lock().unwrap();
    let started = std::time::Instant::now();
    let output = cli(&fixture, &["__prune-stale-trust", "schedule-upgrade"]);
    assert!(output.status.success(), "{output:?}");
    // The shared writer lock normally waits 10 s. Cleanup must finish the caller while the
    // lock is still held; leave room for process startup on a busy test host.
    assert!(started.elapsed() < std::time::Duration::from_secs(8));
    wait_for(|| marker.exists());
    assert_eq!(
        std::fs::read_to_string(fixture.home.join(".swarm/trust-prune.log")).unwrap(),
        ""
    );
    assert_eq!(fixture.states(), vec![managed::State::Present; 3]);
    drop(lock);
    std::fs::remove_file(&marker).unwrap();
    assert!(
        cli(&fixture, &["__prune-stale-trust", "schedule-upgrade"])
            .status
            .success()
    );
    wait_for(|| fixture.home.join(".swarm/trust-prune-version").exists());
    assert_eq!(fixture.states(), vec![managed::State::Off; 3]);
}

#[test]
fn a_cleanup_failure_is_logged_and_does_not_fail_the_caller() {
    let fixture = Fixture::new();
    fixture.trust(&fixture.home.join("gone"));
    let marker = fixture.home.join(".swarm/trust-prune-version");
    std::fs::remove_file(&marker).unwrap();
    std::fs::write(fixture.home.join(".codex/config.toml"), "[broken").unwrap();
    let output = cli(&fixture, &["__prune-stale-trust", "schedule-upgrade"]);
    assert!(output.status.success(), "{output:?}");
    wait_for(|| {
        std::fs::read_to_string(fixture.home.join(".swarm/trust-prune.log")).is_ok_and(|text| {
            text.contains("trust cleanup skipped") && text.contains("not valid TOML")
        })
    });
    assert!(!marker.exists());
}

#[cfg(feature = "serve")]
#[test]
fn ui_start_checks_the_version_and_stop_or_server_signals_always_start_cleanup() {
    use std::io::BufRead;
    for signal in ["upgrade", "stop", "-TERM", "-INT"] {
        let fixture = Fixture::new();
        let root = fixture.home.join(".swarm");
        std::fs::write(
            root.join("trust-prune-version"),
            if signal == "upgrade" {
                "older-version"
            } else {
                env!("CARGO_PKG_VERSION")
            },
        )
        .unwrap();
        fixture.trust(&fixture.home.join("gone"));
        let assets = fixture.home.join("assets");
        std::fs::create_dir(&assets).unwrap();
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &fixture.home)
            .env("SWARM_HOME", &fixture.home)
            .env("PATH", "/usr/bin:/bin")
            .current_dir(&fixture.home)
            .args([
                "ui",
                "--foreground",
                "--port",
                "0",
                "--assets",
                assets.to_str().unwrap(),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::fs::File::create(fixture.home.join("server.log")).unwrap())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(
            line.starts_with("http://127.0.0.1:"),
            "{line} {}",
            std::fs::read_to_string(fixture.home.join("server.log")).unwrap()
        );
        wait_for(|| root.join("ui-url").exists());
        // The signal handlers are installed immediately after the URL record is written.
        wait_for(|| {
            std::net::TcpStream::connect(
                line.split("/#")
                    .next()
                    .unwrap()
                    .trim_start_matches("http://"),
            )
            .is_ok()
        });
        if signal == "upgrade" {
            wait_for(|| fixture.states() == vec![managed::State::Off; 3]);
        }
        if signal == "stop" || signal == "upgrade" {
            let output = cli(&fixture, &["ui", "stop"]);
            assert!(output.status.success(), "{output:?}");
        } else {
            // An HTTP response proves the server installed its handlers.
            let mut stream = std::net::TcpStream::connect(
                line.split("/#")
                    .next()
                    .unwrap()
                    .trim_start_matches("http://"),
            )
            .unwrap();
            use std::io::{Read, Write};
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            stream
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1"));
            assert!(
                std::process::Command::new("/bin/kill")
                    .args([signal, &child.id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        wait_for(|| child.try_wait().unwrap().is_some());
        wait_for(|| fixture.states() == vec![managed::State::Off; 3]);
    }
}
