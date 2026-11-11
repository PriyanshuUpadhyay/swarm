// Reasoning-summary launch settings yield to an owner's choice.
use super::super::owner_choice::{claude_settings_name, codex_config_names};

#[test]
fn thinking_summary_settings_yield_to_an_owner_choice() {
    let root = std::env::temp_dir().join(format!("swarm-thinking-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // No file names the key, so Swarm turns it on.
    assert!(!claude_settings_name(&root, &root, "showThinkingSummaries"));
    assert!(!codex_config_names(&root, "model_reasoning_summary"));
    // The owner's file names it, even with `false`, so Swarm leaves it.
    std::fs::write(
        root.join("settings.json"),
        r#"{"showThinkingSummaries":false}"#,
    )
    .unwrap();
    assert!(claude_settings_name(&root, &root, "showThinkingSummaries"));
    std::fs::write(
        root.join("config.toml"),
        "model_reasoning_summary = \"none\"\n",
    )
    .unwrap();
    assert!(codex_config_names(&root, "model_reasoning_summary"));
    std::fs::remove_dir_all(&root).unwrap();
}
