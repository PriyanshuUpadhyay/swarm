//! Every write swarm makes outside its home (ADR 0042): the plan types each writer fills, the
//! digest that apply checks, and the locked, link-following, compare-before-rename file writes.

use std::os::unix::fs::OpenOptionsExt;

/// An entry that swarm needs at a place where the file already holds another one (ADR 0036).
/// Swarm never writes over it: the owner removes it, or does without swarm's hooks.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Conflict {
    pub file: String,
    pub entry: String,
    pub found: String,
    pub wanted: String,
    pub fix: String,
}

/// What one writer would do to one file: its text now, its planned text, each conflict, and each
/// item that the planned text adds. An entry equal to swarm's stays as it is.
#[derive(Debug)]
pub struct FilePlan {
    pub path: std::path::PathBuf,
    pub before: String,
    pub after: String,
    pub conflicts: Vec<Conflict>,
    pub edits: Vec<Edit>,
}

impl FilePlan {
    /// A file that swarm cannot read or edit, as a conflict, so setup writes no file and the
    /// owner sees the fix (owner's choice, 2026-10-01).
    pub fn unreadable(path: std::path::PathBuf, error: String) -> Self {
        let file = path.display().to_string();
        Self {
            conflicts: vec![Conflict {
                file: file.clone(),
                entry: "the whole file".into(),
                found: error,
                wanted: "a file that swarm can read and edit".into(),
                fix: format!("repair {file}, or move it away"),
            }],
            path,
            before: String::new(),
            after: String::new(),
            edits: Vec::new(),
        }
    }
}

/// The three places swarm writes. Wire names are open: a later build may add one (A4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A key in a TOML table; `path` is the tables, then the key.
    TomlKey,
    /// A key in a JSON object; `path` is the keys, the last one the item's own.
    JsonKey,
    /// One item of a JSON array; `path` is the keys to the array.
    JsonArrayItem,
}

/// The swarm feature that wrote an item. Wire names are open: a later build may add one (A4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Writer {
    #[serde(rename = "hooks.state")]
    HooksState,
    #[serde(rename = "hooks.guard")]
    HooksGuard,
    #[serde(rename = "launch.trust")]
    LaunchTrust,
    #[serde(rename = "herdr")]
    Herdr,
}

/// One item that a writer adds to a file outside the swarm home.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub writer: Writer,
    /// The write target, links followed, so two names of one file are one place.
    pub file: std::path::PathBuf,
    pub kind: Kind,
    pub path: Vec<String>,
    /// The value written, as JSON; a TOML value keeps its type (string, integer, float, bool).
    pub wrote: serde_json::Value,
    /// The value the write replaced; None when the place was absent.
    pub before: Option<serde_json::Value>,
    /// How many containers at the end of `path` (tables, objects, the array) the file lacked
    /// before the write, so a revert removes them again once they are empty.
    pub created: u8,
    /// The id of an edit that is always reverted with this one.
    pub with: Option<String>,
}

impl Edit {
    /// A new item at `path` of `file`, with nothing before it.
    pub fn new(
        writer: Writer,
        file: &std::path::Path,
        kind: Kind,
        path: &[&str],
        wrote: serde_json::Value,
    ) -> Self {
        Self {
            writer,
            file: place(file),
            kind,
            path: path.iter().map(|part| part.to_string()).collect(),
            wrote,
            before: None,
            created: 0,
            with: None,
        }
    }

