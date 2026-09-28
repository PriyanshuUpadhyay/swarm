//! Role routing: which runner (provider, model, effort, sandbox) starts each role. The config is
//! user data at `$AGENT_ROUTING_CONFIG`, else `$XDG_CONFIG_HOME/agent-routing/roles.json`, else
//! the shipped `default-roles.json`.

use serde_json::Value;
use std::path::PathBuf;

pub fn config_path() -> Result<PathBuf, String> {
    // An explicit path that is missing is an error, never a fall-through to the default file.
    if let Ok(path) = std::env::var("AGENT_ROUTING_CONFIG") {
        let path = PathBuf::from(path);
        return match path.exists() {
            true => Ok(path),
            false => Err(format!(
                "AGENT_ROUTING_CONFIG names a missing file: {}",
                path.display()
            )),
        };
    }
    let config_home = match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(std::env::var("HOME").map_err(|_| "HOME not set")?).join(".config"),
    };
    Ok(config_home.join("agent-routing/roles.json"))
}

const DEFAULT: &str = include_str!("../default-roles.json");

pub fn load() -> Result<(PathBuf, Value), String> {
    let path = config_path()?;
    // No user file means the default; a broken symlink is an error, so a moved dotfiles checkout
    // cannot switch every role to the default without a word.
    let text = match std::fs::read_to_string(&path) {
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && std::fs::symlink_metadata(&path).is_err() =>
        {
            DEFAULT.into()
        }
        result => result.map_err(|error| format!("cannot read {}: {error}", path.display()))?,
    };
    let config =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((path, config))
}

/// Sets one runner's model and saves the file: a timestamped backup beside it, then an atomic
/// rename onto the real file, because the live roles.json is often a symlink that a rename onto
/// the link itself would replace.
pub fn set_model(runner: &str, model: &str) -> Result<(), String> {
    let (path, mut config) = load()?;
    let Some(Value::Object(fields)) = config["runners"].get_mut(runner) else {
        return Err(format!("unknown runner '{runner}'"));
    };
    fields.insert("model".into(), model.into());
    let errors = validate(&config);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let text = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())? + "\n";
    if std::fs::symlink_metadata(&path).is_err() {
        let dir = path.parent().ok_or("config path has no folder")?;
        std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(&path, DEFAULT))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    let target =
        std::fs::canonicalize(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let backup = format!("{}.{stamp}.bak", path.display());
    std::fs::copy(&target, &backup).map_err(|error| format!("{backup}: {error}"))?;
    let temp = target.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temp, text)
        .and_then(|()| std::fs::rename(&temp, &target))
        .map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            format!("{}: {error}", target.display())
        })
}

/// Every rule the config breaks. A runner id reads `<provider>-<tier>-<effort>-<agent|worker>`
/// with no generation number, so a model bump never forces a rename.
pub fn validate(config: &Value) -> Vec<String> {
    let (Some(routes), Some(runners)) =
        (config["routes"].as_object(), config["runners"].as_object())
    else {
        return vec!["routes and runners must be objects.".into()];
    };
    let mut errors = Vec::new();
    if routes.is_empty() {
        errors.push("At least one route is required.".into());
    }
    let mut routed = std::collections::HashSet::new();
    for (route, ids) in routes {
        let Some(ids) = ids.as_array().filter(|ids| !ids.is_empty()) else {
            errors.push(format!("Route '{route}' must have at least one runner."));
            continue;
        };
        if ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() {
            errors.push(format!("Route '{route}' has duplicate runners."));
        }
        for id in ids.iter().map(|id| id.as_str().unwrap_or_default()) {
            let Some(runner) = runners.get(id) else {
                errors.push(format!("Route '{route}' references missing runner '{id}'."));
                continue;
            };
            routed.insert(id);
            // Fable is a child seat only where it judges, never where it writes code.
            let fable = runner["model"]
                .as_str()
                .is_some_and(|model| model.to_lowercase().contains("fable"));
            if fable && !route.starts_with("review.") && !route.starts_with("council.") {
                errors.push(format!("Route '{route}': Fable runner '{id}' is allowed only in a review.* or council.* route."));
            }
        }
    }
    for (id, runner) in runners {
        let Some(fields) = runner.as_object() else {
            errors.push(format!("Runner '{id}' must be an object."));
            continue;
        };
        let parts: Vec<&str> = id.split('-').collect();
        let word =
            |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_lowercase());
        if !matches!(parts[..], [provider, tier, effort, kind] if ["claude", "codex", "agy"].contains(&provider)
            && word(tier) && word(effort) && ["agent", "worker"].contains(&kind))
        {
            errors.push(format!("Runner id '{id}' must read <provider>-<tier>-<effort>-<agent|worker> with no generation number."));
        }
        if fields
            .get("provider")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            errors.push(format!("Runner '{id}' must have a provider."));
        }
        for field in fields
            .keys()
            .filter(|field| !RUNNER_FIELDS.contains(&field.as_str()))
        {
            errors.push(format!("Runner '{id}' has unknown field '{field}'."));
        }
        if let Some(effort) = fields.get("effort").filter(|effort| {
            !effort
                .as_str()
                .is_some_and(|effort| EFFORTS.contains(&effort))
        }) {
            errors.push(format!(
                "Runner '{id}' has invalid effort '{}'.",
                effort.as_str().unwrap_or(&effort.to_string())
            ));
        }
        if !routed.contains(id.as_str()) {
            errors.push(format!("Runner '{id}' is in no route."));
        }
    }
    errors
}

