//! The agent CLIs swarm can launch, and what each one accepts. This module is the only place that
//! names a provider as a string; other code matches on `Provider`, so a new variant shows every
//! place that must handle it. Code that reads a provider's own file format (hook payloads,
//! screens, chair logs, trust files) still dispatches by provider in its own module.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
    Agy,
}

/// One flag a provider takes beside model and effort, as a profile runner field.
#[derive(Debug, Serialize)]
pub struct Field {
    pub name: &'static str,
    pub label: &'static str,
    /// The values the CLI help listed on 2026-09-30. A hint for the app's picker, not a gate:
    /// the CLIs add and drop values between releases.
    pub values: &'static [&'static str],
    pub default: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Info {
    pub id: &'static str,
    pub label: &'static str,
    pub efforts: &'static [&'static str],
    pub default_effort: &'static str,
    pub accounts: bool,
    pub fields: &'static [Field],
}

#[derive(Debug, Serialize)]
pub struct InfoList {
    pub providers: Vec<Info>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Model {
    pub id: String,
    pub label: String,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct ModelList {
    pub provider: String,
    pub models: Vec<Model>,
}

const CLAUDE_FIELDS: &[Field] = &[Field {
    name: "permission",
    label: "Permission",
    values: &[
        "acceptEdits",
        "auto",
        "bypassPermissions",
        "manual",
        "dontAsk",
        "plan",
    ],
    default: "auto",
}];

const CODEX_FIELDS: &[Field] = &[
    Field {
        name: "sandbox",
        label: "Sandbox",
        values: &["read-only", "workspace-write", "danger-full-access"],
        default: "workspace-write",
    },
    Field {
        name: "approval",
        label: "Approval",
        values: &["on-request", "never"],
        default: "never",
    },
];

const AGY_FIELDS: &[Field] = &[Field {
    name: "permission",
    label: "Permission",
    values: &["skip", "accept-edits", "plan"],
    default: "skip",
}];

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Claude, Provider::Codex, Provider::Agy];

    pub fn parse(name: &str) -> Option<Provider> {
        Provider::ALL
            .into_iter()
            .find(|provider| provider.id() == name)
    }

    /// The provider's id, which is also the name of its CLI on PATH.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Agy => "agy",
        }
    }

    pub fn info(self) -> Info {
        let (label, efforts, fields): (_, &'static [&'static str], _) = match self {
            Provider::Claude => (
                "Claude",
                &["low", "medium", "high", "xhigh", "max"],
                CLAUDE_FIELDS,
            ),
            Provider::Codex => (
                "Codex",
                &["low", "medium", "high", "xhigh", "max", "ultra"],
                CODEX_FIELDS,
            ),
            Provider::Agy => ("Gemini", &["low", "medium", "high", "max"], AGY_FIELDS),
        };
        Info {
            id: self.id(),
            label,
            efforts,
            default_effort: "medium",
            accounts: self.has_accounts(),
            fields,
        }
    }

    /// Whether yelo keeps signed-in accounts and usage for this provider. AGY has no source yet
    /// and launches on its CLI's own login.
    pub fn has_accounts(self) -> bool {
        !matches!(self, Provider::Agy)
    }

    /// The same variables yelo writes into `~/.local/bin/<cli>-<account>`.
    ///
    /// `CLAUDE_CONFIG_DIR` alone is not enough: Claude Code keeps the credentials in a second tree,
    /// so a pane that gets only the config dir starts at "Not logged in · Run /login".
    pub fn account_env(
        self,
        name: &str,
        dir: &str,
        env_var: impl FnOnce(&str) -> Option<std::ffi::OsString>,
    ) -> BTreeMap<String, String> {
        match self {
            Provider::Claude => {
                let home = std::path::PathBuf::from(env_var("HOME").unwrap_or_default());
                BTreeMap::from([
                    ("AGENT_PROFILE_LABEL".to_string(), name.to_string()),
                    ("CLAUDE_CONFIG_DIR".to_string(), dir.to_string()),
                    (
                        "CLAUDE_SECURESTORAGE_CONFIG_DIR".to_string(),
                        home.join(format!(".claude-{name}"))
                            .to_string_lossy()
                            .into_owned(),
                    ),
                ])
            }
            Provider::Codex => BTreeMap::from([("CODEX_HOME".to_string(), dir.to_string())]),
            Provider::Agy => BTreeMap::new(),
        }
    }

    /// The models the provider's CLI lists. `run` runs a CLI and returns its stdout. Claude has no
    /// list command, so it gets its documented aliases.
    pub fn models(
        self,
        run: impl Fn(&str, &[&str]) -> Result<String, String>,
    ) -> Result<Vec<Model>, String> {
        match self {
            Provider::Claude => Ok(claude_models()),
            Provider::Codex => codex_models(run("codex", &["debug", "models"])?.as_bytes()),
            Provider::Agy => Ok(agy_models(&run("agy", &["models"])?)),
        }
    }

    /// What the provider CLI on PATH knows as models, for `model_known`. None when the check
    /// cannot run; agy is skipped because `agy models` asks the network (about 5 s).
    pub fn catalog(self, account_env: &BTreeMap<String, String>) -> Option<Vec<u8>> {
        match self {
            Provider::Claude => std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|dir| dir.join("claude"))
                .find(|path| path.is_file())
                .and_then(|path| std::fs::read(path).ok()),
            Provider::Codex => std::process::Command::new("codex")
                .args(["debug", "models"])
                .envs(account_env)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| output.stdout),
            Provider::Agy => None,
        }
    }

    /// `catalog` is the Claude binary itself, where every model id and alias sits as a quoted
    /// string, or the JSON of `codex debug models`.
    pub fn model_known(self, catalog: &[u8], model: &str) -> bool {
        if self == Provider::Claude {
            let quoted = format!("\"{model}\"");
            return catalog
                .windows(quoted.len())
                .any(|window| window == quoted.as_bytes());
        }
        serde_json::from_slice::<serde_json::Value>(catalog)
            .ok()
            .and_then(|value| value["models"].as_array().cloned())
            .is_some_and(|models| models.iter().any(|entry| entry["slug"] == model))
    }

    /// Provider flags the role owns; a caller's extra args may not set them.
    pub fn owned_flags(self) -> &'static [&'static str] {
        match self {
            Provider::Codex => &[
                "-m",
                "--model",
                "-s",
                "--sandbox",
                "-a",
                "--ask-for-approval",
            ],
            Provider::Claude | Provider::Agy => &[
                "--model",
                "--effort",
                "--permission-mode",
                "--mode",
                "--dangerously-skip-permissions",
                "--yolo",
            ],
        }
    }
}

