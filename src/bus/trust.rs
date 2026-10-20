// Project trust plans and trust-target checks.
use super::*;

/// The plan that marks `dir` trusted in a Codex home's `config.toml`. Codex reads folder trust
/// from its config file; its `-c` override does not satisfy the dialog. The file is edited as
/// TOML, so a project the owner wrote in any form counts as present and swarm adds nothing to it
/// (ADR 0036, C3); a second table for it would make the file unreadable.
pub fn codex_trust_plan(home: &std::path::Path, dir: &std::path::Path) -> Result<FilePlan, String> {
    let path = home.join("config.toml");
    codex_trust_plan_on(&path, read_text(&path)?, dir)
}

/// `codex_trust_plan` on `before`, the text another plan of the same setup leaves in `path`, so
/// the two plans apply one after the other.
pub fn codex_trust_plan_on(
    path: &std::path::Path,
    before: String,
    dir: &std::path::Path,
) -> Result<FilePlan, String> {
    let path = path.to_path_buf();
    let mut config: toml_edit::DocumentMut = before.parse().map_err(|error| {
        format!(
            "{} is not valid TOML, so swarm does not edit it: {error}",
            path.display()
        )
    })?;
    let dir = dir.to_string_lossy();
    let had_projects = config.get("projects").is_some();
    let projects = toml_table(config.as_table_mut(), "projects", true).ok_or_else(|| {
        format!(
            "{} has a projects that is not a table; trust {dir} by hand",
            path.display()
        )
    })?;
    let mut edits = Vec::new();
    if !projects.contains_key(&dir) {
        toml_table(projects, &dir, false)
            .expect("a missing key becomes a table")
            .insert("trust_level", toml_edit::value("trusted"));
        edits.push(Edit {
            // The containers the file lacked (`Edit.created`): the folder's table, and
            // `projects` when it was missing too.
            created: 1 + u8::from(!had_projects),
            ..Edit::new(
                Writer::LaunchTrust,
                &path,
                Kind::TomlKey,
                &["projects", &dir, "trust_level"],
                "trusted".into(),
            )
        });
    }
    trust_plan(
        path,
        before,
        (!edits.is_empty()).then(|| config.to_string()),
        edits,
    )
}

/// Whether `text`, a Codex `config.toml`, holds `dir` with no `trust_level = "trusted"`. Swarm
/// leaves such a project as the owner set it, so Codex asks in the pane (C3).
pub fn codex_left_untrusted(text: &str, dir: &std::path::Path) -> bool {
    text.parse::<toml_edit::DocumentMut>().is_ok_and(|config| {
        config
            .get("projects")
            .and_then(|projects| projects.get(&*dir.to_string_lossy()))
            .is_some_and(|project| {
                project.get("trust_level").and_then(toml_edit::Item::as_str) != Some("trusted")
            })
    })
}

/// A trust plan for `path`: `after` is the planned text, None when nothing changes.
fn trust_plan(
    path: std::path::PathBuf,
    before: String,
    after: Option<String>,
    edits: Vec<Edit>,
) -> Result<FilePlan, String> {
    if after.is_some() {
        refuse_read_only(&path)?;
    }
    Ok(FilePlan {
        after: after.unwrap_or_else(|| before.clone()),
        path,
        before,
        conflicts: Vec::new(),
        edits,
    })
}

/// Write one trust plan through the managed-edits module, so the write is recorded (ADR 0042).
/// A running CLI may rewrite its file at any moment, so a plan whose file changed is made again.
/// Returns the plan written, or None when the entry was already there.
pub fn write_trust(
    store: &rusqlite::Connection,
    plan: impl Fn() -> Result<FilePlan, String>,
) -> Result<Option<FilePlan>, String> {
    retried(|| {
        let plan = plan()?;
        if plan.after == plan.before {
            return Ok(None);
        }
        crate::managed::apply(store, &[plan.clone().into()])?;
        Ok(Some(plan))
    })
}

