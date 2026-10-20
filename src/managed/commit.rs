//! Apply and revert transactions, and plan digests.

use super::*;

/// Write each planned file and record its edits, one transaction per file, so a row exists only
/// for a file that was written. Refuses any conflict in any plan before any write. Returns each
/// file that changed. The caller holds `trust.lock` from the plan to here and checks the digest.
pub fn apply(
    store: &rusqlite::Connection,
    plans: &[Plan],
) -> Result<Vec<std::path::PathBuf>, String> {
    commit(store, plans, false)
}

/// `apply` for the plans of `revert_plan`: each edit's row is set off, not added.
pub fn revert(
    store: &rusqlite::Connection,
    plans: &[Plan],
) -> Result<Vec<std::path::PathBuf>, String> {
    commit(store, plans, true)
}

fn commit(
    store: &rusqlite::Connection,
    plans: &[Plan],
    off: bool,
) -> Result<Vec<std::path::PathBuf>, String> {
    if let Some(conflicts) = conflicts_text(plans) {
        return Err(conflicts);
    }
    for plan in plans {
        match plan {
            Plan::Link(plan) => validate_link(store, plan, off)?,
            Plan::File(plan) if plan.edits.iter().any(|edit| edit.kind == Kind::Symlink) => {
                return Err("swarm: a symlink edit requires a link plan".into());
            }
            _ => {}
        }
    }
    let failed = |error: rusqlite::Error| format!("swarm: cannot record a managed edit: {error}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    let mut changed = Vec::new();
    let result = (|| {
        for plan in plans {
            let writes = plan.changes();
            if !writes && plan.edits().is_empty() {
                continue;
            }
            let tx = rusqlite::Transaction::new_unchecked(
                store,
                rusqlite::TransactionBehavior::Immediate,
            )
            .map_err(failed)?;
            if let Plan::Link(link) = plan {
                validate_link(&tx, link, off)?;
            }
            // A revert keeps a row, also for a found item, so `list` shows it off and the app can
            // offer On; a recorded row keeps its time of the last apply.
            let upsert = match off {
                true => "ON CONFLICT (id) DO UPDATE SET off = 1",
                false => {
                    "ON CONFLICT (id) DO UPDATE SET writer = excluded.writer,
                     file = excluded.file, kind = excluded.kind, path = excluded.path,
                     wrote = excluded.wrote, before = excluded.before,
                     created = excluded.created, with_id = excluded.with_id,
                     at_s = excluded.at_s, off = 0"
                }
            };
            for edit in plan.edits() {
                tx.execute(
                    &format!(
                        "INSERT INTO managed_edit
                         (id, writer, file, kind, path, wrote, before, created, with_id, at_s, off)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) {upsert}"
                    ),
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
                        off,
                    ],
                )
                .map_err(failed)?;
            }
            if writes {
                match plan {
                    Plan::File(plan) => write_text(&plan.path, &plan.before, &plan.after)?,
                    Plan::Link(plan) => write_link(plan)?,
                }
                changed.push(plan.path().to_path_buf());
            }
            tx.commit().map_err(|error| match (writes, off) {
                (true, false) => format!(
                    "swarm: wrote {} but could not record it: {error}; check `swarm managed list` and the destination before retrying",
                    plan.path().display()
                ),
                (true, true) => format!(
                    "swarm: set back {} but could not record it: {error}; swarm managed list shows its items as gone or changed",
                    plan.path().display()
                ),
                (false, _) => failed(error),
            })?;
        }
        Ok(())
    })();
    match result {
        // A later file failed: the error names each file already written, as the owner saw
        // each one printed before (L-12).
        Err(error) if !changed.is_empty() => {
            let files: Vec<_> = changed
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            Err(format!(
                "{error}\nswarm: already wrote {}",
                files.join(", ")
            ))
        }
        result => result.map(|()| changed),
    }
}

/// A digest of each planned file's path, text, and planned text, so apply refuses a file that
/// changed after the owner saw the plan, and a swarm whose entries differ from the plan's.
pub fn digest(plans: &[Plan]) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    for plan in plans {
        match plan {
            Plan::File(plan) => {
                digest.update(plan.path.to_string_lossy().as_bytes());
                digest.update([0]);
                digest.update(plan.before.as_bytes());
                digest.update([0]);
                digest.update(plan.after.as_bytes());
                digest.update([0]);
            }
            Plan::Link(plan) => {
                for part in [
                    "symlink".into(),
                    plan.path.to_string_lossy().into_owned(),
                    plan.edits
                        .first()
                        .map(|edit| edit.file.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    link_text(&plan.before),
                    link_text(&plan.after),
                    serde_json::to_string(&plan.conflicts).unwrap(),
                ] {
                    digest.update(part.as_bytes());
                    digest.update([0]);
                }
            }
        }
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