    /// The first 12 hex digits of a hash of the place, so a found item and a recorded one of one
    /// place share an id. An array item adds its value, because two items share one array.
    pub fn id(&self) -> String {
        use sha2::Digest;
        let mut digest = sha2::Sha256::new();
        digest.update(self.file.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(wire(&self.kind).as_bytes());
        for part in &self.path {
            digest.update([0]);
            digest.update(part.as_bytes());
        }
        if self.kind == Kind::JsonArrayItem {
            digest.update([0]);
            digest.update(self.wrote.to_string().as_bytes());
        }
        digest.finalize()[..6]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// The file a write to `file` lands in, with its folder's links resolved too, so two names of one
/// file, or of one missing file, are one place.
fn place(file: &std::path::Path) -> std::path::PathBuf {
    let target = write_target(file).unwrap_or_else(|_| file.to_path_buf());
    match (target.parent(), target.file_name()) {
        (Some(dir), Some(name)) => {
            std::fs::canonicalize(dir).map_or(target.clone(), |dir| dir.join(name))
        }
        _ => target,
    }
}

/// The wire name of a `Kind` or `Writer`.
fn wire(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => unreachable!("Kind and Writer serialize as strings"),
    }
}

/// Write each planned file and record its edits, one transaction per file, so a row exists only
/// for a file that was written. Refuses any conflict in any plan before any write. Returns each
/// file that changed. The caller holds `trust.lock` from the plan to here and checks the digest.
pub fn apply(
    store: &rusqlite::Connection,
    plans: &[FilePlan],
) -> Result<Vec<std::path::PathBuf>, String> {
    commit(store, plans, false)
}

/// `apply` for the plans of `revert_plan`: each edit's row is set off, not added.
pub fn revert(
    store: &rusqlite::Connection,
    plans: &[FilePlan],
) -> Result<Vec<std::path::PathBuf>, String> {
    commit(store, plans, true)
}

fn commit(
    store: &rusqlite::Connection,
    plans: &[FilePlan],
    off: bool,
) -> Result<Vec<std::path::PathBuf>, String> {
    if let Some(conflicts) = conflicts_text(plans) {
        return Err(conflicts);
    }
    let failed = |error: rusqlite::Error| format!("swarm: cannot record a managed edit: {error}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    let mut changed = Vec::new();
    for plan in plans {
        let writes = plan.after != plan.before;
        if !writes && plan.edits.is_empty() {
            continue;
        }
        let tx =
            rusqlite::Transaction::new_unchecked(store, rusqlite::TransactionBehavior::Immediate)
                .map_err(failed)?;
        for edit in &plan.edits {
            if off {
                tx.execute("UPDATE managed_edit SET off = 1 WHERE id = ?1", [edit.id()])
            } else {
                tx.execute(
                    "INSERT INTO managed_edit
                         (id, writer, file, kind, path, wrote, before, created, with_id, at_s, off)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
                     ON CONFLICT (id) DO UPDATE SET writer = excluded.writer,
                         file = excluded.file, kind = excluded.kind, path = excluded.path,
                         wrote = excluded.wrote, before = excluded.before,
                         created = excluded.created, with_id = excluded.with_id,
                         at_s = excluded.at_s, off = 0",
                    rusqlite::params![
                        edit.id(),
                        wire(&edit.writer),
                        edit.file.to_string_lossy(),
                        wire(&edit.kind),
                        serde_json::Value::from(edit.path.clone()).to_string(),
                        edit.wrote.to_string(),
                        edit.before.as_ref().map(serde_json::Value::to_string),
                        edit.created,
                        edit.with,
                        now,
                    ],
                )
            }
            .map_err(failed)?;
        }
        if writes {
            write_text(&plan.path, &plan.before, &plan.after)?;
        }
        tx.commit().map_err(|error| match writes {
            true => format!(
                "swarm: wrote {} but could not record it: {error}; swarm managed list shows it as found",
                plan.path.display()
            ),
            false => failed(error),
        })?;
        if writes {
            changed.push(plan.path.clone());
        }
    }
    Ok(changed)
}

/// A digest of each planned file's path, text, and planned text, so apply refuses a file that
/// changed after the owner saw the plan, and a swarm whose entries differ from the plan's.
pub fn digest(plans: &[FilePlan]) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    for plan in plans {
        digest.update(plan.path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(plan.before.as_bytes());
        digest.update([0]);
        digest.update(plan.after.as_bytes());
        digest.update([0]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Run `change` while holding `lock`, so two launches cannot both read a settings file and the
/// second write drop the first one's trust entry.
pub fn with_lock<T>(
    lock: &std::path::Path,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(lock)
        .map_err(|error| format!("swarm: cannot open {}: {error}", lock.display()))?;
    file.lock()
        .map_err(|error| format!("swarm: cannot lock {}: {error}", lock.display()))?;
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

/// Replace `path` in one rename, with the old file's permissions, because `~/.claude.json` holds
/// credentials and a running CLI may read it at any moment.
pub(crate) fn write_json(
    path: &std::path::Path,
    before: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    write_text(path, before, &json_text(value))
}

pub(crate) fn json_text(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON serialization cannot fail") + "\n"
}

/// The end of the error of a write that found its file changed since the read.
pub(crate) const CHANGED: &str = "changed while swarm edited it; run the command again";

pub(crate) fn changed_error(path: &std::path::Path) -> String {
    format!("swarm: {} {CHANGED}", path.display())
}

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
pub(crate) fn write_target(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
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

/// The unified diff of one planned file.
pub fn diff(plan: &FilePlan) -> String {
    crate::diff::unified(&plan.path.to_string_lossy(), &plan.before, &plan.after)
}

/// Each conflict with its fix, and a last line that says no file was written; None without one.
pub fn conflicts_text(plans: &[FilePlan]) -> Option<String> {
    let conflicts: Vec<_> = plans.iter().flat_map(|plan| &plan.conflicts).collect();
    if conflicts.is_empty() {
        return None;
    }
    let mut text: String = conflicts
        .iter()
        .map(|conflict| {
            format!(
                "conflict: {} {}\n  found:  {}\n  wanted: {}\n  fix:    {}\n\n",
                conflict.file, conflict.entry, conflict.found, conflict.wanted, conflict.fix
            )
        })
        .collect();
    let plural = if conflicts.len() == 1 { "" } else { "s" };
    text += &format!("{} conflict{plural}. No file written.", conflicts.len());
    Some(text)
}

/// Each diff, then the conflicts, or else `unchanged` when no file changes, or else the line that
/// names `apply`, the command that writes the plan.
pub fn plan_text(plans: &[FilePlan], apply: &str, unchanged: &str) -> String {
    let diffs: String = plans.iter().map(diff).collect();
    let last = match conflicts_text(plans) {
        Some(conflicts) => format!("\n{conflicts}"),
        None if diffs.is_empty() => unchanged.into(),
        None => format!("\nPlan only. No file written. Run `{apply}` to apply."),
    };
    format!("{diffs}{last}\n")
}

/// The plan for the app: the digest that apply checks, each file that changes with its diff, and
/// each conflict.
pub fn plan_json(plans: &[FilePlan]) -> serde_json::Value {
    serde_json::json!({
        "digest": digest(plans),
        "files": plans
            .iter()
            .filter(|plan| plan.after != plan.before)
            .map(|plan| serde_json::json!({"path": plan.path.to_string_lossy(), "diff": diff(plan)}))
            .collect::<Vec<_>>(),
        "conflicts": plans.iter().flat_map(|plan| &plan.conflicts).collect::<Vec<_>>(),
    })
}
