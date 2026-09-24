use std::os::unix::fs::OpenOptionsExt;

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

pub fn argv(agent_id: &str, role: &str, resolved: &ResolvedRole, swarm_home: &str) -> Result<Vec<String>, String> {
    let provider = required(role, "provider", resolved.provider.as_deref())?;
    let effort = || required(role, "effort", resolved.effort.as_deref());
    let model = || required(role, "model", resolved.model.as_deref());
    match provider {
        "claude" => {
            let mut args = vec![
                "claude".into(),
                "--model".into(),
                model()?.into(),
                "--effort".into(),
                effort()?.into(),
            ];
            if let Some(permission) = &resolved.permission {
                args.extend(["--permission-mode".into(), permission.clone()]);
            }
            if agent_id == "orchestrator" {
                let command = chair_hook_command("claude")?;
                args.extend([
                    "--settings".into(),
                    serde_json::json!({"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": command, "timeout": 3}]}]}}).to_string(),
                ]);
            }
            Ok(args)
        }
        "codex" => {
            let mut args = vec![
                "codex".into(),
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
                    let root = format!("{swarm_home}/.swarm");
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
            Ok(args)
        }
        "agy" => {
            let mut args = vec!["agy".into()];
            if let Some(model) = resolved
                .model
                .as_deref()
                .filter(|model| *model != "default")
            {
                args.extend(["--model".into(), model.into()]);
            }
            args.extend(["--effort".into(), effort()?.into()]);
            if let Some(permission) = &resolved.permission {
                if permission == "skip" {
                    args.push("--dangerously-skip-permissions".into());
                } else {
                    args.extend(["--mode".into(), permission.clone()]);
                }
            }
            Ok(args)
        }
        provider => Err(format!(
            "swarm: role {role} uses unsupported provider {provider}"
        )),
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

/// `catalog` is the Claude binary itself, where every model id and alias sits as a quoted
/// string, or the JSON of `codex debug models`.
pub fn model_known(provider: &str, catalog: &[u8], model: &str) -> bool {
    if provider == "claude" {
        let quoted = format!("\"{model}\"");
        return catalog.windows(quoted.len()).any(|window| window == quoted.as_bytes());
    }
    serde_json::from_slice::<serde_json::Value>(catalog)
        .ok()
        .and_then(|value| value["models"].as_array().cloned())
        .is_some_and(|models| models.iter().any(|entry| entry["slug"] == model))
}

/// Codex reads folder trust from its config file; its `-c` override does not satisfy the dialog.
/// Keep the existing file byte-identical when the project table is already present.
pub fn ensure_codex_trust(home: &std::path::Path, cwd: &std::path::Path) -> Result<(), String> {
    let path = home.join("config.toml");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let table = format!(
        "[projects.{}]",
        serde_json::to_string(&cwd.to_string_lossy()).expect("string serialization cannot fail")
    );
    if existing.lines().any(|line| line.trim() == table) {
        return Ok(());
    }
    let mut addition = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        addition.push('\n');
    }
    addition.push_str(&format!("{table}\ntrust_level = \"trusted\"\n"));
    std::fs::create_dir_all(home)
        .map_err(|error| format!("cannot create Codex home: {error}"))?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, addition.as_bytes()))
        .map_err(|error| format!("cannot update Codex config: {error}"))
}

/// Why a caller may not launch an agent, or None. A pane `swarm spawn` made carries its own
/// agent id, and only the orchestrator starts children; Herdr marks its own agent panes too.
pub fn launch_refusal(swarm_agent: Option<&str>, herdr_agent_pane: Option<&str>) -> Option<&'static str> {
    let child = swarm_agent.is_some_and(|agent| agent != "orchestrator") || herdr_agent_pane == Some("1");
    child.then_some("swarm: a child agent cannot launch agents; ask the orchestrator")
}

