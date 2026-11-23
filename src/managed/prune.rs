use super::*;

/// Revert active, recorded folder trust only when the folder is absent and the value still
/// matches. The first folder check runs outside the shared write lock.
pub fn prune_stale_trust(
    store: &rusqlite::Connection,
    lock: &std::path::Path,
) -> Result<(), String> {
    prune_with(store, lock, missing_folder)
}

fn missing_folder(dir: &std::path::Path) -> bool {
    matches!(std::fs::symlink_metadata(dir), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn prune_with(
    store: &rusqlite::Connection,
    lock: &std::path::Path,
    missing: impl Fn(&std::path::Path) -> bool,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for entry in state::recorded(store, true)? {
        let Some(dir) = trust_folder(&entry.edit) else {
            continue;
        };
        if !dir.is_absolute() || !missing(dir) {
            continue;
        }
        if let State::Unreadable(error) = &entry.state {
            errors.push(error.clone());
            continue;
        }
        if entry.state != State::Present {
            continue;
        }
        let result = filesystem::try_with_lock(lock, || {
            // A launch may have replaced this row while the folder check ran. Its new record
            // owns the value now, even if the entry's id stayed the same.
            let unchanged: bool = store
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM managed_edit
                 WHERE id = ?1 AND writer = 'launch.trust' AND off = 0
                   AND wrote = ?2 AND before IS ?3 AND created = ?4 AND with_id IS NULL)",
                    rusqlite::params![
                        entry.edit.id(),
                        entry.edit.wrote.to_string(),
                        entry.edit.before.as_ref().map(serde_json::Value::to_string),
                        entry.edit.created,
                    ],
                    |row| row.get(0),
                )
                .map_err(|error| format!("swarm: cannot read managed trust: {error}"))?;
            if !unchanged {
                return Ok(());
            }
            let plan = removal::removal(&entry.edit.file, &[&entry])?;
            if !still_missing(dir) {
                return Ok(());
            }
            // removal checks the live value again, and revert checks the complete file before
            // its rename. A changed value is left for the owner, as in managed revert.
            revert(store, &[plan.into()]).map(|_| ())
        });
        if let Err(error) = result {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn still_missing(dir: &std::path::Path) -> bool {
    // A mount may stall after the first check. A late check must not hold the write lock.
    let (sender, result) = std::sync::mpsc::channel();
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let _ = sender.send(missing_folder(&dir));
    });
    result
        .recv_timeout(std::time::Duration::from_millis(100))
        .unwrap_or(false)
}

fn trust_folder(edit: &Edit) -> Option<&std::path::Path> {
    if edit.writer != Writer::LaunchTrust || edit.with.is_some() {
        return None;
    }
    match (edit.kind, edit.path.as_slice()) {
        (Kind::TomlKey, [projects, dir, key])
            if projects == "projects" && key == "trust_level" && edit.wrote == "trusted" =>
        {
            Some(std::path::Path::new(dir))
        }
        (Kind::JsonKey, [projects, dir, key])
            if projects == "projects" && key == "hasTrustDialogAccepted" && edit.wrote == true =>
        {
            Some(std::path::Path::new(dir))
        }
        (Kind::JsonArrayItem, [key]) if key == "trustedWorkspaces" => {
            edit.wrote.as_str().map(std::path::Path::new)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_regression_prune_keeps_trust_for_a_folder_that_came_back() {
        let root = std::env::temp_dir().join(format!("swarm-prune-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let store = crate::store::open(&root.join("store.db")).unwrap();
        let dir = root.join("worktree");
        let plan = crate::bus::codex_trust_plan(&root.join(".codex"), &dir).unwrap();
        apply(&store, &[plan.into()]).unwrap();
        prune_with(&store, &root.join("trust.lock"), |dir| {
            let missing = missing_folder(dir);
            if missing {
                std::fs::create_dir(dir).unwrap();
            }
            missing
        })
        .unwrap();
        let entry = list(&store, &[]).unwrap().pop().unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(entry.state, State::Present);
    }
}
