use crate::managed::{
    Conflict, ConflictKind, Edit, FilePlan, Kind, Writer, json_object, json_text, read_json_object,
    read_optional, read_text, refuse_read_only, retried, write_json, write_text,
};
use crate::providers::Provider;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ResolvedRole {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub sandbox: Option<String>,
    pub approval: Option<String>,
    pub permission: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Agent {
    pub id: String,
    pub role: String,
    pub pane: Option<String>,
    pub provider: Option<String>,
    pub created_at: i64,
    pub alive: Option<bool>,
    /// `working`, `waiting`, `done`, or `failed`; null until the first report (ADR 0021).
    pub state: Option<String>,
    pub state_at_s: Option<i64>,
    /// `hook` or `screen`.
    pub state_source: Option<String>,
    pub state_detail: Option<String>,
    /// The chat log the agent's provider hooks reported; null until the first report.
    pub log: Option<String>,
    /// The question the agent's screen shows now, if any; `swarm answer` picks a choice.
    pub prompt: Option<crate::screen::Prompt>,
}

#[derive(Debug, Serialize)]
pub struct AgentList {
    pub agents: Vec<Agent>,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub seq: i64,
    pub sender: String,
    pub recipient: String,
    pub kind: String,
    pub body: Option<String>,
    pub created_at: i64,
    pub read: bool,
    /// What the last ring proved: `hook`, `screen`, `seen`, `unconfirmed`, or `unchecked`; null until a
    /// ring ends. More values may come (ADR 0041).
    pub delivery: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageList {
    pub messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: String,
    pub talk_mode: String,
    pub adapter: Option<String>,
    pub cwd: String,
    pub created_at: i64,
    pub chair_provider: Option<String>,
    pub chair_id: Option<String>,
    pub chair_log: Option<String>,
    pub continuation_of: Option<String>,
    pub agents: i64,
    pub messages: i64,
    pub last_message_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionList {
    pub sessions: Vec<Session>,
}

pub fn valid_agent_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=40).contains(&bytes.len())
        && bytes
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

pub fn argv(
    agent_id: &str,
    role: &str,
    resolved: &ResolvedRole,
    swarm_home: &str,
) -> Result<Vec<String>, String> {
    let provider = required(role, "provider", resolved.provider.as_deref())?;
    let effort = || required(role, "effort", resolved.effort.as_deref());
    let model = || required(role, "model", resolved.model.as_deref());
    let Some(provider) = Provider::parse(provider) else {
        return Err(format!(
            "swarm: role {role} uses unsupported provider {provider}"
        ));
    };
    match provider {
        Provider::Claude => {
            let mut args = vec![
                provider.id().into(),
                "--model".into(),
                model()?.into(),
                "--effort".into(),
                effort()?.into(),
            ];
            if let Some(permission) = &resolved.permission {
                args.extend(["--permission-mode".into(), permission.clone()]);
            }
            let handler = |command: String| serde_json::json!({"hooks": [{"type": "command", "command": command, "timeout": STATE_HOOK_TIMEOUT}]});
            let state = state_hook_command(provider.id())?;
            let mut hooks = serde_json::Map::new();
            for event in CLAUDE_STATE_EVENTS {
                hooks.insert(event.into(), serde_json::json!([handler(state.clone())]));
            }
            // `/clear` opens a new log, so every Claude agent reports SessionStart state; the chair
            // also records its new session id.
            let mut start = vec![handler(state.clone())];
            if agent_id == "orchestrator" {
                start.insert(0, handler(chair_hook_command(provider.id())?));
            }
            hooks.insert("SessionStart".into(), start.into());
            args.extend([
                "--settings".into(),
                serde_json::json!({ "hooks": hooks }).to_string(),
            ]);
            Ok(args)
        }
        Provider::Codex => {
            let mut args = vec![
                provider.id().into(),
                "--model".into(),
                model()?.into(),
                "-c".into(),
                format!("model_reasoning_effort=\"{}\"", effort()?),
                // Codex asks "Update available! 1. Update now 2. Skip" before it reads anything,
                // and an agent pane has nobody at it to answer.
                "-c".into(),
                "check_for_update_on_startup=false".into(),
            ];
            if let Some(sandbox) = &resolved.sandbox {
                args.extend(["--sandbox".into(), sandbox.clone()]);
                if sandbox == "workspace-write" {
                    // Codex refuses every command when a writable root has a symlink in its path.
                    let root = format!("{swarm_home}/.swarm");
                    let root = std::fs::canonicalize(&root)
                        .map_or(root, |path| path.to_string_lossy().into_owned());
                    args.extend([
                        "-c".into(),
                        format!(
                            "sandbox_workspace_write.writable_roots=[{}]",
                            serde_json::to_string(&root).expect("string serialization cannot fail")
                        ),
                    ]);
                }
            }
            if let Some(approval) = &resolved.approval {
                args.extend(["--ask-for-approval".into(), approval.clone()]);
            }
            // `swarm hooks setup` trusts these for the owner (`codex_hook_trust`); the trust holds
            // only while each value stays byte-identical, so nothing per launch goes in it. The
            // pane env names the agent. The empty first group puts swarm's hook in group 1, so
            // its trust key differs from another tool's `-c` hook in group 0.
            let command = serde_json::to_string(&shared_hook_command(provider.id()))
                .expect("string serialization cannot fail");
            for event in CODEX_STATE_EVENTS {
                args.extend([
                    "-c".into(),
                    format!(
                        "hooks.{event}=[{{hooks=[]}},{{hooks=[{{type=\"command\",command={command},timeout={STATE_HOOK_TIMEOUT}}}]}}]"
                    ),
                ]);
            }
            Ok(args)
        }
        Provider::Agy => {
            let mut args = vec![provider.id().into()];
            if let Some(model) = resolved
                .model
                .as_deref()
                .filter(|model| *model != "default")
            {
                args.extend(["--model".into(), model.into()]);
            }
            let effort = effort()?;
            // An AGY model id such as `gemini-3.8-flash-high` fixes its own effort, and AGY
            // refuses `--effort` for it.
            if !resolved.model.as_deref().is_some_and(|model| {
                ["-low", "-medium", "-high"]
                    .iter()
                    .any(|level| model.ends_with(level))
            }) {
                args.extend(["--effort".into(), effort.into()]);
            }
            if let Some(permission) = &resolved.permission {
                if permission == "skip" {
                    args.push("--dangerously-skip-permissions".into());
                } else {
                    args.extend(["--mode".into(), permission.clone()]);
                }
            }
            Ok(args)
        }
    }
}

/// The model a provider command line names with `--model` or `-m`, without a `[1m]` suffix.
pub fn command_model(command: &[String]) -> Option<&str> {
    let mut args = command.iter().take_while(|arg| *arg != "--");
    while let Some(arg) = args.next() {
        let model = match arg.strip_prefix("--model=") {
            Some(model) => model,
            None if arg == "--model" || arg == "-m" => args.next()?,
            None => continue,
        };
        return model.split('[').next();
    }
    None
}

/// Codex reads folder trust from its config file; its `-c` override does not satisfy the dialog.
/// The file is edited as TOML, so a project the owner wrote in any form counts as present and
/// swarm adds nothing to it (ADR 0036); a second table for it would make the file unreadable.
pub fn ensure_codex_trust(home: &std::path::Path, cwd: &std::path::Path) -> Result<(), String> {
    retried(|| {
        let path = home.join("config.toml");
        let (before, mut config) = read_codex_config(&path)?;
        let dir = cwd.to_string_lossy();
        let projects = toml_table(config.as_table_mut(), "projects", true).ok_or_else(|| {
            format!(
                "{} has a projects that is not a table; trust {dir} by hand",
                path.display()
            )
        })?;
        if projects.contains_key(&dir) {
            return Ok(());
        }
        toml_table(projects, &dir, false)
            .expect("a missing key becomes a table")
            .insert("trust_level", toml_edit::value("trusted"));
        write_text(&path, &before, &config.to_string())
    })
}

/// A Codex `config.toml` as text and as TOML.
fn read_codex_config(path: &std::path::Path) -> Result<(String, toml_edit::DocumentMut), String> {
    let existing = read_text(path)?;
    let config = existing.parse().map_err(|error| {
        format!(
            "{} is not valid TOML, so swarm does not edit it: {error}",
            path.display()
        )
    })?;
    Ok((existing, config))
}

/// The Codex `hooks.state` key and trusted hash of each state hook `swarm launch` passes with
/// `-c`, as Codex computes them (codex-rs `hooks/src/engine/discovery.rs` `hook_hash` and
/// `config/src/fingerprint.rs` `version_for_toml`): the SHA-256 of the hook's identity as JSON with
/// sorted keys. `tests` pins the values a real Codex 0.159.0 app-server reported.
pub fn codex_hook_trust(command: &str) -> Vec<(String, String)> {
    CODEX_STATE_EVENTS
        .iter()
        .map(|event| {
            let label = codex_event_label(event);
            (
                format!("/<session-flags>/config.toml:{label}:1:0"),
                codex_hash(&label, None, command, STATE_HOOK_TIMEOUT, false),
            )
        })
        .collect()
}

/// The timeout of each state hook that swarm passes or writes, in seconds.
const STATE_HOOK_TIMEOUT: u64 = 3;

/// The hash Codex trusts for one command hook: its handler and its group's matcher.
fn codex_hash(
    label: &str,
    matcher: Option<&str>,
    command: &str,
    timeout: u64,
    is_async: bool,
) -> String {
    use sha2::Digest;
    // Keys in sorted order; with `preserve_order` the map keeps them so.
    let mut identity = serde_json::json!({
        "event_name": label,
        "hooks": [{"async": is_async, "command": command, "timeout": timeout, "type": "command"}],
    });
    if let Some(matcher) = matcher {
        identity["matcher"] = matcher.into();
    }
    let digest = sha2::Sha256::digest(identity.to_string().as_bytes());
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256:{hash}")
}

/// The Codex trust entry of each guard handler in a Codex home's `hooks.json` text, keyed by its
/// group and handler place as Codex counts them (`codex app-server` 0.159.0 `hooks/list`,
/// 2026-10-05). The hash follows the handler as the file holds it, so an owner's own timeout or
/// matcher is trusted as it is. A handler with no timeout gets no entry, because Codex's default
/// is not known here; Codex then asks for review.
pub fn codex_guard_trust(home: &std::path::Path, hooks: &str) -> Vec<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(hooks).unwrap_or_default();
    let file = home.join("hooks.json");
    let mut entries = Vec::new();
    for event in crate::guard::EVENTS {
        let command = guard_command("codex", event);
        let label = codex_event_label(event);
        let groups = value["hooks"][event].as_array().into_iter().flatten();
        for (group_index, group) in groups.enumerate() {
            let handlers = group["hooks"].as_array().into_iter().flatten();
            for (handler_index, handler) in handlers.enumerate() {
                let Some(timeout) = handler["timeout"].as_u64() else {
                    continue;
                };
                if handler["command"] != command.as_str() {
                    continue;
                }
                entries.push((
                    format!("{}:{label}:{group_index}:{handler_index}", file.display()),
                    codex_hash(
                        &label,
                        group["matcher"].as_str(),
                        &command,
                        timeout,
                        handler["async"].as_bool().unwrap_or(false),
                    ),
                ));
            }
        }
    }
    entries
}

/// Codex's snake_case name for a hook event, as its trust keys use it.
fn codex_event_label(event: &str) -> String {
    let mut label = String::new();
    for (index, character) in event.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            label.push('_');
        }
        label.push(character.to_ascii_lowercase());
    }
    label
}

