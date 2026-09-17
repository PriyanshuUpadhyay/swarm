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
    pub id: i64,
    pub talk_mode: String,
    pub cwd: String,
    pub created_at: i64,
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
            Ok(args)
        }
        "codex" => {
            let mut args = vec![
                "codex".into(),
                "--model".into(),
                model()?.into(),
                "-c".into(),
                format!("model_reasoning_effort=\"{}\"", effort()?),
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
        assert_eq!(
            argv("coder", &claude, "/home"),
            Ok(vec![
                "claude",
                "--model",
                "model-1",
                "--effort",
                "high",
                "--permission-mode",
                "acceptEdits"
            ]
            .into_iter()
            .map(String::from)
            .collect())
        );

        let mut codex = role("codex");
        codex.sandbox = Some("workspace-write".into());
        codex.approval = Some("never".into());
        assert_eq!(
            argv("coder", &codex, "/home x"),
            Ok(vec![
                "codex",
                "--model",
                "model-1",
                "-c",
                "model_reasoning_effort=\"high\"",
                "--sandbox",
                "workspace-write",
                "-c",
                "sandbox_workspace_write.writable_roots=[\"/home x/.swarm\"]",
                "--ask-for-approval",
                "never"
            ]
            .into_iter()
            .map(String::from)
            .collect())
        );

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
        assert_eq!(
            argv("coder", &agy, "/home"),
            Ok(vec!["agy", "--effort", "high"]
                .into_iter()
                .map(String::from)
                .collect())
        );
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