pub fn info_list() -> InfoList {
    InfoList {
        providers: Provider::ALL.into_iter().map(Provider::info).collect(),
    }
}

fn claude_models() -> Vec<Model> {
    [
        "default",
        "sonnet",
        "opus",
        "haiku",
        "fable",
        "best",
        "sonnet[1m]",
        "opus[1m]",
        "opusplan",
    ]
    .into_iter()
    .map(|id| Model {
        id: id.into(),
        label: id.into(),
    })
    .collect()
}

fn codex_models(json: &[u8]) -> Result<Vec<Model>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(json).map_err(|error| format!("codex model JSON: {error}"))?;
    let rows = value["models"]
        .as_array()
        .ok_or("codex model JSON has no models")?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            if row["visibility"] == "hide" {
                return None;
            }
            let id = row["slug"].as_str()?;
            let label = row["display_name"].as_str().unwrap_or(id);
            Some(Model {
                id: id.into(),
                label: label.into(),
            })
        })
        .collect())
}

fn agy_models(output: &str) -> Vec<Model> {
    output
        .lines()
        .filter_map(|line| {
            let (id, label) = line.split_once('\t')?;
            (!id.is_empty() && !label.is_empty()).then(|| Model {
                id: id.into(),
                label: label.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
        let codex = br#"{"models":[{"slug":"gpt-6-sol","display_name":"GPT-6-Sol","visibility":"list"},{"slug":"internal","visibility":"hide"}]}"#;
        assert_eq!(
            codex_models(codex).unwrap(),
            vec![Model {
                id: "gpt-6-sol".into(),
                label: "GPT-6-Sol".into()
            }]
        );
        assert!(codex_models(b"not json").is_err());
        assert_eq!(
            agy_models("Fetching...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\n"),
            vec![Model {
                id: "gemini-3.8-flash-high".into(),
                label: "Gemini 3.8 Flash (High)".into()
            }]
        );
    }

    #[test]
    fn claude_secure_storage_uses_home_and_ignores_swarm_home() {
        let requested = std::cell::RefCell::new(Vec::new());
        let environment = Provider::Claude.account_env("work", "/profiles/work", |name| {
            requested.borrow_mut().push(name.to_string());
            match name {
                "HOME" => Some("/login-home".into()),
                "SWARM_HOME" => Some("/swarm-home".into()),
                _ => None,
            }
        });
        assert_eq!(
            environment["CLAUDE_SECURESTORAGE_CONFIG_DIR"],
            "/login-home/.claude-work"
        );
        assert_eq!(*requested.borrow(), ["HOME"]);
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
}
