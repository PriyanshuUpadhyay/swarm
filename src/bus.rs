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

pub fn argv(role: &str, resolved: &ResolvedRole, swarm_home: &str) -> Result<Vec<String>, String> {
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
            let command = chair_hook_command("claude")?;
            args.extend([
                "--settings".into(),
                serde_json::json!({"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": command, "timeout": 3}]}]}}).to_string(),
            ]);
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
            let command = chair_hook_command("codex")?;
            args.extend([
                "--dangerously-bypass-hook-trust".into(),
                "-c".into(),
                format!(
                    "hooks.SessionStart=[{{hooks=[{{type=\"command\",command={},timeout=3}}]}}]",
                    serde_json::to_string(&command).expect("string serialization cannot fail")
                ),
            ]);
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
        let claude_args = argv("coder", &claude, "/home").unwrap();
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
        let codex_args = argv("coder", &codex, "/home x").unwrap();
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
        assert_eq!(
            codex_args[13..15],
            ["--dangerously-bypass-hook-trust", "-c"]
        );
        assert!(codex_args[15].starts_with("hooks.SessionStart="));
        assert!(codex_args[15].contains("session chair codex:"));

        let mut agy = role("agy");
        agy.permission = Some("skip".into());
        assert_eq!(
            argv("coder", &agy, "/home"),
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
            argv("coder", &agy, "/home"),
            Ok(vec!["agy", "--effort", "high", "--mode", "plan"]
                .into_iter()
                .map(String::from)
                .collect())
        );
        agy.model = None;
        agy.permission = None;
        let agy_args = argv("coder", &agy, "/home").unwrap();
        assert_eq!(
            agy_args,
            vec!["agy", "--effort", "high"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert!(!agy_args.iter().any(|arg| arg.contains("SessionStart")));
    }

    #[test]
    fn reports_required_fields_and_unsupported_provider() {
        let mut missing = role("claude");
        missing.provider = None;
        assert_eq!(
            argv("reviewer", &missing, "/home").unwrap_err(),
            "swarm: role reviewer has no provider"
        );
        missing.provider = Some("claude".into());
        missing.model = None;
        assert_eq!(
            argv("reviewer", &missing, "/home").unwrap_err(),
            "swarm: role reviewer has no model"
        );
        missing.model = Some("model".into());
        missing.effort = None;
        assert_eq!(
            argv("reviewer", &missing, "/home").unwrap_err(),
            "swarm: role reviewer has no effort"
        );
        let unsupported = role("other");
        assert_eq!(
            argv("reviewer", &unsupported, "/home").unwrap_err(),
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
}