/// The table `key` of `parent`, added when it is missing. An added `implicit` table writes no
/// header of its own. None when `key` holds a value that is not a table.
fn toml_table<'a>(
    parent: &'a mut dyn toml_edit::TableLike,
    key: &str,
    implicit: bool,
) -> Option<&'a mut dyn toml_edit::TableLike> {
    if parent.get(key).is_none() {
        let mut table = toml_edit::Table::new();
        table.set_implicit(implicit);
        parent.insert(key, toml_edit::Item::Table(table));
    }
    parent.get_mut(key)?.as_table_like_mut()
}

/// The plan for a Codex home's `config.toml`, with the trust keys of swarm's `state` hooks and of
/// its `guard` registration. A missing trust entry is added; an entry with swarm's hash stays, in
/// any TOML form or key spelling; any other entry at swarm's key is a conflict. The file is edited
/// as TOML, so an entry is never added twice: a second table with the same name makes the whole
/// file unreadable to Codex.
pub fn codex_hook_plan(
    home: &std::path::Path,
    state: &[(String, String)],
    guard: &[(String, String)],
) -> Result<FilePlan, String> {
    let path = home.join("config.toml");
    let (before, mut config) = read_codex_config(&path)?;
    let file = path.display().to_string();
    let not_table = |name: &str| {
        format!("{file} has a {name} that is not a table; set swarm's trusted_hash by hand")
    };
    // The tables the file lacks now, so a revert removes them again once they are empty.
    let had_hooks = config.get("hooks").is_some();
    let had_state = config
        .get("hooks")
        .and_then(|hooks| hooks.get("state"))
        .is_some();
    let created = 1 + u8::from(!had_hooks) + u8::from(!had_state);
    let hooks =
        toml_table(config.as_table_mut(), "hooks", true).ok_or_else(|| not_table("hooks"))?;
    let table = toml_table(hooks, "state", true).ok_or_else(|| not_table("hooks.state"))?;
    let mut conflicts = Vec::new();
    let mut edits = Vec::new();
    let writers = state.iter().map(|entry| (entry, Writer::HooksState));
    for ((key, hash), writer) in
        writers.chain(guard.iter().map(|entry| (entry, Writer::HooksGuard)))
    {
        let found = match table.get(key) {
            Some(entry) => entry.get("trusted_hash").and_then(toml_edit::Item::as_str),
            None => {
                toml_table(&mut *table, key, false)
                    .expect("a missing key becomes a table")
                    .insert("trusted_hash", toml_edit::value(hash.as_str()));
                edits.push(Edit {
                    created,
                    ..codex_trust_edit(home, key, hash, writer)
                });
                continue;
            }
        };
        if found != Some(hash.as_str()) {
            conflicts.push(Conflict {
                kind: ConflictKind::Taken,
                file: file.clone(),
                entry: format!("[hooks.state.{key:?}]"),
                found: found.unwrap_or("an entry with no trusted_hash").to_string(),
                wanted: hash.clone(),
                fix: format!(
                    "delete [hooks.state.{key:?}] from {file}, or do not use swarm's Codex hooks"
                ),
            });
        }
    }
    // TOML output ends in a newline, so a file that only lacks one is kept as it is.
    let after = if edits.is_empty() {
        before.clone()
    } else {
        config.to_string()
    };
    if after != before {
        refuse_read_only(&path)?;
    }
    Ok(FilePlan {
        path,
        before,
        after,
        conflicts,
        edits,
    })
}

/// The trust key `key` with `hash` in a Codex home's `config.toml`. A guard key goes with the
/// guard group it trusts, so a revert of either removes both: Codex asks again for a group whose
/// trust is gone, and a trust key left without its group trusts nothing.
pub fn codex_trust_edit(home: &std::path::Path, key: &str, hash: &str, writer: Writer) -> Edit {
    let mut edit = Edit::new(
        writer,
        &home.join("config.toml"),
        Kind::TomlKey,
        &["hooks", "state", key, "trusted_hash"],
        hash.into(),
    );
    if writer == Writer::HooksGuard {
        // A guard key is `<hooks.json>:<event label>:<group>:<handler>` (`codex_guard_trust`).
        let label = key.rsplit(':').nth(2).unwrap_or_default();
        edit.with = crate::guard::EVENTS
            .iter()
            .find(|event| codex_event_label(event) == label)
            .map(|event| guard_group_edit(&home.join("hooks.json"), "codex", event).id());
    }
    edit
}

/// Whether a Codex home's `config.toml` trusts every entry.
pub fn codex_hooks_trusted(home: &std::path::Path, entries: &[(String, String)]) -> bool {
    let text = std::fs::read_to_string(home.join("config.toml")).unwrap_or_default();
    let Ok(config) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    let state = config.get("hooks").and_then(|hooks| hooks.get("state"));
    entries.iter().all(|(key, hash)| {
        state
            .and_then(|state| state.get(key.as_str()))
            .and_then(|entry| entry.get("trusted_hash"))
            .and_then(toml_edit::Item::as_str)
            == Some(hash.as_str())
    })
}

/// Why a caller may not launch an agent, or None. A pane `swarm spawn` made carries its own
/// agent id, and only the orchestrator starts children; Herdr marks its own agent panes too.
pub fn launch_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane)
        .then_some("swarm: a child agent cannot launch agents; ask the orchestrator")
}

/// Why a caller may not revert the owner's managed changes, or None: the same child panes that
/// may not launch, because a revert removes the guard registration that blocks their own calls.
pub fn revert_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane).then_some(
        "swarm: a child agent cannot revert the owner's managed changes; ask the orchestrator",
    )
}

fn child_agent(swarm_agent: Option<&str>, herdr_agent_pane: Option<&str>) -> bool {
    swarm_agent.is_some_and(|agent| agent != "orchestrator") || herdr_agent_pane == Some("1")
}

/// Fable runs as a child only for review and council seats. The router refuses it too; this is
/// the second, independent check, so a config edited past the router still cannot reach a pane.
pub fn fable_refusal(agent_id: &str, role: &str, model: Option<&str>) -> Option<String> {
    let fable = model.is_some_and(|model| model.to_ascii_lowercase().contains("fable"));
    (fable
        && agent_id != "orchestrator"
        && !role.starts_with("review.")
        && !role.starts_with("council."))
    .then(|| format!("swarm: Fable is a child only for review.* and council.* (role {role})"))
}

