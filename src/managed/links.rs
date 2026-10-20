//! Skill link ownership, validation, and writes.

use super::*;

#[derive(Debug, PartialEq)]
pub(super) enum LinkLeaf {
    Absent,
    Link(std::path::PathBuf),
    Other(&'static str),
}

/// Read only the leaf and its stored target, including broken links.
pub(super) fn link_leaf(path: &std::path::Path) -> Result<LinkLeaf, String> {
    let fail = |error| format!("swarm: cannot read link {}: {error}", path.display());
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LinkLeaf::Absent),
        Err(error) => Err(fail(error)),
        Ok(meta) if meta.is_symlink() => {
            let target = std::fs::read_link(path).map_err(fail)?;
            if target.to_str().is_none() {
                return Err(format!(
                    "swarm: {} has a link target that is not UTF-8",
                    path.display()
                ));
            }
            Ok(LinkLeaf::Link(target))
        }
        Ok(meta) if meta.is_dir() => Ok(LinkLeaf::Other("directory")),
        Ok(meta) if meta.is_file() => Ok(LinkLeaf::Other("regular file")),
        Ok(_) => Ok(LinkLeaf::Other("other leaf type")),
    }
}

fn recorded_link(
    store: &rusqlite::Connection,
    edit: &Edit,
) -> Result<Option<std::path::PathBuf>, String> {
    stored_link_target(store, edit, false)
}

