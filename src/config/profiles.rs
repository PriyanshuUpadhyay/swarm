//! Profile data, validation, import, and runner selection.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runner {
    pub provider: Provider,
    pub model: String,
    pub effort: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
}

impl Runner {
    /// `claude/opus/high`, the name launch output and skip reasons use.
    pub fn label(&self) -> String {
        format!("{}/{}/{}", self.provider.id(), self.model, self.effort)
    }

    fn extras(&self) -> [(&'static str, Option<&str>); 3] {
        [
            ("sandbox", self.sandbox.as_deref()),
            ("approval", self.approval.as_deref()),
            ("permission", self.permission.as_deref()),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub runners: Vec<Runner>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Imported {
    pub from: String,
    pub unmapped: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default = "default_min_usage_left_pct")]
    pub min_usage_left_pct: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported: Option<Imported>,
    pub profiles: Vec<Profile>,
}

fn default_min_usage_left_pct() -> u8 {
    DEFAULT_MIN_USAGE_LEFT_PCT
}

/// The configured `min_usage_left_pct`, or the default when the config cannot be read.
pub fn min_usage_left_pct() -> u8 {
    load().map_or(DEFAULT_MIN_USAGE_LEFT_PCT, |(config, _)| {
        config.min_usage_left_pct
    })
}

impl Config {
    pub fn profile(&self, name: &str) -> Result<&Profile, String> {
        self.profiles
            .iter()
            .find(|profile| profile.name == name)
            .ok_or_else(|| format!("profile '{name}' is not defined"))
    }
}

/// What `swarm roles --json` prints: the config plus the revision a save must name.
#[derive(Debug, Serialize)]
pub struct Listing {
    pub revision: String,
    #[serde(flatten)]
    pub config: Config,
}

/// One value that goes into an agent's argv after a flag, so it is the trust boundary: one word,
/// not a flag.
pub fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
}

pub const MAX_ACCOUNT_NAME_LENGTH: usize = 64;

/// An account name becomes one native directory component; reserved names cannot be registered.
pub fn valid_account_name(name: &str) -> bool {
    (1..=MAX_ACCOUNT_NAME_LENGTH).contains(&name.len())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        && !matches!(name, "auto" | "default" | "current" | "menu" | "." | "..")
        && !name.contains("..")
}

/// Every rule the config breaks.
pub fn validate(config: &Config) -> Vec<String> {
    let mut errors = Vec::new();
    if config.version != 1 {
        errors.push(format!("version must be 1, not {}.", config.version));
    }
    if config.min_usage_left_pct > 100 {
        errors.push("min_usage_left_pct must be 0 to 100.".into());
    }
    if config.profiles.first().map(|profile| profile.name.as_str()) != Some("chat") {
        errors.push("The first profile must be 'chat'.".into());
    }
    let mut names = std::collections::HashSet::new();
    for profile in &config.profiles {
        errors.extend(profile_errors(profile));
        if !names.insert(profile.name.as_str()) {
            errors.push(format!("Profile '{}' is defined twice.", profile.name));
        }
    }
    errors
}

/// Every rule one profile breaks on its own.
fn profile_errors(profile: &Profile) -> Vec<String> {
    let mut errors = Vec::new();
    let name = &profile.name;
    if name.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        errors.push(format!(
            "Profile name '{name}' must use a-z, 0-9, '.', '_', '-'."
        ));
    }
    if profile.runners.is_empty() {
        errors.push(format!("Profile '{name}' must have at least one runner."));
    }
    // Fable is a child seat only where it judges, never where it writes code.
    let judges = name.starts_with("review.") || name.starts_with("council.");
    for (index, runner) in profile.runners.iter().enumerate() {
        let at = format!("Profile '{name}' runner {}", index + 1);
        if profile.runners[..index].contains(runner) {
            errors.push(format!("{at} repeats an earlier runner."));
        }
        if !valid_name(&runner.model) {
            errors.push(format!("{at} has invalid model '{}'.", runner.model));
        }
        let info = runner.provider.info();
        if !info.efforts.contains(&runner.effort.as_str()) {
            errors.push(format!(
                "{at} has effort '{}', which {} does not take.",
                runner.effort, info.label
            ));
        }
        for (field, value) in runner.extras() {
            let Some(value) = value else { continue };
            if !info.fields.iter().any(|known| known.name == field) {
                errors.push(format!("{at}: {} has no {field}.", info.label));
            } else if !valid_name(value) {
                errors.push(format!("{at} has invalid {field} '{value}'."));
            }
        }
        if runner.model.to_lowercase().contains("fable") && !judges {
            errors.push(format!(
                "{at}: Fable is allowed only in a review.* or council.* profile."
            ));
        }
    }
    errors
}

pub fn parse(text: &str) -> Result<Config, String> {
    let config: Config = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let errors = validate(&config);
    match errors.is_empty() {
        true => Ok(config),
        false => Err(errors.join("\n")),
    }
}

pub fn built_in() -> Config {
    parse(DEFAULT).expect("default-profiles.json is valid; a test checks it")
}

