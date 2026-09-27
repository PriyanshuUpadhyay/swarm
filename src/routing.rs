//! Role routing: which runner (provider, model, effort, sandbox) starts each role. The config is
//! user data at `$AGENT_ROUTING_CONFIG`, else `$XDG_CONFIG_HOME/agent-routing/roles.json`.

use serde_json::Value;
use std::path::PathBuf;

pub fn config_path() -> Result<PathBuf, String> {
    // An explicit path that is missing is an error, never a fall-through to the default file.
    if let Ok(path) = std::env::var("AGENT_ROUTING_CONFIG") {
        let path = PathBuf::from(path);
        return match path.exists() {
            true => Ok(path),
            false => Err(format!("AGENT_ROUTING_CONFIG names a missing file: {}", path.display())),
        };
    }
    let config_home = match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(std::env::var("HOME").map_err(|_| "HOME not set")?).join(".config"),
    };
    Ok(config_home.join("agent-routing/roles.json"))
}

pub fn load() -> Result<(PathBuf, Value), String> {
    let path = config_path()?;
    let text = std::fs::read_to_string(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let config = serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((path, config))
}

/// The route's first runner, or its first runner of `provider`, with its fields plus `role`,
/// `runnerId`, and `fallbackRunnerIds`: the route's other runners of the same provider, in order.
pub fn resolve(config: &Value, role: &str, provider: Option<&str>) -> Result<Value, String> {
    let ids = config["routes"][role]
        .as_array()
        .filter(|ids| !ids.is_empty())
        .ok_or_else(|| format!("route '{role}' is not defined"))?;
    let provider_of = |id: &Value| config["runners"][id.as_str().unwrap_or_default()]["provider"].as_str();
    let id = match provider {
        Some(provider) => ids
            .iter()
            .find(|id| provider_of(id) == Some(provider))
            .ok_or_else(|| format!("route '{role}' has no {provider} runner"))?,
        None => &ids[0],
    };
    let Some(Value::Object(runner)) = config["runners"].get(id.as_str().unwrap_or_default()) else {
        return Err(format!("runner '{}' for route '{role}' is not defined", id.as_str().unwrap_or_default()));
    };
    let fallbacks = ids
        .iter()
        .filter(|other| *other != id && provider_of(other) == runner.get("provider").and_then(Value::as_str))
        .cloned()
        .collect();
    let mut resolved = runner.clone();
    resolved.insert("role".into(), role.into());
    resolved.insert("runnerId".into(), id.clone());
    resolved.insert("fallbackRunnerIds".into(), Value::Array(fallbacks));
    Ok(Value::Object(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Value {
        serde_json::json!({
            "routes": {"code": ["codex-sol-high-agent", "claude-opus-high-agent", "codex-luna-low-agent"]},
            "runners": {
                "codex-sol-high-agent": {"provider": "codex", "model": "gpt-sol", "effort": "high"},
                "claude-opus-high-agent": {"provider": "claude", "model": "opus"},
                "codex-luna-low-agent": {"provider": "codex", "model": "gpt-luna"}
            }
        })
    }

    #[test]
    fn resolves_the_first_runner_with_same_provider_fallbacks() {
        let resolved = resolve(&config(), "code", None).unwrap();
        assert_eq!(resolved.to_string(), r#"{"provider":"codex","model":"gpt-sol","effort":"high","role":"code","runnerId":"codex-sol-high-agent","fallbackRunnerIds":["codex-luna-low-agent"]}"#);
    }

    #[test]
    fn a_provider_picks_its_first_runner_and_a_missing_one_errors() {
        assert_eq!(resolve(&config(), "code", Some("claude")).unwrap()["runnerId"], "claude-opus-high-agent");
        assert_eq!(resolve(&config(), "code", Some("agy")).unwrap_err(), "route 'code' has no agy runner");
        assert_eq!(resolve(&config(), "chat", None).unwrap_err(), "route 'chat' is not defined");
    }
}
