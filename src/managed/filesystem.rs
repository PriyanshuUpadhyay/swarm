//! Locked, link-following text reads and writes.

use super::changed_error;

use std::os::unix::fs::OpenOptionsExt;

/// How long a launch or setup waits for another swarm's trust lock. Every launch takes the one
/// lock, and the holder may run git on a hung mount, so a wait with no end could stall every
/// launch on the Mac (SRV-18). 10 s matches the guard hook's registration timeout.
pub const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Run `change` while holding `lock`, so two launches cannot both read a settings file and the
/// second write drop the first one's trust entry.
pub fn with_lock<T>(
    lock: &std::path::Path,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    with_lock_within(lock, LOCK_WAIT, change)
}

/// Background maintenance yields to an active writer instead of waiting or reporting a failure.
pub(super) fn try_with_lock<T>(
    lock: &std::path::Path,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<Option<T>, String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(lock)
        .map_err(|error| format!("swarm: cannot open {}: {error}", lock.display()))?;
    match file.try_lock() {
        Ok(()) => change().map(Some),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => {
            Err(format!("swarm: cannot lock {}: {error}", lock.display()))
        }
    }
}

/// `with_lock` that gives up after `wait` with an error that names the lock.
pub(super) fn with_lock_within<T>(
    lock: &std::path::Path,
    wait: std::time::Duration,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(lock)
        .map_err(|error| format!("swarm: cannot open {}: {error}", lock.display()))?;
    let deadline = std::time::Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(format!(
                    "swarm: another swarm holds {} and did not let go in {wait:?}; try again when it ends",
                    lock.display()
                ));
            }
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(format!("swarm: cannot lock {}: {error}", lock.display()));
            }
        }
    }
    change()
}

/// A file's text, or "" for a missing file.
pub(crate) fn read_text(path: &std::path::Path) -> Result<String, String> {
    Ok(read_optional(path)?.unwrap_or_default())
}

/// A file's text, or None for a missing file. A file that cannot be read, such as one that is not
/// UTF-8, is an error, so it is never written over whole.
pub(crate) fn read_optional(path: &std::path::Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("swarm: cannot read {}: {error}", path.display())),
    }
}

/// A JSON file's text ("" for a missing file) and its object.
pub(crate) fn read_json_object(
    path: &std::path::Path,
) -> Result<(String, serde_json::Value), String> {
    let text = read_optional(path)?;
    let value = json_object(path, text.as_deref())?;
    Ok((text.unwrap_or_default(), value))
}

/// `text` as a JSON object; a missing file is an empty object. An empty file is refused, because
/// another program may be writing it now.
pub(crate) fn json_object(
    path: &std::path::Path,
    text: Option<&str>,
) -> Result<serde_json::Value, String> {
    let Some(text) = text else {
        return Ok(serde_json::json!({}));
    };
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("swarm: cannot parse {}: {error}", path.display()))?;
    value
        .is_object()
        .then_some(value)
        .ok_or_else(|| format!("swarm: {} is not a JSON object", path.display()))
}

pub(crate) fn json_text(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON serialization cannot fail") + "\n"
}

/// The end of the error of a write that found its file changed since the read.
pub const CHANGED: &str = "changed while swarm edited it; run the command again";

/// A launch edits a file that a running CLI may rewrite at any moment, such as `~/.claude.json`,
/// so an edit that found its file changed reads it again and tries again, up to 3 more times.
pub(crate) fn retried<T>(mut edit: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    for _ in 0..3 {
        match edit() {
            Err(error) if error.ends_with(CHANGED) => {}
            result => return result,
        }
    }
    edit()
}

/// Where a write to `path` lands: the file at the end of its links, or `path` itself. A link, or a
/// chain of links, to a missing file gets that file made, as a write through the link would, but
/// never a missing folder. A chain longer than the system's 32 hops is refused.
pub(super) fn write_target(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let fail = |error: std::io::Error| format!("swarm: cannot write {}: {error}", path.display());
    if !std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()) {
        return Ok(path.to_path_buf());
    }
    match std::fs::canonicalize(path) {
        Ok(target) => Ok(target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut target = path.to_path_buf();
            for _ in 0..32 {
                if !std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_symlink()) {
                    break;
                }
                let link = std::fs::read_link(&target).map_err(fail)?;
                target = target.parent().unwrap_or(&target).join(link);
            }
            if std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_symlink()) {
                return Err(format!("swarm: {} links too many times", path.display()));
            }
            match target.parent() {
                Some(dir) if dir.is_dir() => Ok(target),
                _ => Err(format!(
                    "swarm: {} links to {}, whose folder is missing",
                    path.display(),
                    target.display()
                )),
            }
        }
        Err(error) => Err(fail(error)),
    }
}