fn stored_link_target(
    store: &rusqlite::Connection,
    edit: &Edit,
    off: bool,
) -> Result<Option<std::path::PathBuf>, String> {
    use rusqlite::OptionalExtension;
    let target: Option<String> = store
        .query_row(
            "SELECT wrote FROM managed_edit WHERE id = ?1 AND writer = ?2
         AND file = ?3 AND kind = 'symlink' AND path = '[]' AND off = ?4
         AND before IS NULL AND created = 0 AND with_id IS NULL",
            rusqlite::params![
                edit.id(),
                wire(&edit.writer),
                edit.file.to_string_lossy(),
                off
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("swarm: cannot read managed edits: {error}"))?;
    target
        .map(|target| {
            let text: String = serde_json::from_str(&target)
                .map_err(|error| format!("swarm: cannot read managed link target: {error}"))?;
            let target = std::path::PathBuf::from(text);
            if !target.is_absolute() {
                return Err("swarm: recorded skill link target must be absolute".into());
            }
            Ok(target)
        })
        .transpose()
}

impl LinkPlan {
    /// Plan one skill link. A matching unrecorded link belongs to its existing owner.
    pub fn create(
        store: &rusqlite::Connection,
        path: std::path::PathBuf,
        target: std::path::PathBuf,
    ) -> Result<Self, String> {
        let target_text = target
            .to_str()
            .filter(|_| target.is_absolute())
            .ok_or_else(|| "swarm: a skill link target must be an absolute UTF-8 path".to_string())?
            .to_string();
        let edit = Edit::new(
            Writer::Skills,
            &path,
            Kind::Symlink,
            &[],
            target_text.clone().into(),
        );
        let recorded = recorded_link(store, &edit)?;
        let mut plan = Self {
            path,
            before: LinkState::Absent,
            after: LinkState::Link(target),
            conflicts: Vec::new(),
            edits: vec![edit],
        };
        let conflict = match link_leaf(&plan.path) {
            Ok(LinkLeaf::Absent) => None,
            Ok(LinkLeaf::Link(live))
                if recorded
                    .as_ref()
                    .is_some_and(|target| target.as_os_str() == live.as_os_str()) =>
            {
                plan.before = LinkState::Link(live);
                None
            }
            Ok(leaf) => {
                let kind = if recorded.is_some() {
                    ConflictKind::Changed
                } else {
                    ConflictKind::Taken
                };
                let found = match leaf {
                    LinkLeaf::Link(target) => {
                        plan.before = LinkState::Link(target.clone());
                        format!("link -> {}", target.display())
                    }
                    LinkLeaf::Other(kind) => kind.into(),
                    LinkLeaf::Absent => unreachable!(),
                };
                Some((kind, found))
            }
            Err(error) => Some((ConflictKind::Unreadable, error)),
        };
        if let Some((kind, found)) = conflict {
            plan.after = plan.before.clone();
            plan.conflicts.push(Conflict {
                kind,
                file: plan.path.display().to_string(),
                entry: "the symlink".into(),
                found,
                wanted: format!("link -> {}", target_text),
                fix: format!(
                    "swarm leaves it; move or repair {} and check the plan again",
                    plan.path.display()
                ),
            });
        }
        Ok(plan)
    }
}

pub(super) fn validate_link(
    store: &rusqlite::Connection,
    plan: &LinkPlan,
    off: bool,
) -> Result<(), String> {
    let invalid = || format!("swarm: invalid link plan for {}", plan.path.display());
    let [edit] = plan.edits.as_slice() else {
        return Err(invalid());
    };
    let target = edit
        .wrote
        .as_str()
        .map(std::path::Path::new)
        .filter(|target| target.is_absolute())
        .ok_or_else(invalid)?;
    if edit.kind != Kind::Symlink
        || edit.writer != Writer::Skills
        || !edit.path.is_empty()
        || edit.before.is_some()
        || edit.created != 0
        || edit.with.is_some()
        || edit.last
    {
        return Err(invalid());
    }
    let recorded = recorded_link(store, edit)?;
    let recorded = if off && plan.before == LinkState::Absent && recorded.is_none() {
        stored_link_target(store, edit, true)?
    } else {
        recorded
    };
    let expected_record = match (&plan.before, &plan.after, off) {
        (LinkState::Absent, LinkState::Link(after), false)
            if after.as_os_str() == target.as_os_str() =>
        {
            None
        }
        (LinkState::Link(before), LinkState::Link(after), false)
            if after.as_os_str() == target.as_os_str() =>
        {
            Some(before.as_path())
        }
        (LinkState::Link(before), LinkState::Absent, true)
            if before.as_os_str() == target.as_os_str() =>
        {
            Some(target)
        }
        (LinkState::Absent, LinkState::Absent, true) => Some(target),
        _ => return Err(invalid()),
    };
    if let Some(expected) = expected_record
        && recorded
            .as_ref()
            .is_none_or(|stored| stored.as_os_str() != expected.as_os_str())
    {
        return Err(format!(
            "swarm: {} is not the recorded link; swarm leaves it",
            plan.path.display()
        ));
    }
    validate_link_leaf(plan)
}

fn validate_link_leaf(plan: &LinkPlan) -> Result<(), String> {
    if link_place(&plan.path) != plan.edits[0].file {
        return Err(format!(
            "swarm: the parent of {} changed after the plan",
            plan.path.display()
        ));
    }
    match (link_leaf(&plan.path)?, &plan.before) {
        (LinkLeaf::Absent, LinkState::Absent) => Ok(()),
        (LinkLeaf::Link(live), LinkState::Link(before))
            if live.as_os_str() == before.as_os_str() =>
        {
            Ok(())
        }
        _ => Err(format!(
            "swarm: {} changed after the link plan; swarm leaves it",
            plan.path.display()
        )),
    }
}

pub(super) fn write_link(plan: &LinkPlan) -> Result<(), String> {
    let fail = |error| format!("swarm: cannot write link {}: {error}", plan.path.display());
    match &plan.after {
        LinkState::Link(target) => {
            let parent = plan
                .path
                .parent()
                .ok_or_else(|| format!("swarm: {} has no parent", plan.path.display()))?;
            std::fs::create_dir_all(parent).map_err(fail)?;
            validate_link_leaf(plan)?;
            if matches!(plan.before, LinkState::Link(_)) {
                std::fs::remove_file(&plan.path).map_err(fail)?;
            }
            std::os::unix::fs::symlink(target, &plan.path).map_err(fail)
        }
        LinkState::Absent => {
            validate_link_leaf(plan)?;
            std::fs::remove_file(&plan.path).map_err(fail)
        }
    }
}

pub(super) fn link_removal(
    store: &rusqlite::Connection,
    entry: &Entry,
) -> Result<LinkPlan, String> {
    let edit = &entry.edit;
    let mut plan = LinkPlan {
        path: edit.file.clone(),
        before: LinkState::Absent,
        after: LinkState::Absent,
        conflicts: Vec::new(),
        edits: vec![edit.clone()],
    };
    let conflict = match link_leaf(&plan.path) {
        Ok(LinkLeaf::Absent) => None,
        Ok(LinkLeaf::Link(target)) => {
            plan.before = LinkState::Link(target.clone());
            if entry.recorded
                && recorded_link(store, edit)?
                    .as_ref()
                    .is_some_and(|stored| stored.as_os_str() == target.as_os_str())
                && edit.wrote.as_str() == target.to_str()
            {
                None
            } else {
                Some((
                    ConflictKind::Changed,
                    format!("link -> {}", target.display()),
                ))
            }
        }
        Ok(LinkLeaf::Other(kind)) => Some((ConflictKind::Changed, kind.into())),
        Err(error) => Some((ConflictKind::Unreadable, error)),
    };
    if let Some((kind, found)) = conflict {
        plan.after = plan.before.clone();
        plan.conflicts.push(Conflict {
            kind,
            file: plan.path.display().to_string(),
            entry: "the symlink".into(),
            found,
            wanted: edit.wrote.to_string(),
            fix: "swarm leaves it; restore the recorded link target and check the plan again"
                .into(),
        });
    }
    Ok(plan)
}
