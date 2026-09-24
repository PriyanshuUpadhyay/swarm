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