/// Why this session has no spawn path, from the stderr of `herdr status`, or None. A sandbox that
/// denies the Herdr socket denies every later `herdr` call too, and a wider sandbox would spend a
/// user approval on a path this session is not meant to use.
pub fn socket_refusal(herdr_status_stderr: &str) -> Option<&'static str> {
    herdr_status_stderr.contains("PermissionDenied").then_some(
        "swarm: this sandbox denies the Herdr socket, so this session has no spawn path; report that and stop, and do not request a wider sandbox",
    )
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    // Everything past a bare `--` is a positional prompt, not a flag.
    args.iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| flags.contains(&arg.split('=').next().unwrap_or(arg)))
}

/// The caller's extra args for `provider`, checked and in the provider's shape.
pub fn extra_args(provider: Provider, extra: &[String]) -> Result<Vec<String>, String> {
    if has_flag(extra, provider.owned_flags()) {
        return Err(format!(
            "swarm: the role owns {} model, effort, sandbox, and approval flags; remove them from the extra args",
            provider.id()
        ));
    }
    // AGY reads a leading positional as a one-shot prompt; `-i` keeps the pane interactive.
    if provider == Provider::Agy && extra.first().is_some_and(|arg| !arg.starts_with('-')) {
        return Ok([vec!["-i".to_string()], extra.to_vec()].concat());
    }
    Ok(extra.to_vec())
}

/// Where a Claude child runs and the args that go with it. Claude files a transcript under its
/// cwd and has no way to move it, so a child runs from a pool dir under the caller's tree to stay
/// out of the caller's /resume picker; `--add-dir` gives it the tree back. Its session id takes a
/// reserved prefix so tools can tell a worker transcript apart. A resuming child keeps its cwd,
/// because its transcript already lives under the original one.
pub fn claude_child(
    agent_id: &str,
    cwd: &std::path::Path,
    extra: &[String],
    session_id: &uuid::Uuid,
) -> (std::path::PathBuf, Vec<String>) {
    const RESUME: &[&str] = &["-r", "--resume", "-c", "--continue", "--fork-session"];
    if has_flag(extra, RESUME) {
        return (cwd.to_path_buf(), extra.to_vec());
    }
    let mut args = Vec::new();
    if !has_flag(extra, &["--session-id"]) {
        let id = session_id.to_string();
        args.extend(["--session-id".to_string(), format!("aaaaaaaa{}", &id[8..])]);
        if !has_flag(extra, &["-n", "--name"]) {
            args.extend(["-n".to_string(), agent_id.to_string()]);
        }
    }
    args.extend_from_slice(extra);
    args.extend(["--add-dir".to_string(), cwd.to_string_lossy().into_owned()]);
    (cwd.join(".herdr").join("workers"), args)
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

/// Mark `dir` trusted in Claude's `~/.claude.json`, so a child does not boot into the folder-trust
/// dialog and wait there with nobody to answer. Returns whether the file changed.
pub fn ensure_claude_trust(
    config: &std::path::Path,
    dir: &std::path::Path,
) -> Result<bool, String> {
    retried(|| {
        let (before, mut value) = read_json_object(config)?;
        let key = dir.to_string_lossy().into_owned();
        let project = value
            .as_object_mut()
            .expect("read_json_object returns an object")
            .entry("projects")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or("swarm: projects in ~/.claude.json is not an object")?
            .entry(key)
            .or_insert_with(|| serde_json::json!({}));
        if project["hasTrustDialogAccepted"] == true {
            return Ok(false);
        }
        project
            .as_object_mut()
            .ok_or("swarm: a project entry in ~/.claude.json is not an object")?
            .insert("hasTrustDialogAccepted".into(), true.into());
        write_json(config, &before, &value).map(|()| true)
    })
}

/// Add `dir` to AGY's `trustedWorkspaces`. Returns whether the file changed.
pub fn ensure_agy_trust(settings: &std::path::Path, dir: &std::path::Path) -> Result<bool, String> {
    retried(|| {
        let (before, mut value) = read_json_object(settings)?;
        let trusted = value
            .as_object_mut()
            .expect("read_json_object returns an object")
            .entry("trustedWorkspaces")
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or("swarm: trustedWorkspaces in the AGY settings is not a list")?;
        let dir = serde_json::Value::from(dir.to_string_lossy().into_owned());
        if trusted.contains(&dir) {
            return Ok(false);
        }
        trusted.push(dir);
        write_json(settings, &before, &value).map(|()| true)
    })
}

/// This executable's path, single-quoted for a shell command line.
fn quoted_exe() -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("swarm: cannot find executable: {error}"))?
        .to_string_lossy()
        .replace('\'', "'\\''");
    Ok(format!("'{exe}'"))
}

fn chair_hook_command(provider: &str) -> Result<String, String> {
    let exe = quoted_exe()?;
    // Both CLI SessionStart payloads include a top-level session_id string.
    Ok(format!(
        "id=$(sed -n 's/.*\"session_id\"[[:space:]]*:[[:space:]]*\"\\([A-Za-z0-9-]\\{{1,64\\}}\\)\".*/\\1/p'); [ -n \"$id\" ] && {exe} session chair {provider}:\"$id\""
    ))
}

/// The command a provider hook runs to report agent state (ADR 0021).
pub fn state_hook_command(provider: &str) -> Result<String, String> {
    Ok(format!("{} hook {provider}", quoted_exe()?))
}

/// The state hook for Codex and AGY, whose hook config every swarm build on the Mac shares: Codex
/// trusts a hash of this text, and AGY's `hooks.json` is global. So the text names no build. It
/// runs the `runs/<session>/bin/swarm` link that `swarm launch` made for the pane, not `swarm` on
/// PATH, because a login shell can put another build first. Outside a swarm agent the link is
/// missing, and the hook prints `{}` as `swarm hook` does (ADR 0034). Codex runs a hook in the
/// user's `$SHELL`, which can be fish, so the script goes to `/bin/sh` as one single-quoted word
/// that every shell passes through as it is. `args` is `codex` or `agy <Event>`.
pub fn shared_hook_command(args: &str) -> String {
    format!(
        r#"/bin/sh -c '[ -x "$SWARM_HOME/.swarm/runs/$SWARM_SESSION_ID/bin/swarm" ] && exec "$SWARM_HOME/.swarm/runs/$SWARM_SESSION_ID/bin/swarm" hook {args}; printf "{{}}"'"#
    )
}

const CLAUDE_STATE_EVENTS: [&str; 8] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Elicitation",
    "Notification",
    "Stop",
    "StopFailure",
];

const CODEX_STATE_EVENTS: [&str; 6] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "Interrupt",
];

/// The plan for AGY's global `hooks.json`, which has no per-process hook flag. The `swarm` group,
/// and with `guard` the `swarm-guard` group, is added when missing, stays when it equals swarm's,
/// and is a conflict otherwise. Every other
/// group stays, and a file that is not a JSON object is refused. AGY sends no event name, so each
/// handler names it.
pub fn agy_hook_plan(path: &std::path::Path, guard: bool) -> Result<FilePlan, String> {
    let text = read_optional(path)?;
    let mut value = json_object(path, text.as_deref())?;
    let before = text.unwrap_or_default();
    let groups = value
        .as_object_mut()
        .expect("json_object returns an object");
    let file = path.display().to_string();
    let mut conflicts = Vec::new();
    let mut edits = Vec::new();
    // The guard is a group of its own (ADR 0040), so a Mac that has swarm's state group keeps it.
    let mut wanted = vec![agy_group_edit(path, "swarm")];
    if guard {
        wanted.push(agy_group_edit(path, "swarm-guard"));
    }
    for edit in wanted {
        let (name, group) = (edit.path[0].as_str(), &edit.wrote);
        match groups.get(name) {
            None => {
                groups.insert(name.into(), group.clone());
                edits.push(edit);
            }
            Some(found) if found == group => {}
            Some(found) => conflicts.push(Conflict {
                kind: ConflictKind::Taken,
                file: file.clone(),
                entry: format!("group {name:?}"),
                found: found.to_string(),
                wanted: group.to_string(),
                fix: format!("rename or delete the {name:?} group in {file}"),
            }),
        }
    }
    if !conflicts.is_empty() {
        edits.clear();
    }
    let after = if edits.is_empty() {
        before.clone()
    } else {
        json_text(&value)
    };
    if after != before {
        refuse_read_only(path)?;
    }
    Ok(FilePlan {
        path: path.to_path_buf(),
        before,
        after,
        conflicts,
        edits,
    })
}

/// The group `name` (`swarm` or `swarm-guard`) as `agy_hook_plan` adds it to AGY's `hooks.json`.
pub fn agy_group_edit(path: &std::path::Path, name: &str) -> Edit {
    let (writer, group) = match name {
        "swarm" => (Writer::HooksState, agy_group()),
        _ => (Writer::HooksGuard, agy_guard_group()),
    };
    Edit::new(writer, path, Kind::JsonKey, &[name], group)
}

/// Whether AGY's `hooks.json` holds swarm's group as `agy_hook_plan` adds it.
pub fn agy_hooks_set(path: &std::path::Path) -> bool {
    read_json_object(path).is_ok_and(|(_, value)| value.get("swarm") == Some(&agy_group()))
}

