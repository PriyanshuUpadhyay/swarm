//! Tests for providers.

use super::*;

#[test]
fn every_provider_parses_from_its_own_id_and_nothing_else_does() {
    for provider in Provider::ALL {
        assert_eq!(Provider::parse(provider.id()), Some(provider));
    }
    assert_eq!(Provider::parse("Claude"), None);
    assert_eq!(Provider::parse("opencode"), None);
}

#[test]
fn each_field_default_is_one_of_its_listed_values() {
    for info in info_list().providers {
        assert!(info.efforts.contains(&info.default_effort), "{}", info.id);
        for field in info.fields {
            assert!(field.values.contains(&field.default), "{}", field.name);
        }
    }
}

#[test]
fn catalogs_show_listed_codex_models_and_agy_models() {
    let codex = br#"{"models":[{"slug":"gpt-6-sol","display_name":"GPT-6-Sol","visibility":"list","supported_reasoning_levels":[{"effort":"low"},{"effort":"ultra"}]},{"slug":"gpt-old","visibility":"list"},{"slug":"internal","visibility":"hide"}]}"#;
    assert_eq!(
        codex_models(codex).unwrap(),
        vec![
            Model {
                id: "gpt-6-sol".into(),
                label: "GPT-6-Sol".into(),
                efforts: Some(vec!["low".into(), "ultra".into()]),
                note: None,
                default_effort: None,
            },
            Model {
                id: "gpt-old".into(),
                label: "gpt-old".into(),
                efforts: None,
                note: None,
                default_effort: None,
            },
        ]
    );
    assert!(codex_models(b"not json").is_err());
    assert_eq!(
        agy_models(
            "Fetching...\ngemini-3.8-flash\tGemini 3.8 Flash\nclaude-sonnet-4-6\tClaude Sonnet 4.6\n"
        ),
        vec![
            Model {
                id: "gemini-3.8-flash".into(),
                label: "Gemini 3.8 Flash".into(),
                efforts: None,
                note: None,
                default_effort: None,
            },
            // AGY refuses --effort for this model, so it lists no level.
            Model {
                id: "claude-sonnet-4-6".into(),
                label: "Claude Sonnet 4.6".into(),
                efforts: Some(vec![]),
                note: None,
                default_effort: None,
            },
        ]
    );
}

#[test]
fn models_carry_a_note_and_the_cli_default_level() {
    let codex = br#"{"models":[{"slug":"gpt-6-luna","display_name":"GPT-6-Luna","visibility":"list","description":"Fast and affordable model.","default_reasoning_level":"medium"}]}"#;
    let luna = &codex_models(codex).unwrap()[0];
    assert_eq!(luna.note.as_deref(), Some("Fast and affordable model."));
    assert_eq!(luna.default_effort.as_deref(), Some("medium"));
    // An empty description or level is absent, as the CLI gave none.
    let bare = br#"{"models":[{"slug":"x","display_name":"X","visibility":"list","description":" ","default_reasoning_level":""}]}"#;
    let bare = &codex_models(bare).unwrap()[0];
    assert!(bare.note.is_none() && bare.default_effort.is_none());
    let claude = claude_models();
    let opus = claude.iter().find(|m| m.id == "opus").unwrap();
    assert_eq!(opus.note.as_deref(), Some("deepest"));
    assert!(claude.iter().all(|m| m.note.is_some()));
    // An absent note is absent in the JSON, never null.
    let json = serde_json::to_value(&agy_models("g\tG\n")[0]).unwrap();
    assert!(json.get("note").is_none() && json.get("default_effort").is_none());
}

#[test]
fn pi_lists_its_models_as_provider_slash_id() {
    let listed = "provider       model            context  max-out  thinking  images\nanthropic      claude-haiku-5-5 1M       128K     yes       yes\nopenai-codex   gpt-6-sol        272K     128K     yes       yes\n\n";
    assert_eq!(
        pi_models(listed),
        vec![
            Model {
                id: "anthropic/claude-haiku-5-5".into(),
                label: "claude-haiku-5-5 (anthropic)".into(),
                efforts: None,
                note: None,
                default_effort: None,
            },
            Model {
                id: "openai-codex/gpt-6-sol".into(),
                label: "gpt-6-sol (openai-codex)".into(),
                efforts: None,
                note: None,
                default_effort: None,
            },
        ]
    );
    assert!(pi_models("No models available.\n").is_empty());
    assert!(!Provider::Pi.has_accounts());
    assert!(Provider::Pi.info().efforts.contains(&"off"));
}

#[test]
fn claude_current_hashes_its_config_home_and_launch_shares_chats() {
    let launch = Provider::Claude.launch_env("current", "/private/config");
    assert_eq!(launch["CLAUDE_SECURESTORAGE_CONFIG_DIR"], "/private/config");
    assert!(!launch.contains_key("CLAUDE_CONFIG_DIR"));
    assert_eq!(
        Provider::Claude.identity_env("current", "/private/config")["CLAUDE_CONFIG_DIR"],
        "/private/config"
    );
}

#[test]
fn claude_default_uses_the_native_store_without_profile_overrides() {
    assert!(
        Provider::Claude
            .launch_env("default", "/native-home/.claude")
            .is_empty()
    );
    assert!(
        Provider::Claude
            .identity_env("default", "/native-home/.claude")
            .is_empty()
    );
}

#[test]
fn a_model_is_checked_against_the_catalog() {
    let binary = br#"aliases:{opus:{default:"claude-opus-5-5"}},x="opus""#;
    assert!(Provider::Claude.model_known(binary, "claude-opus-5-5"));
    assert!(Provider::Claude.model_known(binary, "opus"));
    assert!(!Provider::Claude.model_known(binary, "claude-opus-5"));
    let codex = br#"{"models":[{"slug":"gpt-6-sol"},{"slug":"gpt-6-luna"}]}"#;
    assert!(Provider::Codex.model_known(codex, "gpt-6-sol"));
    assert!(!Provider::Codex.model_known(codex, "gpt-5.6-sol"));
    assert!(!Provider::Codex.model_known(b"not json", "gpt-6-sol"));
}