/// A file that swarm cannot edit is the owner's lock, and a rename would replace it anyway, so
/// swarm refuses it. An open for write, with no truncate, changes nothing and fails for a
/// read-only mode, Finder's Locked box, an ACL, or another user's file. The folder needs its
/// write bit for the rename. Checks the file at the end of `path`'s links.
pub(crate) fn refuse_read_only(path: &std::path::Path) -> Result<(), String> {
    let target = write_target(path)?;
    let refused =
        |error: std::io::Error| format!("swarm: cannot edit {}: {error}", target.display());
    match std::fs::OpenOptions::new().write(true).open(&target) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(refused(error)),
        _ => {}
    }
    // ponytail: mode bits only; a locked or ACL-denied folder fails at the rename, add an
    // access(2) check if that case is seen.
    match target.parent() {
        Some(dir) if std::fs::metadata(dir).is_ok_and(|meta| meta.permissions().readonly()) => {
            Err(format!("swarm: {} is read-only", dir.display()))
        }
        _ => Ok(()),
    }
}

/// Replace `path` in one rename, with the old file's permissions; a hard-linked TOML file is written
/// in place. A linked file is replaced at its target, because a rename onto the link itself would
/// replace the owner's link (ADR 0036).
/// `before` is the text the edit was made from ("" for a missing file); a file that another
/// program changed since then is refused, not written over, and a file that already holds `text`
/// is done, as when an earlier plan of the same setup wrote it through another link.
pub(crate) fn write_text(path: &std::path::Path, before: &str, text: &str) -> Result<(), String> {
    let fail = |error: std::io::Error| format!("swarm: cannot write {}: {error}", path.display());
    let target = write_target(path)?;
    let path = target.as_path();
    refuse_read_only(path)?;
    let now = read_optional(path)?.unwrap_or_default();
    if now == text {
        return Ok(());
    }
    if now != before {
        return Err(changed_error(path));
    }
    // A rename would split a hard-linked Codex config.toml from the owner's other name, so it is
    // written in place, as the base did. A JSON file is renamed, as at the base, because a running
    // CLI may read `~/.claude.json` at any moment.
    let toml = path
        .extension()
        .is_some_and(|extension| extension == "toml");
    if toml
        && std::fs::metadata(path)
            .is_ok_and(|meta| std::os::unix::fs::MetadataExt::nlink(&meta) > 1)
    {
        return std::fs::write(path, text).map_err(fail);
    }
    let dir = path
        .parent()
        .ok_or_else(|| format!("swarm: {} has no parent", path.display()))?;
    // A linked file's folder exists already (`write_target`); this makes a missing ~/.gemini/config.
    std::fs::create_dir_all(dir).map_err(fail)?;
    let tmp = dir.join(format!(
        ".{}.swarm-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let permissions = std::fs::metadata(path)
        .map(|meta| meta.permissions())
        .unwrap_or_else(|_| std::os::unix::fs::PermissionsExt::from_mode(0o600));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
        .and_then(|()| std::fs::set_permissions(&tmp, permissions))
        .map_err(fail)
        .and_then(
            |()| match read_optional(path)?.unwrap_or_default() == before {
                true => std::fs::rename(&tmp, path).map_err(fail),
                false => Err(changed_error(path)),
            },
        )
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

/// Remove the file a `Kind::File` item made, only while its text is still `before`.
pub(crate) fn remove_file(path: &std::path::Path, before: &str) -> Result<(), String> {
    match read_optional(path)? {
        None => Ok(()),
        Some(now) if now == before => std::fs::remove_file(path)
            .map_err(|error| format!("swarm: cannot remove {}: {error}", path.display())),
        Some(_) => Err(changed_error(path)),
    }
}