/// Whether AGY's `hooks.json` holds the guard group as `agy_hook_plan` adds it.
pub fn agy_guard_set(path: &std::path::Path) -> bool {
    read_json_object(path)
        .is_ok_and(|(_, value)| value.get("swarm-guard") == Some(&agy_guard_group()))
}

/// Whether AGY's `hooks.json` has a `swarm-guard` group in any form.
pub fn agy_guard_present(path: &std::path::Path) -> bool {
    read_json_object(path).is_ok_and(|(_, value)| value.get("swarm-guard").is_some())
}

/// Whether a Claude `settings.json` or a Codex `hooks.json` runs swarm's guard command anywhere.
pub fn guard_registered(path: &std::path::Path, provider: &str) -> bool {
    read_json_object(path).is_ok_and(|(_, value)| {
        crate::guard::EVENTS.iter().any(|event| {
            value["hooks"][event].as_array().is_some_and(|groups| {
                groups
                    .iter()
                    .any(|group| has_handler(group, &guard_command(provider, event)))
            })
        })
    })
}

/// The command a guard registration runs. It is `swarm` on PATH, not a session's link, because
/// a chair that the owner started by hand has no swarm session (ADR 0040).
pub fn guard_command(provider: &str, event: &str) -> String {
    format!("swarm guard {provider} {event}")
}

fn guard_handler(provider: &str, event: &str) -> serde_json::Value {
    serde_json::json!({"type": "command", "command": guard_command(provider, event), "timeout": crate::guard::REGISTRATION_TIMEOUT})
}

fn has_handler(group: &serde_json::Value, command: &str) -> bool {
    group["hooks"]
        .as_array()
        .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == command))
}

fn agy_guard_group() -> serde_json::Value {
    let events: serde_json::Map<_, _> = crate::guard::EVENTS
        .iter()
        .map(|event| {
            let group =
                serde_json::json!([{"matcher": "*", "hooks": [guard_handler("agy", event)]}]);
            (event.to_string(), group)
        })
        .collect();
    events.into()
}

/// The plan for the guard registration in a Claude `settings.json` or a Codex `hooks.json`, which
/// share one shape. A group with no matcher is added to each guard event that has no handler
/// running swarm's guard command; every other group stays as it is.
pub fn guard_hooks_plan(path: &std::path::Path, provider: &str) -> Result<FilePlan, String> {
    let (before, mut value) = read_json_object(path)?;
    let file = path.display().to_string();
    let had_hooks = value.get("hooks").is_some();
    let hooks = value
        .as_object_mut()
        .expect("json_object returns an object")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err(format!("swarm: {file} has hooks that is not an object"));
    };
    let mut edits = Vec::new();
    let mut conflicts = Vec::new();
    for event in crate::guard::EVENTS {
        let had_event = hooks.contains_key(event);
        let Some(groups) = hooks
            .entry(event)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
        else {
            return Err(format!(
                "swarm: {file} has hooks.{event} that is not a list"
            ));
        };
        let command = guard_command(provider, event);
        let handlers: Vec<serde_json::Value> = groups
            .iter()
            .filter_map(|group| group["hooks"].as_array())
            .flatten()
            .filter(|handler| handler["command"] == command.as_str())
            .cloned()
            .collect();
        if handlers.is_empty() {
            let edit = guard_group_edit(path, provider, event);
            groups.push(edit.wrote.clone());
            edits.push(Edit {
                created: u8::from(!had_hooks) + u8::from(!had_event),
                ..edit
            });
        }
        // A CLI that times the hook out before the runner answers lets the call through, and
        // swarm cannot hash a Codex handler whose timeout the file does not state (ADR 0040).
        for handler in handlers.into_iter().filter(|handler| {
            handler["timeout"]
                .as_u64()
                .is_none_or(|timeout| timeout < crate::guard::REGISTRATION_TIMEOUT)
        }) {
            conflicts.push(Conflict {
                kind: ConflictKind::Taken,
                file: file.clone(),
                entry: format!("hooks.{event} handler {command:?}"),
                found: handler.to_string(),
                wanted: guard_handler(provider, event).to_string(),
                fix: format!(
                    "set its timeout to {} or more in {file}, so the CLI waits for the runner's answer and swarm can check and trust the handler",
                    crate::guard::REGISTRATION_TIMEOUT
                ),
            });
        }
    }
    let after = if edits.is_empty() {
        before.clone()
    } else {
        json_text(&value)
    };
    if after != before {
        refuse_read_only(path)?;
    }
    Ok(FilePlan {
        path: path.to_path_buf(),
        before,
        after,
        conflicts,
        edits,
    })
}

/// The group that `guard_hooks_plan` adds to `event` in a Claude `settings.json` or a Codex
/// `hooks.json`. A Codex group must stay last, because Codex keys hook trust by place: groups
/// after it would move when it goes and lose their trust.
pub fn guard_group_edit(path: &std::path::Path, provider: &str, event: &str) -> Edit {
    let group = serde_json::json!({"hooks": [guard_handler(provider, event)]});
    Edit {
        last: provider == "codex",
        ..Edit::new(
            Writer::HooksGuard,
            path,
            Kind::JsonArrayItem,
            &["hooks", event],
            group,
        )
    }
}

fn agy_group() -> serde_json::Value {
    let handler = |event: &str| serde_json::json!({"type": "command", "command": shared_hook_command(&format!("agy {event}")), "timeout": STATE_HOOK_TIMEOUT});
    // PostToolUse takes matcher groups; PreInvocation and Stop take a flat handler list.
    // Not PreToolUse: that is AGY's permission gate, which needs a `decision`, and the `{}` that
    // `swarm hook` prints makes AGY refuse every tool call in every AGY session.
    serde_json::json!({
        "PreInvocation": [handler("PreInvocation")],
        "PostToolUse": [{"matcher": "*", "hooks": [handler("PostToolUse")]}],
        "Stop": [handler("Stop")],
    })
}

