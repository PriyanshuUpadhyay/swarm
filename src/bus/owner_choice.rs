// Reasoning-summary launch settings that yield to an owner's own choice.
use std::path::Path;

/// The Claude `--settings` object. Claude writes an empty thinking block unless it is asked for
/// summaries. A flag outranks the owner's settings files, so an explicit owner choice is read and
/// kept.
pub(super) fn claude_json(
    hooks: serde_json::Map<String, serde_json::Value>,
    swarm_home: &str,
    cwd: &Path,
) -> String {
    let mut settings = serde_json::json!({ "hooks": hooks });
    let home = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map_or_else(|| Path::new(swarm_home).join(".claude"), Into::into);
    if !claude_settings_name(&home, cwd, "showThinkingSummaries") {
        settings["showThinkingSummaries"] = true.into();
    }
    settings.to_string()
}

/// The Codex args that ask for reasoning summaries. `-c` outranks config.toml, so the owner's own
/// choice is read and kept.
pub(super) fn codex_args(swarm_home: &str) -> Vec<String> {
    let home = std::env::var_os("CODEX_HOME")
        .map_or_else(|| Path::new(swarm_home).join(".codex"), Into::into);
    if codex_config_names(&home, "model_reasoning_summary") {
        return Vec::new();
    }
    vec!["-c".into(), "model_reasoning_summary=\"auto\"".into()]
}

/// Whether the owner's Claude settings (user, project, project-local) name `key`. A file that is
/// missing or not JSON names nothing.
pub(super) fn claude_settings_name(config: &Path, cwd: &Path, key: &str) -> bool {
    [
        config.join("settings.json"),
        cwd.join(".claude/settings.json"),
        cwd.join(".claude/settings.local.json"),
    ]
    .iter()
    .filter_map(|path| std::fs::read_to_string(path).ok())
    .filter_map(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .any(|settings| settings.get(key).is_some())
}

/// Whether the owner's Codex `config.toml` sets `key` at its top level or in a table.
pub(super) fn codex_config_names(home: &Path, key: &str) -> bool {
    std::fs::read_to_string(home.join("config.toml")).is_ok_and(|text| {
        text.lines().any(|line| {
            line.trim_start()
                .strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
    })
}