/// Fable runs as a child only for review and council seats. The router refuses it too; this is
/// the second, independent check, so a config edited past the router still cannot reach a pane.
pub fn fable_refusal(agent_id: &str, role: &str, model: Option<&str>) -> Option<String> {
    let fable = model.is_some_and(|model| model.to_ascii_lowercase().contains("fable"));
    (fable && agent_id != "orchestrator" && !role.starts_with("review.") && !role.starts_with("council."))
        .then(|| format!("swarm: Fable is a child only for review.* and council.* (role {role})"))
}

/// Provider flags the role owns; a caller's extra args may not set them.
fn owned_flags(provider: &str) -> &'static [&'static str] {
    match provider {
        "codex" => &["-m", "--model", "-s", "--sandbox", "-a", "--ask-for-approval"],
        _ => &["--model", "--effort", "--permission-mode", "--mode", "--dangerously-skip-permissions", "--yolo"],
    }
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    // Everything past a bare `--` is a positional prompt, not a flag.
    args.iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| flags.contains(&arg.split('=').next().unwrap_or(arg)))
}

/// The caller's extra args for `provider`, checked and in the provider's shape.
pub fn extra_args(provider: &str, extra: &[String]) -> Result<Vec<String>, String> {
    if has_flag(extra, owned_flags(provider)) {
        return Err(format!(
            "swarm: the role owns {provider} model, effort, sandbox, and approval flags; remove them from the extra args"
        ));
    }
    // AGY reads a leading positional as a one-shot prompt; `-i` keeps the pane interactive.
    if provider == "agy" && extra.first().is_some_and(|arg| !arg.starts_with('-')) {
        return Ok([vec!["-i".to_string()], extra.to_vec()].concat());
    }
    Ok(extra.to_vec())
}

/// Where a Claude child runs and the args that go with it. Claude files a transcript under its
/// cwd and has no way to move it, so a child runs from a pool dir under the caller's tree to stay
/// out of the caller's /resume picker; `--add-dir` gives it the tree back. Its session id takes a
/// reserved prefix so tools can tell a worker transcript apart. A resuming child keeps its cwd,
/// because its transcript already lives under the original one.
pub fn claude_child(agent_id: &str, cwd: &std::path::Path, extra: &[String], session_id: &uuid::Uuid) -> (std::path::PathBuf, Vec<String>) {
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
/// scratch roots swarm and the council write. $HOME and `/` are too broad, and every checked dir
/// must belong to the user and be closed to group and world writes, so another account cannot
/// plant files in a place the agents then trust.
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
                .ok_or_else(|| format!("{} is not in a git repository or a scratch dir", cwd.display()))?;
            (cwd.to_path_buf(), root.clone())
        }
    };
    if target == home || target.parent().is_none() {
        return Err(format!("{} is too broad to trust", target.display()));
    }
    let uid = std::fs::metadata(home).map_err(|error| format!("{}: {error}", home.display()))?.uid();
    let mut dir = target.as_path();
    loop {
        let meta = std::fs::metadata(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        if meta.uid() != uid || meta.mode() & 0o022 != 0 {
            return Err(format!("{} must be yours and closed to group and world writes", dir.display()));
        }
        if dir == top {
            return Ok(target);
        }
        dir = dir.parent().ok_or_else(|| format!("{} left its root", target.display()))?;
    }
}

/// Run `change` while holding `lock`, so two launches cannot both read a settings file and the
/// second write drop the first one's trust entry.
pub fn with_lock<T>(lock: &std::path::Path, change: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(lock)
        .map_err(|error| format!("swarm: cannot open {}: {error}", lock.display()))?;
    file.lock().map_err(|error| format!("swarm: cannot lock {}: {error}", lock.display()))?;
    change()
}

/// Mark `dir` trusted in Claude's `~/.claude.json`, so a child does not boot into the folder-trust
/// dialog and wait there with nobody to answer. Returns whether the file changed.
pub fn ensure_claude_trust(config: &std::path::Path, dir: &std::path::Path) -> Result<bool, String> {
    let mut value = read_json_object(config)?;
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
    write_json(config, &value).map(|()| true)
}

