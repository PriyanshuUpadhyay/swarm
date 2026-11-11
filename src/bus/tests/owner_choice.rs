// Reasoning-summary launch settings yield to an owner's choice.
use super::super::owner_choice::{CLAUDE_KEY, CODEX_KEY, claude_settings_name, codex_config_names};

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("swarm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn a_claude_settings_key_the_owner_set_is_kept() {
    let root = scratch("claude-summary");
    assert!(!claude_settings_name(Some(&root), &root, CLAUDE_KEY));

    // Even `false` is a choice.
    let file = root.join("settings.json");
    std::fs::write(file, format!(r#"{{"{CLAUDE_KEY}":false}}"#)).unwrap();
    assert!(claude_settings_name(Some(&root), &root, CLAUDE_KEY));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_codex_key_counts_at_top_level_or_in_the_active_profile_only() {
    let root = scratch("codex-summary");
    let key = CODEX_KEY;
    let write = |text: &str| std::fs::write(root.join("config.toml"), text).unwrap();
    assert!(!codex_config_names(&root, key));

    write(&format!("{CODEX_KEY} = \"none\"\n"));
    assert!(codex_config_names(&root, key));

    // A key in a profile that is not active, or in another table, is not the owner's choice.
    write(&format!(
        "[profiles.fast]\n{CODEX_KEY} = \"none\"\n[mcp_servers.x]\n{CODEX_KEY} = \"none\"\n"
    ));
    assert!(!codex_config_names(&root, key));

    write(&format!(
        "profile = \"fast\"\n[profiles.fast]\n{CODEX_KEY} = \"none\"\n"
    ));
    assert!(codex_config_names(&root, key));

    std::fs::remove_dir_all(&root).unwrap();
}
