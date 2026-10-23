use super::*;

/// Revert active, recorded folder trust only when the folder is absent and the value still
/// matches. Folder checks run outside the shared write lock, so a slow mount cannot hold it.
pub fn prune_stale_trust(
    store: &rusqlite::Connection,
    lock: &std::path::Path,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for entry in state::recorded(store, true)? {
        let Some(dir) = trust_folder(&entry.edit) else {
            continue;
        };
        if !dir.is_absolute()
            || !matches!(std::fs::symlink_metadata(dir), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
        {
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