/// Converts the old agent-routing config. Each route becomes a profile in the file's key order;
/// its runners are the route's runners, then each one's substitutes, copied by value with exact
/// repeats dropped. A route that cannot convert is named in `unmapped`, and the rest still import.
pub fn import(old: &Value, from: &str) -> Result<Config, String> {
    let (Some(routes), Some(runners)) = (old["routes"].as_object(), old["runners"].as_object())
    else {
        return Err(format!("{from}: routes and runners must be objects"));
    };
    let convert = |id: &str| -> Option<Runner> {
        let fields = runners.get(id)?.as_object()?;
        let text = |key: &str| fields.get(key).and_then(Value::as_str).map(str::to_string);
        let provider = Provider::parse(&text("provider")?)?;
        Some(Runner {
            provider,
            model: text("model")?,
            effort: text("effort").unwrap_or_else(|| provider.info().default_effort.into()),
            sandbox: text("sandbox"),
            approval: text("approval"),
            permission: text("permission"),
        })
    };
    let mut profiles = Vec::new();
    let mut unmapped = Vec::new();
    for (route, ids) in routes {
        let ids: Vec<&str> = ids
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let substitutes = ids.iter().flat_map(|id| {
            old["substitutes"][*id]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        });
        let Some(mut list) = ids
            .iter()
            .map(|id| convert(id))
            .collect::<Option<Vec<Runner>>>()
            .filter(|list| !list.is_empty())
        else {
            unmapped.push(route.clone());
            continue;
        };
        list = list.into_iter().fold(Vec::new(), |mut kept, runner| {
            if !kept.contains(&runner) {
                kept.push(runner);
            }
            kept
        });
        for runner in substitutes.filter_map(convert) {
            if !list.contains(&runner) {
                list.push(runner);
            }
        }
        let profile = Profile {
            name: route.clone(),
            runners: list,
        };
        // A route the old schema took but a profile may not hold is left out, not the import.
        if !profile_errors(&profile).is_empty() {
            unmapped.push(route.clone());
            continue;
        }
        profiles.push(profile);
    }
    if !profiles.iter().any(|profile| profile.name == "chat") {
        profiles.insert(0, built_in().profiles.remove(0));
    }
    if let Some(at) = profiles.iter().position(|profile| profile.name == "chat") {
        let chat = profiles.remove(at);
        profiles.insert(0, chat);
    }
    let config = Config {
        version: 1,
        min_usage_left_pct: DEFAULT_MIN_USAGE_LEFT_PCT,
        imported: Some(Imported {
            from: from.into(),
            unmapped,
        }),
        profiles,
    };
    let errors = validate(&config);
    match errors.is_empty() {
        true => Ok(config),
        false => Err(format!("{from}: {}", errors.join("\n"))),
    }
}

/// The runner for a chat whose provider and model the owner picked once (ADR 0033): the chat
/// profile's first runner of that provider with the picked model, else the provider's default
/// effort and the flags a chat had before profiles existed.
pub fn one_off(chat: &Profile, provider: Provider, model: &str) -> Result<Runner, String> {
    if !valid_name(model) {
        return Err("model must be one non-empty name".into());
    }
    let base = chat
        .runners
        .iter()
        .find(|runner| runner.provider == provider)
        .cloned()
        .unwrap_or_else(|| Runner {
            provider,
            model: String::new(),
            effort: provider.info().default_effort.into(),
            sandbox: (provider == Provider::Codex).then(|| "workspace-write".into()),
            approval: None,
            permission: None,
        });
    Ok(Runner {
        model: model.into(),
        ..base
    })
}

/// Why a runner did not start. New codes may appear; a reader shows `text` for one it does not
/// know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipCode {
    CliMissing,
    SignedOut,
    LowUsage,
    FableSpent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Skip {
    pub index: usize,
    pub code: SkipCode,
    pub text: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Selection {
    /// The runner that starts, as an index into the profile's runners; None when every one was
    /// skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick: Option<usize>,
    pub skipped: Vec<Skip>,
}

/// Which runner each profile would start now, as `roles check --json` prints it.
#[derive(Debug, Serialize)]
pub struct ProfileCheck {
    pub name: String,
    #[serde(flatten)]
    pub selection: Selection,
}

/// The first runner `probe` does not skip. With `only`, that provider's runners are tried first
/// and the rest after, so a seat for one provider still starts when that provider cannot run.
pub fn select(
    profile: &Profile,
    only: Option<Provider>,
    probe: impl Fn(&Runner) -> Option<(SkipCode, String)>,
) -> Selection {
    let mut order: Vec<usize> = (0..profile.runners.len()).collect();
    if let Some(only) = only {
        order.sort_by_key(|index| profile.runners[*index].provider != only);
    }
    let mut skipped = Vec::new();
    for index in order {
        match probe(&profile.runners[index]) {
            None => {
                return Selection {
                    pick: Some(index),
                    skipped,
                };
            }
            Some((code, text)) => skipped.push(Skip { index, code, text }),
        }
    }
    Selection {
        pick: None,
        skipped,
    }
}

/// One account as the usage check sees it.
#[derive(Clone, Copy, Debug)]
pub struct AccountState {
    pub signed_in: bool,
    pub remaining_pct: Option<i64>,
}

/// Why a provider's accounts cannot start a runner, or None. `accounts` is None when the read
/// failed or timed out, and an empty list means no account source; both count as "can run", so a
/// missing usage source still permits launch (ADR 0032). An unknown percentage permits launch.
pub fn account_skip(
    provider: Provider,
    accounts: Option<&[AccountState]>,
    min_usage_left_pct: u8,
) -> Option<(SkipCode, String)> {
    let accounts = accounts.filter(|accounts| !accounts.is_empty())?;
    let signed_in: Vec<&AccountState> = accounts
        .iter()
        .filter(|account| account.signed_in)
        .collect();
    if signed_in.is_empty() {
        return Some((
            SkipCode::SignedOut,
            format!("no {} account is signed in", provider.id()),
        ));
    }
    let mut best = i64::MIN;
    for account in signed_in {
        match account.remaining_pct {
            None => return None,
            Some(left) if left >= i64::from(min_usage_left_pct) => return None,
            Some(left) => best = best.max(left),
        }
    }
    Some((
        SkipCode::LowUsage,
        format!("usage {best}% left (threshold {min_usage_left_pct}%)"),
    ))
}