/// Add `dir` to AGY's `trustedWorkspaces`. Returns whether the file changed.
pub fn ensure_agy_trust(settings: &std::path::Path, dir: &std::path::Path) -> Result<bool, String> {
    let mut value = read_json_object(settings)?;
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
    write_json(settings, &value).map(|()| true)
}

fn read_json_object(path: &std::path::Path) -> Result<serde_json::Value, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(serde_json::json!({})),
        Err(error) => return Err(format!("swarm: cannot read {}: {error}", path.display())),
    };
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("swarm: cannot parse {}: {error}", path.display()))?;
    value
        .is_object()
        .then_some(value)
        .ok_or_else(|| format!("swarm: {} is not a JSON object", path.display()))
}

/// Replace `path` in one rename, with the old file's permissions, because `~/.claude.json` holds
/// credentials and a running CLI may read it at any moment.
fn write_json(path: &std::path::Path, value: &serde_json::Value) -> Result<(), String> {
    let fail = |error: std::io::Error| format!("swarm: cannot write {}: {error}", path.display());
    let dir = path.parent().ok_or_else(|| format!("swarm: {} has no parent", path.display()))?;
    std::fs::create_dir_all(dir).map_err(fail)?;
    let tmp = dir.join(format!(
        ".{}.swarm-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let permissions = std::fs::metadata(path)
        .map(|meta| meta.permissions())
        .unwrap_or_else(|_| std::os::unix::fs::PermissionsExt::from_mode(0o600));
    let text = serde_json::to_string_pretty(value).expect("JSON serialization cannot fail") + "\n";
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
        .and_then(|()| std::fs::set_permissions(&tmp, permissions))
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&tmp);
            fail(error)
        })
}

fn chair_hook_command(provider: &str) -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("swarm: cannot find executable: {error}"))?
        .to_string_lossy()
        .replace('\'', "'\\''");
    // Both CLI SessionStart payloads include a top-level session_id string.
    Ok(format!(
        "id=$(sed -n 's/.*\"session_id\"[[:space:]]*:[[:space:]]*\"\\([A-Za-z0-9-]\\{{1,64\\}}\\)\".*/\\1/p'); [ -n \"$id\" ] && '{exe}' session chair {provider}:\"$id\""
    ))
}

