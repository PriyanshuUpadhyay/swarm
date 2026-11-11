// Reasoning-summary launch settings that yield to an owner's own choice.
use std::path::{Path, PathBuf};

pub(super) const CLAUDE_KEY: &str = "showThinkingSummaries";
pub(super) const CODEX_KEY: &str = "model_reasoning_summary";

/// The owner's home. `std::env::home_dir` falls back to the passwd home when HOME is unset, and
/// gives None only when that lookup also fails; then nothing is read. A launched pane runs with
/// `CLAUDE_CONFIG_DIR` and `CODEX_HOME` removed, so its CLI reads `$HOME/.claude` and
/// `$HOME/.codex`; the launcher's own values do not count. A picked Codex account sets `CODEX_HOME`
/// to its own home, and that home's config is not read here, because `argv` runs before the pick.
fn home() -> Option<PathBuf> {
    std::env::home_dir()
}

/// The Claude `--settings` object. Claude writes an empty thinking block unless it is asked for
/// summaries. A flag outranks the owner's settings files, so an explicit owner choice is read and
/// kept.
pub(super) fn claude_json(hooks: serde_json::Map<String, serde_json::Value>, cwd: &Path) -> String {
    let mut settings = serde_json::json!({ "hooks": hooks });
    let owner = home().map(|home| home.join(".claude"));
    if !claude_settings_name(owner.as_deref(), cwd, CLAUDE_KEY) {
        settings[CLAUDE_KEY] = true.into();
    }
    settings.to_string()
}

/// The Codex args that ask for reasoning summaries. `-c` outranks config.toml, so the owner's own
/// choice is read and kept. A picked account's home is not read here, because it is not known yet.
pub(super) fn codex_args() -> Vec<String> {
    let owner = home().map(|home| home.join(".codex"));
    if owner.is_some_and(|owner| codex_config_names(&owner, CODEX_KEY)) {
        return Vec::new();
    }
    vec!["-c".into(), format!("{CODEX_KEY}=\"auto\"")]
}

/// Whether the owner's Claude settings (user, project, project-local) name `key`. A file that is
/// missing or not JSON names nothing.
pub(super) fn claude_settings_name(config: Option<&Path>, cwd: &Path, key: &str) -> bool {
    [
        config.map(|config| config.join("settings.json")),
        Some(cwd.join(".claude/settings.json")),
        Some(cwd.join(".claude/settings.local.json")),
    ]
    .iter()
    .flatten()
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
