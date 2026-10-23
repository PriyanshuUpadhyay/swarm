//! Detached, best-effort maintenance. No caller waits for the config scan or its writes.

use std::{
    path::Path,
    process::{Command, Stdio},
};

const COMMAND: &str = "__prune-stale-trust";
const VERSION_FILE: &str = "trust-prune-version";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Handle the private child command before scheduling, so the child cannot start itself again.
pub(crate) fn trust_maintenance(args: &[String]) -> bool {
    if args == [COMMAND, "schedule-upgrade"] {
        schedule_upgrade_cleanup();
        return true;
    }
    if let [cmd, reason] = args
        && cmd == COMMAND
        && (reason == "upgrade" || reason == "ui-exit")
    {
        if let Err(error) = cleanup(reason == "upgrade") {
            eprintln!("swarm: trust cleanup skipped: {error}");
        }
        return true;
    }
    false
}

pub(crate) fn schedule_upgrade_cleanup() {
    // This read does not claim a new home. Ordinary CLI commands never schedule a writer.
    if let Ok(db) = swarm::paths::sqlite_db_to_read()
        && db.is_file()
        && let Some(root) = db.parent()
        && std::fs::read_to_string(root.join(VERSION_FILE))
            .ok()
            .as_deref()
            != Some(VERSION)
    {
        spawn_trust_cleanup(root, "upgrade");
    }
}

pub(crate) fn spawn_trust_cleanup(root: &Path, reason: &str) {
    use std::os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    };
    // Never write a log or a lock in a foreign or removed home (ADR 0036).
    if !swarm::paths::is_claimed(root) {
        return;
    }
    let start = || -> Result<(), Box<dyn std::error::Error>> {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(root.join("trust-prune.log"))?;
        log.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        let mut child = Command::new(std::env::current_exe()?)
            .args([COMMAND, reason])
            // Pin the parent's home even for a branch build or a changed executable link.
            .env(
                "SWARM_HOME",
                root.parent().ok_or("swarm home has no parent")?,
            )
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .process_group(0)
            .spawn()?;
        // Reap while the server lives. A short-lived caller can exit before this thread does.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    };
    if let Err(error) = start() {
        eprintln!("swarm: could not start trust cleanup: {error}");
    }
}

fn cleanup(after_upgrade: bool) -> Result<(), Box<dyn std::error::Error>> {
    // A short-lived caller's home can disappear before its child starts. Never recreate it.
    let db = swarm::paths::sqlite_db_to_read()?;
    if !db.is_file() {
        return Ok(());
    }
    let root = db.parent().ok_or("swarm database has no parent")?;
    if !swarm::paths::is_claimed(root) {
        return Ok(());
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("trust-prune.lock"))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    let version = root.join(VERSION_FILE);
    if after_upgrade && std::fs::read_to_string(&version).ok().as_deref() == Some(VERSION) {
        return Ok(());
    }
    // A new home has no trust records. Do not create its database from a cleanup child.
    if !db.is_file() {
        return Ok(());
    }
    let store = swarm::store::open(&db)?;
    swarm::managed::prune_stale_trust(&store, &swarm::paths::trust_lock_path()?)?;
    swarm::store::write_atomic(&version, VERSION)?;
    Ok(())
}
