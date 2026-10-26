//! The agent CLIs swarm can launch, and what each one accepts. This module is the only place that
//! names a provider as a string; other code matches on `Provider`, so a new variant shows every
//! place that must handle it. Code that reads a provider's own file format (hook payloads,
//! screens, chair logs, trust files) still dispatches by provider in its own module.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
    Agy,
    Pi,
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
    /// The efforts this model takes, when the CLI says; else the provider's list applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub efforts: Option<Vec<String>>,
    /// A short line on what the model is for, when the CLI or the alias list gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The level the CLI picks for this model, when it says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
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
    pub const ALL: [Provider; 4] = [
        Provider::Claude,
        Provider::Codex,
        Provider::Agy,
        Provider::Pi,
    ];

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
            Provider::Pi => "pi",
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
            Provider::Agy => ("AGY", &["low", "medium", "high", "max"], AGY_FIELDS),
            // pi's `--thinking` levels.
            Provider::Pi => (
                "Pi",
                &["off", "minimal", "low", "medium", "high", "xhigh", "max"],
                &[],
            ),
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

    /// AGY and pi have no native account adapter and use their CLI login.
    pub fn has_accounts(self) -> bool {
        !matches!(self, Provider::Agy | Provider::Pi)
    }

    /// The CLI whose accounts a launch on `model` uses. pi's claude-bridge starts `claude` with
    /// pi's environment, so a Claude account applies to it. pi's openai-codex reads the Codex
    /// account that `PI_CODEX_HOME` names (`pi_codex_env`).
    pub fn account_provider(self, model: &str) -> Option<Provider> {
        match self {
            Provider::Pi => match pi_provider(model).as_deref() {
                Some("claude-bridge") => Some(Provider::Claude),
                Some("openai-codex") => Some(Provider::Codex),
                _ => None,
            },
            _ => self.has_accounts().then_some(self),
        }
    }

    pub fn login_argv(self) -> Option<&'static [&'static str]> {
        match self {
            Provider::Codex => Some(&["codex", "login"]),
            Provider::Claude => Some(&["claude", "auth", "login"]),
            Provider::Agy | Provider::Pi => None,
        }
    }

    /// Clear inherited keys before applying an account, including the native default.
    pub fn account_env_keys(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &[
                "CLAUDE_CONFIG_DIR",
                "CLAUDE_SECURESTORAGE_CONFIG_DIR",
                "AGENT_PROFILE_LABEL",
            ],
            // pi gets a Claude account for claude-bridge and a Codex one for openai-codex.
            Provider::Pi => &[
                "CLAUDE_CONFIG_DIR",
                "CLAUDE_SECURESTORAGE_CONFIG_DIR",
                "AGENT_PROFILE_LABEL",
                "PI_CODEX_HOME",
                "PI_CODEX_PROFILE",
            ],
            Provider::Codex => &["CODEX_HOME"],
            Provider::Agy => &[],
        }
    }

    /// The account variables of a chat launch. It never sets `CLAUDE_CONFIG_DIR`: each Claude
    /// account uses the owner's one config folder and differs only in its login storage.
    pub fn launch_env(self, name: &str, dir: &str) -> BTreeMap<String, String> {
        match self {
            Provider::Claude => {
                secure_storage_dir(dir, name).map_or_else(BTreeMap::new, |storage| {
                    BTreeMap::from([
                        ("AGENT_PROFILE_LABEL".into(), name.into()),
                        ("CLAUDE_SECURESTORAGE_CONFIG_DIR".into(), storage),
                    ])
                })
            }
            Provider::Codex => BTreeMap::from([("CODEX_HOME".into(), dir.into())]),
            Provider::Agy | Provider::Pi => BTreeMap::new(),
        }
    }

    /// Only for `claude auth status` and `claude auth login`, which find a login by
    /// `CLAUDE_CONFIG_DIR`. Never give this to a chat launch.
    pub fn identity_env(self, name: &str, dir: &str) -> BTreeMap<String, String> {
        let mut env = self.launch_env(name, dir);
        if self == Provider::Claude && name != "default" {
            env.insert("CLAUDE_CONFIG_DIR".into(), dir.into());
        }
        env
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
            Provider::Pi => Ok(pi_models(&run("pi", &["--list-models"])?)),
        }
    }

    /// What the provider CLI on PATH knows as models, for `model_known`. None when the check
    /// cannot run; agy is skipped because `agy models` asks the network (about 5 s), and pi
    /// because it takes a model pattern, not only a listed id.
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
            Provider::Agy | Provider::Pi => None,
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
            // The session flags would move or drop the log the app finds by its fixed id.
            Provider::Pi => &[
                "--model",
                "--provider",
                "--thinking",
                "--session-id",
                "--session",
                "--session-dir",
                "--no-session",
                "--continue",
                "-c",
                "--resume",
                "-r",
                "--fork",
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

/// The default uses the native Keychain item; existing named homes keep their legacy item.
pub fn secure_storage_dir(home: &str, name: &str) -> Option<String> {
    if name == "default" {
        return None;
    }
    if name == "current"
        || crate::paths::root_dir().is_ok_and(|root| {
            let root = std::fs::canonicalize(&root).unwrap_or(root);
            std::path::Path::new(home)
                .starts_with(crate::accounts::accounts_dir(&root, Provider::Claude))
        })
    {
        return Some(home.into());
    }
    Some(
        std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join(format!(".claude-{name}"))
            .to_string_lossy()
            .into_owned(),
    )
}

pub fn info_list() -> InfoList {
    InfoList {
        providers: Provider::ALL.into_iter().map(Provider::info).collect(),
    }
}

fn claude_models() -> Vec<Model> {
    [
        ("default", "the account's default"),
        ("sonnet", "fast, most work"),
        ("opus", "deepest"),
        ("haiku", "quickest"),
        ("fable", "own usage limit"),
        ("best", "most capable, now Fable"),
        ("sonnet[1m]", "Sonnet, 1M context"),
        ("opus[1m]", "Opus, 1M context"),
        ("opusplan", "Opus plans, Sonnet builds"),
    ]
    .into_iter()
    .map(|(id, note)| Model {
        id: id.into(),
        label: id.into(),
        efforts: None,
        note: Some(note.into()),
        default_effort: None,
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
            let efforts = row["supported_reasoning_levels"].as_array().map(|levels| {
                levels
                    .iter()
                    .filter_map(|level| level["effort"].as_str().map(str::to_string))
                    .collect()
            });
            // An empty string is no value, so the field stays absent.
            let text = |key: &str| {
                row[key]
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            Some(Model {
                id: id.into(),
                label: label.into(),
                efforts,
                note: text("description"),
                default_effort: text("default_reasoning_level"),
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
                efforts: (!crate::bus::agy_takes_effort(Some(id))).then(Vec::new),
                note: None,
                default_effort: None,
            })
        })
        .collect()
}

/// The pi provider a launch on `model` runs: the model's prefix, or for `default` the
/// `defaultProvider` in pi's settings. A bare model id names no provider, so it gets none.
fn pi_provider(model: &str) -> Option<String> {
    if let Some((provider, _)) = model.split_once('/') {
        return Some(provider.into());
    }
    if model != "default" {
        return None;
    }
    pi_settings()?["defaultProvider"]
        .as_str()
        .map(str::to_owned)
}

/// The variables that give pi's openai-codex the Codex account `name` in `home`. The owner's pi
/// extension `codex-profile.ts` reads them; pi without it keeps its own login.
pub fn pi_codex_env(name: &str, home: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("PI_CODEX_HOME".into(), home.into()),
        ("PI_CODEX_PROFILE".into(), name.into()),
    ])
}

/// pi's `default` as the `provider/model` its settings name, so a launch records which login it
/// runs on. None when the settings name no provider and model.
pub fn pi_default_model() -> Option<String> {
    let settings = pi_settings()?;
    let (provider, model) = (
        settings["defaultProvider"].as_str()?,
        settings["defaultModel"].as_str()?,
    );
    Some(format!("{provider}/{model}"))
}

fn pi_settings() -> Option<serde_json::Value> {
    let dir = match std::env::var_os("PI_CODING_AGENT_DIR") {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::path::PathBuf::from(std::env::var_os("HOME")?).join(".pi/agent"),
    };
    serde_json::from_slice(&std::fs::read(dir.join("settings.json")).ok()?).ok()
}

/// The table of `pi --list-models`: a header row, then `provider model context ...` rows.
fn pi_models(output: &str) -> Vec<Model> {
    output
        .lines()
        .skip_while(|line| !line.starts_with("provider "))
        .skip(1)
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let (provider, model) = (columns.next()?, columns.next()?);
            Some(Model {
                id: format!("{provider}/{model}"),
                label: format!("{model} ({provider})"),
                efforts: None,
                note: None,
                default_effort: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