fn required<'a>(role: &str, field: &str, value: Option<&'a str>) -> Result<&'a str, String> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("swarm: role {role} has no {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::{changed_error, with_lock};

    fn role(provider: &str) -> ResolvedRole {
        ResolvedRole {
            provider: Some(provider.into()),
            model: Some("model-1".into()),
            effort: Some("high".into()),
            sandbox: None,
            approval: None,
            permission: None,
        }
    }

    #[test]
    fn builds_each_provider_and_optional_flags() {
        let mut claude = role("claude");
        claude.permission = Some("acceptEdits".into());
        let claude_args = argv("orchestrator", "code.complex", &claude, "/home").unwrap();
        assert_eq!(
            claude_args[..7],
            [
                "claude",
                "--model",
                "model-1",
                "--effort",
                "high",
                "--permission-mode",
                "acceptEdits"
            ]
        );
        assert_eq!(claude_args[7], "--settings");
        let settings: serde_json::Value = serde_json::from_str(&claude_args[8]).unwrap();
        assert!(
            settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("session chair claude:")
        );

        let mut codex = role("codex");
        codex.sandbox = Some("workspace-write".into());
        codex.approval = Some("never".into());
        let codex_args = argv("coder", "coder", &codex, "/home x").unwrap();
        assert_eq!(
            codex_args[..13],
            [
                "codex",
                "--model",
                "model-1",
                "-c",
                "model_reasoning_effort=\"high\"",
                "-c",
                "check_for_update_on_startup=false",
                "--sandbox",
                "workspace-write",
                "-c",
                "sandbox_workspace_write.writable_roots=[\"/home x/.swarm\"]",
                "--ask-for-approval",
                "never"
            ]
        );
        assert_eq!(codex_args.len(), 13 + 2 * CODEX_STATE_EVENTS.len());

        let mut agy = role("agy");
        agy.permission = Some("skip".into());
        assert_eq!(
            argv("coder", "coder", &agy, "/home"),
            Ok(vec![
                "agy",
                "--model",
                "model-1",
                "--effort",
                "high",
                "--dangerously-skip-permissions"
            ]
            .into_iter()
            .map(String::from)
            .collect())
        );
        agy.model = Some("default".into());
        agy.permission = Some("plan".into());
        assert_eq!(
            argv("coder", "coder", &agy, "/home"),
            Ok(vec!["agy", "--effort", "high", "--mode", "plan"]
                .into_iter()
                .map(String::from)
                .collect())
        );
        agy.model = None;
        agy.permission = None;
        let agy_args = argv("coder", "coder", &agy, "/home").unwrap();
        assert_eq!(
            agy_args,
            vec!["agy", "--effort", "high"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert!(!agy_args.iter().any(|arg| arg.contains("SessionStart")));
        agy.model = Some("gemini-3.8-flash-high".into());
        assert_eq!(
            argv("coder", "coder", &agy, "/home"),
            Ok(vec!["agy", "--model", "gemini-3.8-flash-high"]
                .into_iter()
                .map(String::from)
                .collect())
        );
    }

    #[test]
    fn a_codex_writable_root_has_no_symlink_in_its_path() {
        let dir = std::env::temp_dir().join(format!("swarm-writable-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("real/.swarm")).unwrap();
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();
        let mut codex = role("codex");
        codex.sandbox = Some("workspace-write".into());
        let args = argv(
            "coder",
            "coder",
            &codex,
            &dir.join("link").to_string_lossy(),
        )
        .unwrap();
        let real = std::fs::canonicalize(dir.join("real/.swarm")).unwrap();
        assert!(
            args.contains(&format!(
                "sandbox_workspace_write.writable_roots=[{}]",
                serde_json::to_string(&real.to_string_lossy()).unwrap()
            )),
            "{args:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_launched_agent_reports_state_through_swarm_hook() {
        let claude_command = state_hook_command("claude").unwrap();
        assert!(claude_command.starts_with('\'') && claude_command.ends_with("' hook claude"));
        for agent in ["orchestrator", "coder"] {
            let args = argv(agent, "coder", &role("claude"), "/home").unwrap();
            assert_eq!(args.iter().filter(|arg| *arg == "--settings").count(), 1);
            let at = args.iter().position(|arg| arg == "--settings").unwrap();
            let settings: serde_json::Value = serde_json::from_str(&args[at + 1]).unwrap();
            for event in CLAUDE_STATE_EVENTS {
                assert_eq!(
                    settings["hooks"][event],
                    serde_json::json!([{"hooks": [{"type": "command", "command": claude_command, "timeout": 3}]}]),
                    "{agent} {event}"
                );
            }
            let start: Vec<&str> = settings["hooks"]["SessionStart"]
                .as_array()
                .unwrap()
                .iter()
                .map(|group| group["hooks"][0]["command"].as_str().unwrap())
                .collect();
            let chair = chair_hook_command("claude").unwrap();
            let expected = if agent == "orchestrator" {
                vec![chair.as_str(), claude_command.as_str()]
            } else {
                vec![claude_command.as_str()]
            };
            assert_eq!(start, expected, "{agent} SessionStart");
        }

        let hook_args = |agent: &str| {
            let args = argv(agent, "coder", &role("codex"), "/home").unwrap();
            args.windows(2)
                .filter(|pair| pair[0] == "-c" && pair[1].starts_with("hooks."))
                .map(|pair| pair[1].clone())
                .collect::<Vec<_>>()
        };
        let coder = hook_args("coder");
        assert_eq!(coder, hook_args("reviewer"));
        let codex_command = serde_json::to_string(&shared_hook_command("codex")).unwrap();
        assert_eq!(
            coder,
            CODEX_STATE_EVENTS
                .map(|event| format!(
                    "hooks.{event}=[{{hooks=[]}},{{hooks=[{{type=\"command\",command={codex_command},timeout=3}}]}}]"
                ))
                .to_vec()
        );
    }

    #[test]
    fn codex_hook_trust_matches_the_hashes_a_real_codex_reports() {
        // `codex app-server` 0.159.0 `hooks/list`, empty CODEX_HOME, 2026-09-29, for these `-c`
        // hooks with the command below.
        let reported = [
            (
                "user_prompt_submit",
                "6ada9e2b38032d391c688b4b57e23d57dc704db197c59846631231b6de096fa3",
            ),
            (
                "pre_tool_use",
                "1aae45a70c770d9f20e8b9bb0606f189958d43d2cfd087e63bb2a929c354c43f",
            ),
            (
                "post_tool_use",
                "8e869f2203b834c487812e07bb16ef48cab40dd79e4e4c8b5be9970e3e6a3c27",
            ),
            (
                "permission_request",
                "fb62179a34ddeb588a7474baeb6c99f976299120c70f7b5c495e76d904e7321a",
            ),
            (
                "stop",
                "64466f2454b871f7b36673eaea4506ac61f374a030bf4b162cc432ae54612cc5",
            ),
            (
                "interrupt",
                "e24a3286d74cee9090a79c7dcf48e828202cfcccb8a511ecfadc4d0786234a8c",
            ),
        ];
        assert_eq!(
            codex_hook_trust("'/opt/homebrew/bin/swarm' hook codex"),
            reported
                .map(|(label, hash)| (
                    format!("/<session-flags>/config.toml:{label}:1:0"),
                    format!("sha256:{hash}")
                ))
                .to_vec()
        );
    }

    #[test]
    fn the_shared_codex_hook_has_the_hashes_a_real_codex_reports() {
        // `codex app-server` 0.159.0 `hooks/list`, empty CODEX_HOME, 2026-10-01, for the `-c`
        // hooks that `argv` gives a Codex agent; each was `trusted` after `swarm hooks setup`.
        let reported = [
            (
                "user_prompt_submit",
                "3e695398bce1010ee0eebd574c64c8294ace860963b23ca648d56bc51f9e27c6",
            ),
            (
                "pre_tool_use",
                "dfa165f23090cec590f7881a0c22425aef660ddc3e7eb8a0f00e950d369b6ae6",
            ),
            (
                "post_tool_use",
                "35e581b33733e40e0cdf4a03e0b024f53f58e855cb33a9b3bb1c2799a82f077d",
            ),
            (
                "permission_request",
                "cf69969f87fb6625bca8375eeaf360217099bd09aed0a4ebf2041da02e442322",
            ),
            (
                "stop",
                "72d67131393317eb54b3bcc89528b7459b44e5133ef18e3abf6a54fa392144b9",
            ),
            (
                "interrupt",
                "d08c10f1e58597bc86da11ae4e0e209d094137ce0bd4811c54c21bbfa102e547",
            ),
        ];
        assert_eq!(
            codex_hook_trust(&shared_hook_command("codex")),
            reported
                .map(|(label, hash)| (
                    format!("/<session-flags>/config.toml:{label}:1:0"),
                    format!("sha256:{hash}")
                ))
                .to_vec()
        );
    }

    /// Setup for one file as `swarm hooks setup` does it: no write while the file has a conflict.
    /// Apply `plan` through the module with a store of its own; whether the file changed.
    fn set_up(plan: Result<FilePlan, String>) -> Result<bool, String> {
        apply(&[plan?]).map(|changed| !changed.is_empty())
    }

    fn apply(plans: &[FilePlan]) -> Result<Vec<std::path::PathBuf>, String> {
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        crate::managed::apply(&store, plans)
    }

    #[test]
    fn codex_hook_trust_is_added_once_and_an_older_hash_is_a_conflict() {
        let home = std::env::temp_dir().join(format!("swarm-codex-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        assert!(!codex_hooks_trusted(&home, &entries));

        // A fresh Mac has no Codex home at all.
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(codex_hooks_trusted(&home, &entries));
        assert!(!set_up(codex_hook_plan(&home, &entries, &[])).unwrap());

        // Another tool's group-0 entry stays. An older hash at swarm's key is a conflict that
        // names both hashes, and the file stays as it was (ADR 0036).
        let other = "[hooks.state.\"/<session-flags>/config.toml:stop:0:0\"]\ntrusted_hash = \"sha256:other\"";
        let config = home.join("config.toml");
        let text = std::fs::read_to_string(&config).unwrap();
        let owners = format!("model = \"x\"\n{other}\n\n{text}");
        std::fs::write(&config, &owners).unwrap();
        assert!(!set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        let moved = codex_hook_trust("'/opt/swarm' hook codex");
        let plan = codex_hook_plan(&home, &moved, &[]).unwrap();
        assert_eq!(plan.conflicts.len(), moved.len());
        assert_eq!(plan.conflicts[0].found, entries[0].1);
        assert_eq!(plan.conflicts[0].wanted, moved[0].1);
        assert!(
            plan.conflicts[0]
                .fix
                .contains(&config.display().to_string())
        );
        assert_eq!(plan.after, owners);
        assert!(set_up(codex_hook_plan(&home, &moved, &[])).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), owners);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_codex_trust_entry_in_any_form_is_found_and_never_added_twice() {
        let home = std::env::temp_dir().join(format!("swarm-codex-comment-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        set_up(codex_hook_plan(&home, &entries, &[])).unwrap();
        let config = home.join("config.toml");
        let plain = std::fs::read_to_string(&config).unwrap();

        // The owner commented each header and hash; both still count and none is added again.
        let commented: String = plain
            .lines()
            .map(
                |line| match line.starts_with('[') || line.starts_with("trusted_hash") {
                    true => format!("{line}  # swarm #1"),
                    false => line.to_string(),
                },
            )
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&config, &commented).unwrap();
        assert!(codex_hooks_trusted(&home, &entries));
        assert!(!set_up(codex_hook_plan(&home, &entries, &[])).unwrap());

        // An entry in another form or key spelling is found where it is, so its other hash is
        // one conflict and the missing entries are added once.
        let (key, _) = &entries[0];
        let dotted = format!("[hooks.state]\n{key:?}.trusted_hash = \"sha256:x\"\n");
        let escaped = format!("[hooks.state.{key:?}]\ntrusted_hash = \"sha256:x\"\n");
        let inline =
            format!("hooks = {{ state = {{ {key:?} = {{ trusted_hash = \"sha256:x\" }} }} }}\n");
        // A string that holds a table header, and a nested array, are only values.
        let values =
            format!("note = \"\"\"\n[projects]\n\"\"\"\nlists = [[1, 2], [3]]\n\n{escaped}");
        for form in [
            dotted.clone(),
            dotted.replacen('/', "\\u002f", 1),
            escaped.replacen('/', "\\u002f", 1),
            inline.replacen('/', "\\u002f", 1),
            values.replacen("[hooks.state.\"/", "[hooks.state.\"\\u002f", 1),
        ] {
            std::fs::write(&config, &form).unwrap();
            let plan = codex_hook_plan(&home, &entries, &[]).unwrap();
            assert_eq!(plan.conflicts.len(), 1, "{form}");
            assert_eq!(plan.conflicts[0].found, "sha256:x", "{form}");
            let parsed: toml_edit::DocumentMut = plan.after.parse().unwrap();
            assert_eq!(
                parsed["hooks"]["state"].as_table_like().unwrap().len(),
                entries.len(),
                "{}",
                plan.after
            );
            assert!(set_up(Ok(plan)).is_err());
            assert_eq!(std::fs::read_to_string(&config).unwrap(), form);
        }
        // An escape in a table that is not a hooks table does not block setup.
        let project = "[projects.\"\\u002ftmp/project\"]\ntrust_level = \"trusted\"\n";
        std::fs::write(&config, project).unwrap();
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(codex_hooks_trusted(&home, &entries));
        assert!(
            std::fs::read_to_string(&config)
                .unwrap()
                .starts_with(project)
        );

        // A file that is not valid TOML, such as one with the same table twice, is refused and
        // stays as it was.
        let twice = format!("{escaped}{escaped}");
        std::fs::write(&config, &twice).unwrap();
        assert!(codex_hook_plan(&home, &entries, &[]).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), twice);
        // So is a file that is not UTF-8, which cannot be read as text.
        let latin1 = b"model = \"o3\"\n# caf\xe9\n".to_vec();
        std::fs::write(&config, &latin1).unwrap();
        assert!(codex_hook_plan(&home, &entries, &[]).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), latin1);

        // A commented-out old entry is not an entry, so the missing ones are added.
        let old: String = plain.lines().map(|line| format!("# {line}\n")).collect();
        std::fs::write(&config, &old).unwrap();
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(codex_hooks_trusted(&home, &entries));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn agy_hooks_keep_other_groups_and_refuse_a_broken_file_or_an_owners_swarm_group() {
        let root = std::env::temp_dir().join(format!("swarm-agy-hooks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let hooks = root.join("config/hooks.json");
        assert!(set_up(agy_hook_plan(&hooks, false)).unwrap());
        let created: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&hooks).unwrap()).unwrap();
        assert_eq!(created.as_object().unwrap().len(), 1);
        assert_eq!(
            created["swarm"]["Stop"][0]["command"],
            shared_hook_command("agy Stop")
        );
        assert_eq!(
            created["swarm"]["PostToolUse"][0]["hooks"][0]["command"],
            shared_hook_command("agy PostToolUse")
        );
        assert!(created["swarm"].get("PreToolUse").is_none());
        assert!(!set_up(agy_hook_plan(&hooks, false)).unwrap());

        let herdr = r#"{"herdr": {"PreInvocation": [{"command": "herdr-state session", "timeout": 10, "type": "command"}]}}"#;
        std::fs::write(&hooks, herdr).unwrap();
        assert!(set_up(agy_hook_plan(&hooks, false)).unwrap());
        let merged: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&hooks).unwrap()).unwrap();
        let before: serde_json::Value = serde_json::from_str(herdr).unwrap();
        assert_eq!(merged["herdr"], before["herdr"]);
        assert_eq!(merged["swarm"], created["swarm"]);

        // The owner's own group named `swarm` is a conflict, not replaced (ADR 0036).
        let owners = r#"{"herdr": {}, "swarm": {"Stop": []}}"#;
        std::fs::write(&hooks, owners).unwrap();
        let plan = agy_hook_plan(&hooks, false).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].found, r#"{"Stop":[]}"#);
        assert_eq!(plan.after, owners);
        assert!(apply(std::slice::from_ref(&plan)).is_err());
        assert!(set_up(Ok(plan)).is_err());
        assert_eq!(std::fs::read_to_string(&hooks).unwrap(), owners);

        // Consent covers the planned text too, so a swarm whose entries differ is refused.
        let planned = |after: &str| FilePlan {
            path: hooks.clone(),
            before: owners.into(),
            after: after.into(),
            conflicts: Vec::new(),
            edits: Vec::new(),
        };
        assert_ne!(
            crate::managed::digest(&[planned("{}")]),
            crate::managed::digest(&[planned("{\"swarm\": {}}")])
        );

        std::fs::write(&hooks, "{ not json").unwrap();
        assert!(agy_hook_plan(&hooks, false).is_err());
        assert_eq!(std::fs::read_to_string(&hooks).unwrap(), "{ not json");
    }

    #[test]
    fn codex_trust_is_appended_once_per_cwd() {
        let root = std::env::temp_dir().join(format!("swarm-trust-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        ensure_codex_trust(&root, std::path::Path::new("/one")).unwrap();
        let once = std::fs::read_to_string(&config).unwrap();
        ensure_codex_trust(&root, std::path::Path::new("/one")).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(), once);
        ensure_codex_trust(&root, std::path::Path::new("/two")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&config)
                .unwrap()
                .matches("trust_level = \"trusted\"")
                .count(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The owner may have written the project in any TOML form. A second table for it would make
    /// the whole file unreadable to Codex, so swarm adds nothing (ADR 0036).
    #[test]
    fn codex_trust_keeps_a_project_in_any_form_and_a_broken_file() {
        let root = std::env::temp_dir().join(format!("swarm-trust-forms-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        for owner in [
            "[projects]\n\"/one\" = { trust_level = \"trusted\" }\n",
            "projects.\"/one\".trust_level = \"untrusted\"\n",
            "[projects.'/one']\ntrust_level = \"trusted\"\n",
            "projects = { \"/one\" = { trust_level = \"trusted\" } }\n",
            "not toml = = \n",
        ] {
            std::fs::write(&config, owner).unwrap();
            let result = ensure_codex_trust(&root, std::path::Path::new("/one"));
            assert_eq!(std::fs::read_to_string(&config).unwrap(), owner);
            assert_eq!(result.is_err(), owner.starts_with("not toml"), "{owner}");
        }
        std::fs::write(
            &config,
            "projects = { \"/one\" = { trust_level = \"trusted\" } }\n",
        )
        .unwrap();
        ensure_codex_trust(&root, std::path::Path::new("/two")).unwrap();
        let added: toml_edit::DocumentMut =
            std::fs::read_to_string(&config).unwrap().parse().unwrap();
        assert_eq!(
            added["projects"]["/one"]["trust_level"].as_str(),
            Some("trusted")
        );
        assert_eq!(
            added["projects"]["/two"]["trust_level"].as_str(),
            Some("trusted")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A Codex config that the owner links into dotfiles stays a link, and the change lands in
    /// the file it points to (ADR 0036: a linked file is judged by its target).
    #[test]
    fn a_linked_codex_config_stays_a_link_and_its_target_gets_the_change() {
        let root = std::env::temp_dir().join(format!("swarm-trust-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dotfiles = root.join("dotfiles/config.toml");
        let home = root.join("codex");
        std::fs::create_dir_all(dotfiles.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(&dotfiles, "model = \"o3\"\n").unwrap();
        let config = home.join("config.toml");
        std::os::unix::fs::symlink(&dotfiles, &config).unwrap();
        let linked = || {
            std::fs::symlink_metadata(&config)
                .unwrap()
                .file_type()
                .is_symlink()
        };

        ensure_codex_trust(&home, std::path::Path::new("/one")).unwrap();
        assert!(linked());
        assert!(
            std::fs::read_to_string(&dotfiles)
                .unwrap()
                .contains("[projects.\"/one\"]")
        );
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(linked());
        assert!(codex_hooks_trusted(&home, &entries));

        // A link to a file that does not exist yet gets its target made, as an append through the
        // link did before.
        std::fs::remove_file(&dotfiles).unwrap();
        ensure_codex_trust(&home, std::path::Path::new("/two")).unwrap();
        assert!(linked());
        assert!(
            std::fs::read_to_string(&dotfiles)
                .unwrap()
                .contains("[projects.\"/two\"]")
        );

        // A chain of links to a missing file keeps every link and makes the file at its end.
        let middle = root.join("dotfiles/middle.toml");
        std::fs::remove_file(&config).unwrap();
        std::fs::remove_file(&dotfiles).unwrap();
        std::os::unix::fs::symlink(&dotfiles, &middle).unwrap();
        std::os::unix::fs::symlink(&middle, &config).unwrap();
        ensure_codex_trust(&home, std::path::Path::new("/three")).unwrap();
        assert!(linked());
        assert!(
            std::fs::symlink_metadata(&middle)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            std::fs::read_to_string(&dotfiles)
                .unwrap()
                .contains("[projects.\"/three\"]")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// An empty settings file may be one that another program is writing now, so it is refused,
    /// as before; only a missing file counts as `{}`.
    #[test]
    fn an_empty_json_file_is_refused_and_a_missing_one_is_made() {
        let root = std::env::temp_dir().join(format!("swarm-empty-json-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (empty, missing) = (root.join("empty.json"), root.join("missing.json"));
        std::fs::write(&empty, "").unwrap();
        assert!(ensure_agy_trust(&empty, std::path::Path::new("/x")).is_err());
        assert!(agy_hook_plan(&empty, false).is_err());
        assert_eq!(std::fs::read_to_string(&empty).unwrap(), "");
        assert!(ensure_agy_trust(&missing, std::path::Path::new("/x")).unwrap());
        assert!(set_up(agy_hook_plan(&root.join("hooks.json"), false)).unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_read_only_file_is_refused_and_stays_as_it_was() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-read-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (config, hooks) = (root.join("config.toml"), root.join("hooks.json"));
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        for (file, text) in [(&config, "model = \"o4\"\n"), (&hooks, "{}\n")] {
            std::fs::write(file, text).unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o444)).unwrap();
        }
        assert!(ensure_codex_trust(&root, std::path::Path::new("/one")).is_err());
        assert!(codex_hook_plan(&root, &entries, &[]).is_err());
        assert!(agy_hook_plan(&hooks, false).is_err());
        assert!(ensure_agy_trust(&hooks, std::path::Path::new("/one")).is_err());
        for (file, text) in [(&config, "model = \"o4\"\n"), (&hooks, "{}\n")] {
            assert_eq!(std::fs::read_to_string(file).unwrap(), text);
            let mode = std::fs::metadata(file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o444);
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_file_that_changes_after_it_was_read_is_not_written_over() {
        let root = std::env::temp_dir().join(format!("swarm-changed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        std::fs::write(&config, "model = \"o3\"\n").unwrap();
        let plan =
            codex_hook_plan(&root, &codex_hook_trust("'/bin/swarm' hook codex"), &[]).unwrap();
        let owners = "model = \"o3\"\napproval_policy = \"never\"\n";
        std::fs::write(&config, owners).unwrap();
        assert!(apply(std::slice::from_ref(&plan)).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), owners);
        let hooks = root.join("hooks.json");
        let plan = agy_hook_plan(&hooks, false).unwrap();
        std::fs::write(&hooks, "{\"other\": {}}\n").unwrap();
        assert!(apply(std::slice::from_ref(&plan)).is_err());
        assert_eq!(
            std::fs::read_to_string(&hooks).unwrap(),
            "{\"other\": {}}\n"
        );
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            2,
            "no temp file left"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_launch_edit_reads_a_changed_file_again_and_stops_on_any_other_error() {
        let changed = |path: &str| changed_error(std::path::Path::new(path));
        let mut tries = 0;
        let result = retried(|| {
            tries += 1;
            if tries < 3 {
                Err(changed("/c.json"))
            } else {
                Ok(tries)
            }
        });
        assert_eq!(result, Ok(3));
        let mut tries = 0;
        let result: Result<(), String> = retried(|| {
            tries += 1;
            Err("swarm: cannot parse /c.json".into())
        });
        assert_eq!((result.is_err(), tries), (true, 1));
        let mut tries = 0;
        let result: Result<(), String> = retried(|| {
            tries += 1;
            Err(changed("/c.json"))
        });
        assert_eq!((result, tries), (Err(changed("/c.json")), 4));
    }

    #[test]
    fn two_homes_that_link_to_one_config_both_set_up() {
        let root = std::env::temp_dir().join(format!("swarm-twin-homes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (home, spare) = (root.join(".codex"), root.join(".codex-work"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&spare).unwrap();
        std::fs::write(home.join("config.toml"), "model = \"o4\"\n").unwrap();
        std::os::unix::fs::symlink(home.join("config.toml"), spare.join("config.toml")).unwrap();
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        let plans = [
            codex_hook_plan(&home, &entries, &[]).unwrap(),
            codex_hook_plan(&spare, &entries, &[]).unwrap(),
        ];
        // Both plans have one write target, so their edits have one id and one row.
        assert_eq!(plans[0].edits[0].id(), plans[1].edits[0].id());
        assert_eq!(apply(&plans).unwrap().len(), 2);
        assert!(codex_hooks_trusted(&spare, &entries));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_hard_linked_config_keeps_one_file_for_both_names() {
        use std::os::unix::fs::MetadataExt;
        let root = std::env::temp_dir().join(format!("swarm-hard-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (config, dotfile) = (root.join("config.toml"), root.join("dotfiles-config.toml"));
        std::fs::write(&dotfile, "model = \"o4\"\n").unwrap();
        std::fs::hard_link(&dotfile, &config).unwrap();
        ensure_codex_trust(&root, std::path::Path::new("/one")).unwrap();
        assert!(
            set_up(codex_hook_plan(
                &root,
                &codex_hook_trust("'/bin/swarm' hook codex"),
                &[]
            ))
            .unwrap()
        );
        let (a, b) = (
            std::fs::metadata(&config).unwrap(),
            std::fs::metadata(&dotfile).unwrap(),
        );
        assert_eq!((a.ino(), a.nlink()), (b.ino(), 2));
        assert!(std::fs::read_to_string(&dotfile).unwrap().contains("/one"));
        // A JSON file is still replaced in one rename, as at the base, because a running CLI may
        // read `~/.claude.json` at any moment.
        let (claude, copy) = (root.join(".claude.json"), root.join("claude-copy.json"));
        std::fs::write(&copy, "{}\n").unwrap();
        std::fs::hard_link(&copy, &claude).unwrap();
        assert!(ensure_claude_trust(&claude, std::path::Path::new("/one")).unwrap());
        assert_eq!(std::fs::metadata(&claude).unwrap().nlink(), 1);
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "{}\n");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_locked_file_with_write_bits_is_refused_in_the_plan() {
        let root = std::env::temp_dir().join(format!("swarm-locked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (config, hooks) = (root.join("config.toml"), root.join("hooks.json"));
        std::fs::write(&config, "model = \"o4\"\n").unwrap();
        std::fs::write(&hooks, "{}\n").unwrap();
        let chflags = |flag: &str| {
            let status = std::process::Command::new("chflags")
                .args([flag])
                .args([&config, &hooks])
                .status()
                .unwrap();
            assert!(status.success());
        };
        // Finder's Locked box: the mode stays 0644, but no one can write the file.
        chflags("uchg");
        let (plan, agy) = (
            codex_hook_plan(&root, &codex_hook_trust("'/bin/swarm' hook codex"), &[]),
            agy_hook_plan(&hooks, false),
        );
        chflags("nouchg");
        assert!(plan.is_err());
        assert!(agy.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_config_in_a_read_only_folder_is_refused_in_the_plan() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-read-only-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("config.toml"), "model = \"o4\"\n").unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        let (plan, trust) = (
            codex_hook_plan(&root, &entries, &[]),
            ensure_codex_trust(&root, std::path::Path::new("/one")),
        );
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(plan.is_err());
        assert!(trust.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_link_into_a_missing_folder_makes_no_folder() {
        let root = std::env::temp_dir().join(format!("swarm-link-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let gone = root.join("dotfiles");
        std::os::unix::fs::symlink(gone.join("codex/config.toml"), root.join("config.toml"))
            .unwrap();
        std::os::unix::fs::symlink(gone.join("agy/hooks.json"), root.join("hooks.json")).unwrap();
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        assert!(ensure_codex_trust(&root, std::path::Path::new("/one")).is_err());
        assert!(codex_hook_plan(&root, &entries, &[]).is_err());
        assert!(agy_hook_plan(&root.join("hooks.json"), false).is_err());
        assert!(!gone.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn only_the_orchestrator_launches_and_fable_stays_on_review_and_council() {
        assert!(launch_refusal(None, None).is_none());
        assert!(launch_refusal(Some("orchestrator"), None).is_none());
        assert!(launch_refusal(Some("coder"), None).is_some());
        assert!(launch_refusal(None, Some("1")).is_some());

        assert!(fable_refusal("coder", "code.complex", Some("claude-fable-5-1")).is_some());
        assert!(fable_refusal("seat", "council.claude", Some("claude-fable-5-1")).is_none());
        assert!(fable_refusal("seat", "review.pr", Some("Claude-FABLE")).is_none());
        assert!(fable_refusal("orchestrator", "code.complex", Some("claude-fable-5-1")).is_none());
        assert!(fable_refusal("coder", "code.complex", Some("claude-opus-5-5")).is_none());
    }

    #[test]
    fn a_denied_herdr_socket_refuses_launch() {
        let denied =
            r#"Error: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }"#;
        assert!(socket_refusal(denied).is_some());
        assert!(socket_refusal("").is_none());
    }

    #[test]
    fn extra_args_keep_role_flags_out_and_agy_interactive() {
        assert!(extra_args(Provider::Claude, &strings(&["--model=opus"])).is_err());
        assert!(extra_args(Provider::Codex, &strings(&["-s", "danger-full-access"])).is_err());
        assert_eq!(
            extra_args(Provider::Claude, &strings(&["--", "--model"])).unwrap(),
            strings(&["--", "--model"])
        );
        assert_eq!(
            extra_args(Provider::Agy, &strings(&["fix the bug"])).unwrap(),
            strings(&["-i", "fix the bug"])
        );
        assert_eq!(
            extra_args(Provider::Agy, &strings(&["-i", "fix"])).unwrap(),
            strings(&["-i", "fix"])
        );
    }

    #[test]
    fn a_claude_child_runs_from_the_pool_with_a_reserved_session_id() {
        let cwd = std::path::Path::new("/repo");
        let id = uuid::Uuid::now_v7();
        let (dir, args) = claude_child("coder", cwd, &strings(&["--verbose"]), &id);
        assert_eq!(dir, std::path::Path::new("/repo/.herdr/workers"));
        assert_eq!(args[0], "--session-id");
        assert!(args[1].starts_with("aaaaaaaa-") && uuid::Uuid::parse_str(&args[1]).is_ok());
        assert_eq!(
            args[2..],
            strings(&["-n", "coder", "--verbose", "--add-dir", "/repo"])
        );

        let (_, named) = claude_child("coder", cwd, &strings(&["--name", "x"]), &id);
        assert!(!named.contains(&"-n".to_string()));
        let (dir, resumed) = claude_child("coder", cwd, &strings(&["--resume", "abc"]), &id);
        assert_eq!(
            (dir.as_path(), resumed),
            (cwd, strings(&["--resume", "abc"]))
        );
    }

    #[test]
    fn trust_writes_keep_other_keys_order_and_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-claude-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join(".claude.json");
        std::fs::write(
            &config,
            r#"{"zeta":1,"projects":{"/other":{"allowedTools":[]}},"alpha":2}"#,
        )
        .unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();

        assert!(
            ensure_claude_trust(&config, std::path::Path::new("/repo/.herdr/workers")).unwrap()
        );
        assert!(
            !ensure_claude_trust(&config, std::path::Path::new("/repo/.herdr/workers")).unwrap()
        );
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(
            value.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["zeta", "projects", "alpha"]
        );
        assert_eq!(
            value["projects"]["/repo/.herdr/workers"]["hasTrustDialogAccepted"],
            true
        );
        assert_eq!(
            value["projects"]["/other"]["allowedTools"],
            serde_json::json!([])
        );
        assert_eq!(
            std::fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let settings = root.join("agy/settings.json");
        assert!(ensure_agy_trust(&settings, std::path::Path::new("/repo")).unwrap());
        assert!(!ensure_agy_trust(&settings, std::path::Path::new("/repo")).unwrap());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(value["trustedWorkspaces"], serde_json::json!(["/repo"]));

        std::fs::write(&config, "[]").unwrap();
        assert!(ensure_claude_trust(&config, std::path::Path::new("/repo")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trust_reaches_only_a_git_root_or_a_scratch_dir_that_is_closed_to_others() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("swarm-trust-scope-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (home, repo, scratch) = (
            base.join("home"),
            base.join("home/repo"),
            base.join("scratch"),
        );
        let (seat, open) = (scratch.join("run/seat"), scratch.join("open"));
        for dir in [&repo.join("sub"), &seat, &open] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for dir in [&base, &home, &repo, &scratch, &scratch.join("run"), &seat] {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let roots = [scratch.clone()];

        assert_eq!(
            trust_target(&repo.join("sub"), Some(&repo), &home, &roots),
            Ok(repo.clone())
        );
        assert_eq!(trust_target(&seat, None, &home, &roots), Ok(seat.clone()));
        assert!(trust_target(&home, None, &home, &roots).is_err());
        assert!(trust_target(&home, Some(&home), &home, &roots).is_err());
        assert!(trust_target(&scratch, None, &home, &roots).is_err());
        assert!(trust_target(&open, None, &home, &roots).is_err());
        std::fs::set_permissions(repo.join("sub"), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(trust_target(&repo.join("sub"), Some(&repo), &home, &roots).is_err());
        std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert!(trust_target(&seat, None, &home, &roots).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn locked_trust_writes_keep_every_entry_when_launches_race() {
        let root = std::env::temp_dir().join(format!("swarm-trust-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (settings, lock) = (root.join("settings.json"), root.join("trust.lock"));
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let (settings, lock) = (settings.clone(), lock.clone());
                std::thread::spawn(move || {
                    let dir = std::path::PathBuf::from(format!("/seat-{index}"));
                    with_lock(&lock, || ensure_agy_trust(&settings, &dir)).unwrap();
                })
            })
            .collect();
        threads
            .into_iter()
            .for_each(|thread| thread.join().unwrap());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(value["trustedWorkspaces"].as_array().unwrap().len(), 8);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_required_fields_and_unsupported_provider() {
        let mut missing = role("claude");
        missing.provider = None;
        assert_eq!(
            argv("reviewer", "reviewer", &missing, "/home").unwrap_err(),
            "swarm: role reviewer has no provider"
        );
        missing.provider = Some("claude".into());
        missing.model = None;
        assert_eq!(
            argv("reviewer", "reviewer", &missing, "/home").unwrap_err(),
            "swarm: role reviewer has no model"
        );
        missing.model = Some("model".into());
        missing.effort = None;
        assert_eq!(
            argv("reviewer", "reviewer", &missing, "/home").unwrap_err(),
            "swarm: role reviewer has no effort"
        );
        let unsupported = role("other");
        assert_eq!(
            argv("reviewer", "reviewer", &unsupported, "/home").unwrap_err(),
            "swarm: role reviewer uses unsupported provider other"
        );
    }

    #[test]
    fn validates_agent_ids() {
        assert!(valid_agent_id("coder-1"));
        assert!(!valid_agent_id("Coder-1"));
        assert!(!valid_agent_id("-coder"));
        assert!(!valid_agent_id(&"a".repeat(41)));
    }

    #[test]
    fn finds_the_command_model_and_checks_it_against_the_catalog() {
        let command = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(
            command_model(&command(&[
                "claude", "--model", "opus[1m]", "--effort", "high"
            ])),
            Some("opus")
        );
        assert_eq!(
            command_model(&command(&["codex", "-m", "gpt-6-sol"])),
            Some("gpt-6-sol")
        );
        assert_eq!(
            command_model(&command(&["codex", "--model=gpt-6-luna"])),
            Some("gpt-6-luna")
        );
        assert_eq!(
            command_model(&command(&["agy", "--", "--model", "x"])),
            None
        );
    }

    #[test]
    fn the_guard_plan_keeps_an_owner_handler_and_refuses_a_bad_shape() {
        let root = std::env::temp_dir().join(format!("swarm-guard-plan-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        // The owner's own timeout for swarm's command stays; nothing is added beside it.
        let owned = r#"{"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "swarm guard claude PreToolUse", "timeout": 30}]}]}}"#;
        std::fs::write(&path, owned).unwrap();
        let plan = guard_hooks_plan(&path, "claude").unwrap();
        assert_eq!((plan.after == plan.before, plan.conflicts.len()), (true, 0));
        // A handler the CLI would time out before the runner answers is a conflict with its fix.
        for short in [r#", "timeout": 3"#, ""] {
            let text = format!(
                r#"{{"hooks": {{"PreToolUse": [{{"hooks": [{{"type": "command", "command": "swarm guard claude PreToolUse"{short}}}]}}]}}}}"#
            );
            std::fs::write(&path, text).unwrap();
            let plan = guard_hooks_plan(&path, "claude").unwrap();
            assert_eq!(plan.conflicts.len(), 1, "{short}");
            assert!(
                plan.conflicts[0]
                    .fix
                    .starts_with("set its timeout to 10 or more")
            );
        }
        for bad in [r#"{"hooks": []}"#, r#"{"hooks": {"PreToolUse": {}}}"#] {
            std::fs::write(&path, bad).unwrap();
            assert!(guard_hooks_plan(&path, "claude").is_err(), "{bad}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_guard_trust_follows_the_handler_as_the_file_holds_it() {
        // `codex app-server` 0.159.0 `hooks/list` on 2026-10-05 gave this key and hash for this
        // hooks.json: an owner group with a matcher, the guard second, with its own timeout.
        let hooks = r#"{"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": "echo other", "timeout": 3},
            {"type": "command", "command": "swarm guard codex PreToolUse", "timeout": 30}]}]}}"#;
        let home = std::path::Path::new("/h/.codex");
        assert_eq!(
            codex_guard_trust(home, hooks),
            [(
                "/h/.codex/hooks.json:pre_tool_use:0:1".to_string(),
                "sha256:3bca97397bb529c41561a72882fa330e8a68ca7d4fdee2101617c019f85d6faf"
                    .to_string()
            )]
        );
        let no_timeout = r#"{"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "swarm guard codex PreToolUse"}]}]}}"#;
        assert!(codex_guard_trust(home, no_timeout).is_empty());
    }
}