const RUNNER_FIELDS: [&str; 6] = [
    "provider",
    "model",
    "effort",
    "sandbox",
    "approval",
    "permission",
];
const EFFORTS: [&str; 7] = ["none", "low", "medium", "high", "xhigh", "max", "ultra"];

/// The route's first runner, or its first runner of `provider`, with its fields plus `role`,
/// `runnerId`, and `fallbackRunnerIds`: the route's other runners of the same provider, in order.
pub fn resolve(config: &Value, role: &str, provider: Option<&str>) -> Result<Value, String> {
    let ids = config["routes"][role]
        .as_array()
        .filter(|ids| !ids.is_empty())
        .ok_or_else(|| format!("route '{role}' is not defined"))?;
    let provider_of =
        |id: &Value| config["runners"][id.as_str().unwrap_or_default()]["provider"].as_str();
    let id = match provider {
        Some(provider) => ids
            .iter()
            .find(|id| provider_of(id) == Some(provider))
            .ok_or_else(|| format!("route '{role}' has no {provider} runner"))?,
        None => &ids[0],
    };
    let Some(Value::Object(runner)) = config["runners"].get(id.as_str().unwrap_or_default()) else {
        return Err(format!(
            "runner '{}' for route '{role}' is not defined",
            id.as_str().unwrap_or_default()
        ));
    };
    let fallbacks = ids
        .iter()
        .filter(|other| {
            *other != id && provider_of(other) == runner.get("provider").and_then(Value::as_str)
        })
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
        assert_eq!(
            resolved.to_string(),
            r#"{"provider":"codex","model":"gpt-sol","effort":"high","role":"code","runnerId":"codex-sol-high-agent","fallbackRunnerIds":["codex-luna-low-agent"]}"#
        );
    }

    #[test]
    fn validate_accepts_a_good_config_and_names_each_broken_rule() {
        assert_eq!(validate(&config()), Vec::<String>::new());
        assert_eq!(
            validate(&serde_json::from_str(DEFAULT).unwrap()),
            Vec::<String>::new()
        );
        let bad = serde_json::json!({
            "routes": {"code": ["claude-fable-high-agent", "claude-fable-high-agent", "gone"]},
            "runners": {
                "claude-fable-high-agent": {"provider": "claude", "model": "claude-fable-5", "effort": "huge"},
                "codex-sol5-high-agent": {"provider": "codex", "colour": "red"}
            }
        });
        assert_eq!(
            validate(&bad),
            [
                "Route 'code' has duplicate runners.",
                "Route 'code': Fable runner 'claude-fable-high-agent' is allowed only in a review.* or council.* route.",
                "Route 'code': Fable runner 'claude-fable-high-agent' is allowed only in a review.* or council.* route.",
                "Route 'code' references missing runner 'gone'.",
                "Runner 'claude-fable-high-agent' has invalid effort 'huge'.",
                "Runner id 'codex-sol5-high-agent' must read <provider>-<tier>-<effort>-<agent|worker> with no generation number.",
                "Runner 'codex-sol5-high-agent' has unknown field 'colour'.",
                "Runner 'codex-sol5-high-agent' is in no route.",
            ]
        );
    }

    #[test]
    fn a_provider_picks_its_first_runner_and_a_missing_one_errors() {
        assert_eq!(
            resolve(&config(), "code", Some("claude")).unwrap()["runnerId"],
            "claude-opus-high-agent"
        );
        assert_eq!(
            resolve(&config(), "code", Some("agy")).unwrap_err(),
            "route 'code' has no agy runner"
        );
        assert_eq!(
            resolve(&config(), "chat", None).unwrap_err(),
            "route 'chat' is not defined"
        );
    }
}