fn required<'a>(role: &str, field: &str, value: Option<&'a str>) -> Result<&'a str, String> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("swarm: role {role} has no {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(codex_args.len(), 13);

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

        let child_claude_args = argv("coder", "coder", &claude, "/home").unwrap();
        assert!(!child_claude_args.iter().any(|arg| arg.contains("SessionStart")));
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
        assert_eq!(std::fs::read_to_string(&config).unwrap().matches("trust_level = \"trusted\"").count(), 2);
        std::fs::remove_dir_all(root).unwrap();
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
    fn extra_args_keep_role_flags_out_and_agy_interactive() {
        assert!(extra_args("claude", &strings(&["--model=opus"])).is_err());
        assert!(extra_args("codex", &strings(&["-s", "danger-full-access"])).is_err());
        assert_eq!(extra_args("claude", &strings(&["--", "--model"])).unwrap(), strings(&["--", "--model"]));
        assert_eq!(extra_args("agy", &strings(&["fix the bug"])).unwrap(), strings(&["-i", "fix the bug"]));
        assert_eq!(extra_args("agy", &strings(&["-i", "fix"])).unwrap(), strings(&["-i", "fix"]));
    }

    #[test]
    fn a_claude_child_runs_from_the_pool_with_a_reserved_session_id() {
        let cwd = std::path::Path::new("/repo");
        let id = uuid::Uuid::now_v7();
        let (dir, args) = claude_child("coder", cwd, &strings(&["--verbose"]), &id);
        assert_eq!(dir, std::path::Path::new("/repo/.herdr/workers"));
        assert_eq!(args[0], "--session-id");
        assert!(args[1].starts_with("aaaaaaaa-") && uuid::Uuid::parse_str(&args[1]).is_ok());
        assert_eq!(args[2..], strings(&["-n", "coder", "--verbose", "--add-dir", "/repo"]));

        let (_, named) = claude_child("coder", cwd, &strings(&["--name", "x"]), &id);
        assert!(!named.contains(&"-n".to_string()));
        let (dir, resumed) = claude_child("coder", cwd, &strings(&["--resume", "abc"]), &id);
        assert_eq!((dir.as_path(), resumed), (cwd, strings(&["--resume", "abc"])));
    }

    #[test]
    fn trust_writes_keep_other_keys_order_and_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-claude-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join(".claude.json");
        std::fs::write(&config, r#"{"zeta":1,"projects":{"/other":{"allowedTools":[]}},"alpha":2}"#).unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();

        assert!(ensure_claude_trust(&config, std::path::Path::new("/repo/.herdr/workers")).unwrap());
        assert!(!ensure_claude_trust(&config, std::path::Path::new("/repo/.herdr/workers")).unwrap());
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(value.as_object().unwrap().keys().collect::<Vec<_>>(), ["zeta", "projects", "alpha"]);
        assert_eq!(value["projects"]["/repo/.herdr/workers"]["hasTrustDialogAccepted"], true);
        assert_eq!(value["projects"]["/other"]["allowedTools"], serde_json::json!([]));
        assert_eq!(std::fs::metadata(&config).unwrap().permissions().mode() & 0o777, 0o600);

        let settings = root.join("agy/settings.json");
        assert!(ensure_agy_trust(&settings, std::path::Path::new("/repo")).unwrap());
        assert!(!ensure_agy_trust(&settings, std::path::Path::new("/repo")).unwrap());
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(value["trustedWorkspaces"], serde_json::json!(["/repo"]));

        std::fs::write(&config, "[]").unwrap();
        assert!(ensure_claude_trust(&config, std::path::Path::new("/repo")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trust_reaches_only_a_git_root_or_a_scratch_dir_that_is_closed_to_others() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("swarm-trust-scope-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (home, repo, scratch) = (base.join("home"), base.join("home/repo"), base.join("scratch"));
        let (seat, open) = (scratch.join("run/seat"), scratch.join("open"));
        for dir in [&repo.join("sub"), &seat, &open] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for dir in [&base, &home, &repo, &scratch, &scratch.join("run"), &seat] {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let roots = [scratch.clone()];

        assert_eq!(trust_target(&repo.join("sub"), Some(&repo), &home, &roots), Ok(repo.clone()));
        assert_eq!(trust_target(&seat, None, &home, &roots), Ok(seat.clone()));
        assert!(trust_target(&home, None, &home, &roots).is_err());
        assert!(trust_target(&home, Some(&home), &home, &roots).is_err());
        assert!(trust_target(&scratch, None, &home, &roots).is_err());
        assert!(trust_target(&open, None, &home, &roots).is_err());
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
        threads.into_iter().for_each(|thread| thread.join().unwrap());
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
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
        assert_eq!(command_model(&command(&["claude", "--model", "opus[1m]", "--effort", "high"])), Some("opus"));
        assert_eq!(command_model(&command(&["codex", "-m", "gpt-6-sol"])), Some("gpt-6-sol"));
        assert_eq!(command_model(&command(&["codex", "--model=gpt-6-luna"])), Some("gpt-6-luna"));
        assert_eq!(command_model(&command(&["agy", "--", "--model", "x"])), None);

        let binary = br#"aliases:{opus:{default:"claude-opus-5-5"}},x="opus""#;
        assert!(model_known("claude", binary, "claude-opus-5-5"));
        assert!(model_known("claude", binary, "opus"));
        assert!(!model_known("claude", binary, "claude-opus-5"));

        let codex = br#"{"models":[{"slug":"gpt-6-sol"},{"slug":"gpt-6-luna"}]}"#;
        assert!(model_known("codex", codex, "gpt-6-sol"));
        assert!(!model_known("codex", codex, "gpt-5.6-sol"));
        assert!(!model_known("codex", b"not json", "gpt-6-sol"));
    }
}
