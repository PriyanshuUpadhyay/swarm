// Reasoning-summary launch settings that yield to an owner's own choice.
use std::path::{Path, PathBuf};

const CLAUDE_KEY: &str = "showThinkingSummaries";
const CODEX_KEY: &str = "model_reasoning_summary";

/// The owner's HOME. A launched pane runs with `CLAUDE_CONFIG_DIR` and `CODEX_HOME` removed, so
/// its CLI reads `$HOME/.claude` and `$HOME/.codex`. The launcher's own values do not count, and
/// `swarm_home` is the build's data root, not the owner's HOME.
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// The Claude `--settings` object. Claude writes an empty thinking block unless it is asked for
/// summaries. A flag outranks the owner's settings files, so an explicit owner choice is read and
/// kept.
pub(super) fn claude_json(hooks: serde_json::Map<String, serde_json::Value>, cwd: &Path) -> String {
    let mut settings = serde_json::json!({ "hooks": hooks });
    if !claude_settings_name(&home().join(".claude"), cwd, CLAUDE_KEY) {
        settings[CLAUDE_KEY] = true.into();
    }
    settings.to_string()
}

/// The Codex args that ask for reasoning summaries. `-c` outranks config.toml, so the owner's own
/// choice is read and kept. A picked account's home is not read here, because it is not known yet.
pub(super) fn codex_args() -> Vec<String> {
    if codex_config_names(&home().join(".codex"), CODEX_KEY) {
        return Vec::new();
    }
    vec!["-c".into(), format!("{CODEX_KEY}=\"auto\"")]
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

/// Whether the owner's Codex `config.toml` sets `key` at its top level or in the table of the
/// profile that its top-level `profile` names. A key in another profile or table does not count.
pub(super) fn codex_config_names(home: &Path, key: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(home.join("config.toml")) else {
        return false;
    };
    let Ok(config) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    let active = config
        .get("profile")
        .and_then(toml_edit::Item::as_str)
        .and_then(|name| config.get("profiles")?.get(name));
    config.get(key).is_some() || active.is_some_and(|profile| profile.get(key).is_some())
}