/// The directory `swarm launch` may mark trusted for Codex and AGY: the git root when `cwd` is in a
/// repository, since Codex keys trust on it, or else `cwd` itself when it sits inside one of the
/// scratch roots swarm and the council write. $HOME and `/` are too broad, and every dir from `cwd`
/// up to that root must belong to the user and be closed to group and world writes, so another
/// account cannot plant files in a place the agents then trust.
pub fn trust_target(
    cwd: &std::path::Path,
    git_root: Option<&std::path::Path>,
    home: &std::path::Path,
    scratch_roots: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::MetadataExt;
    let (target, top) = match git_root {
        Some(root) => (root.to_path_buf(), root.to_path_buf()),
        None => {
            let root = scratch_roots
                .iter()
                .find(|root| cwd.starts_with(root) && cwd != root.as_path())
                .ok_or_else(|| {
                    format!(
                        "{} is not in a git repository or a scratch dir",
                        cwd.display()
                    )
                })?;
            (cwd.to_path_buf(), root.clone())
        }
    };
    if target == home || target.parent().is_none() {
        return Err(format!("{} is too broad to trust", target.display()));
    }
    let uid = std::fs::metadata(home)
        .map_err(|error| format!("{}: {error}", home.display()))?
        .uid();
    // Claude keys its trust on `cwd`, and each agent reads project config from the dirs between
    // `cwd` and the root, so the walk starts at `cwd`.
    if !cwd.starts_with(&top) {
        return Err(format!("{} is not inside {}", cwd.display(), top.display()));
    }
    let mut dir = cwd;
    loop {
        let meta = std::fs::metadata(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        if meta.uid() != uid || meta.mode() & 0o022 != 0 {
            return Err(format!(
                "{} must be yours and closed to group and world writes",
                dir.display()
            ));
        }
        if dir == top {
            return Ok(target);
        }
        dir = dir
            .parent()
            .ok_or_else(|| format!("{} left its root", target.display()))?;
    }
}

/// The plan that marks `dir` trusted in a Claude `.claude.json`, so a pane does not boot into the
/// folder-trust dialog and wait there with nobody to answer. Claude writes `false` itself for a
/// folder it saw before trust, so a `false` is replaced and the record keeps it (Q4).
pub fn claude_trust_plan(
    config: &std::path::Path,
    dir: &std::path::Path,
) -> Result<FilePlan, String> {
    let (before, mut value) = read_json_object(config)?;
    let key = dir.to_string_lossy().into_owned();
    let root = value
        .as_object_mut()
        .expect("read_json_object returns an object");
    // The containers the file lacked (`Edit.created`): `projects` and the folder key, or only
    // the folder key, or none.
    let created = match root.get("projects") {
        None => 2,
        Some(projects) => u8::from(projects.get(&key).is_none()),
    };
    let project = root
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("swarm: projects in ~/.claude.json is not an object")?
        .entry(key.clone())
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("swarm: a project entry in ~/.claude.json is not an object")?;
    let found = project.get("hasTrustDialogAccepted").cloned();
    if found == Some(true.into()) {
        return trust_plan(config.to_path_buf(), before, None, Vec::new());
    }
    project.insert("hasTrustDialogAccepted".into(), true.into());
    let edit = Edit {
        before: found,
        created,
        ..Edit::new(
            Writer::LaunchTrust,
            config,
            Kind::JsonKey,
            &["projects", &key, "hasTrustDialogAccepted"],
            true.into(),
        )
    };
    trust_plan(
        config.to_path_buf(),
        before,
        Some(json_text(&value)),
        vec![edit],
    )
}

/// The plan that adds `dir` to AGY's `trustedWorkspaces`.
pub fn agy_trust_plan(
    settings: &std::path::Path,
    dir: &std::path::Path,
) -> Result<FilePlan, String> {
    let (before, mut value) = read_json_object(settings)?;
    let root = value
        .as_object_mut()
        .expect("read_json_object returns an object");
    let created = u8::from(!root.contains_key("trustedWorkspaces"));
    let trusted = root
        .entry("trustedWorkspaces")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or("swarm: trustedWorkspaces in the AGY settings is not a list")?;
    let dir = serde_json::Value::from(dir.to_string_lossy().into_owned());
    if trusted.contains(&dir) {
        return trust_plan(settings.to_path_buf(), before, None, Vec::new());
    }
    trusted.push(dir.clone());
    let edit = Edit {
        created,
        ..Edit::new(
            Writer::LaunchTrust,
            settings,
            Kind::JsonArrayItem,
            &["trustedWorkspaces"],
            dir,
        )
    };
    trust_plan(
        settings.to_path_buf(),
        before,
        Some(json_text(&value)),
        vec![edit],
    )
}
