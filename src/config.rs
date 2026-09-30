//! Agent profiles: which runners start each role, in order (ADRs 0030, 0031). The file is
//! `$SWARM_HOME/.swarm/profiles.json`. With no file, the first read imports the old agent-routing
//! `roles.json` if one exists, else the built-in `default-profiles.json` is used and nothing is
//! written.

use crate::providers::Provider;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const DEFAULT: &str = include_str!("../default-profiles.json");

/// Below this share of usage left on every signed-in account, a runner is skipped. Low enough
/// that a nearly spent account still runs a short task, high enough to skip one that would stop
/// mid-turn.
const DEFAULT_MIN_USAGE_LEFT_PCT: u8 = 5;

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

/// The runner for a chat whose provider and model the owner picked once (ADR 0032): the chat
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
/// Mac without yelo still launches (ADR 0031). An account with no usage number counts as can run.
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

/// The first 12 hex digits of the SHA-256 of the bytes a listing was made from.
pub fn revision(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn path() -> Result<PathBuf, String> {
    crate::paths::root_dir()
        .map(|root| root.join("profiles.json"))
        .map_err(|error| error.to_string())
}

/// The old agent-routing file: `$AGENT_ROUTING_CONFIG`, else under `$XDG_CONFIG_HOME`, else under
/// `~/.config`. An explicit path that is missing is an error, never a fall-through.
fn old_path() -> Result<PathBuf, String> {
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

/// Reads `path`, or None when nothing is there. A link to a missing file is an error, so a moved
/// dotfiles checkout cannot switch every profile to the default without a word.
fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && std::fs::symlink_metadata(path).is_err() =>
        {
            Ok(None)
        }
        result => result
            .map(Some)
            .map_err(|error| format!("cannot read {}: {error}", path.display())),
    }
}

/// The profiles and the bytes they came from. The first read with no file imports the old
/// config and writes the result; with no old config either, it is the built-in default.
pub fn load() -> Result<(Config, Vec<u8>), String> {
    let path = path()?;
    if let Some(bytes) = read(&path)? {
        let text = String::from_utf8(bytes.clone())
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let config = parse(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        warn_if_old_is_newer(&path, &config);
        return Ok((config, bytes));
    }
    let old = old_path()?;
    let Some(bytes) = read(&old)? else {
        return Ok((built_in(), DEFAULT.as_bytes().to_vec()));
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", old.display()))?;
    let config = import(&value, &old.to_string_lossy())?;
    let bytes = write(&path, &config)?;
    Ok((config, bytes))
}

/// The import runs once, so a later edit to the old file changes nothing; say so rather than
/// lose it without a word.
fn warn_if_old_is_newer(path: &Path, config: &Config) {
    let Some(from) = config
        .imported
        .as_ref()
        .map(|imported| PathBuf::from(&imported.from))
    else {
        return;
    };
    let modified = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    };
    if let (Some(old), Some(new)) = (modified(&from), modified(path))
        && old > new
    {
        eprintln!(
            "swarm: {} changed after import; {} is the source now",
            from.display(),
            path.display()
        );
    }
}

pub fn listing() -> Result<Listing, String> {
    let (config, bytes) = load()?;
    Ok(Listing {
        revision: revision(&bytes),
        config,
    })
}

/// Replaces one profile by name when the file still has `expected` as its revision, and returns
/// the new revision.
pub fn save(profile: Profile, expected: &str) -> Result<String, String> {
    let (mut config, bytes) = load()?;
    if revision(&bytes) != expected {
        return Err("profiles changed on disk; reload and try again".into());
    }
    let slot = config
        .profiles
        .iter_mut()
        .find(|slot| slot.name == profile.name)
        .ok_or_else(|| format!("profile '{}' is not defined", profile.name))?;
    *slot = profile;
    let errors = validate(&config);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    write(&path()?, &config).map(|bytes| revision(&bytes))
}

/// Writes `config`: a timestamped backup beside an existing file, then an atomic rename onto the
/// real file, because the owner may link `profiles.json` into dotfiles and a rename onto the link
/// itself would replace it.
fn write(path: &Path, config: &Config) -> Result<Vec<u8>, String> {
    let text = serde_json::to_string_pretty(config).map_err(|error| error.to_string())? + "\n";
    let at = |error: std::io::Error| format!("{}: {error}", path.display());
    let target = if std::fs::symlink_metadata(path).is_ok() {
        let target = std::fs::canonicalize(path).map_err(at)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs();
        let backup = format!("{}.{stamp}.bak", path.display());
        std::fs::copy(&target, &backup).map_err(|error| format!("{backup}: {error}"))?;
        target
    } else {
        std::fs::create_dir_all(path.parent().ok_or("profiles path has no folder")?).map_err(at)?;
        path.to_path_buf()
    };
    let temp = target.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temp, &text)
        .and_then(|()| std::fs::rename(&temp, &target))
        .map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            format!("{}: {error}", target.display())
        })?;
    Ok(text.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner(provider: Provider, model: &str) -> Runner {
        Runner {
            provider,
            model: model.into(),
            effort: "high".into(),
            sandbox: None,
            approval: None,
            permission: None,
        }
    }

    fn config(profiles: Vec<Profile>) -> Config {
        Config {
            version: 1,
            min_usage_left_pct: 5,
            imported: None,
            profiles,
        }
    }

    fn chat() -> Profile {
        Profile {
            name: "chat".into(),
            runners: vec![runner(Provider::Claude, "opus")],
        }
    }

    #[test]
    fn the_built_in_profiles_are_valid_and_start_with_chat() {
        let built_in = built_in();
        assert_eq!(built_in.profiles[0].name, "chat");
        assert!(built_in.imported.is_none());
    }

    #[test]
    fn validate_names_each_broken_rule() {
        assert_eq!(validate(&config(vec![chat()])), Vec::<String>::new());
        let mut codex = runner(Provider::Codex, "--yolo");
        codex.effort = "huge".into();
        codex.permission = Some("auto".into());
        codex.sandbox = Some("two words".into());
        let broken = Config {
            version: 2,
            min_usage_left_pct: 101,
            imported: None,
            profiles: vec![
                Profile {
                    name: "Code".into(),
                    runners: vec![codex, runner(Provider::Claude, "claude-fable-5")],
                },
                Profile {
                    name: "chat".into(),
                    runners: vec![],
                },
                chat(),
            ],
        };
        assert_eq!(
            validate(&broken),
            [
                "version must be 1, not 2.",
                "min_usage_left_pct must be 0 to 100.",
                "The first profile must be 'chat'.",
                "Profile name 'Code' must use a-z, 0-9, '.', '_', '-'.",
                "Profile 'Code' runner 1 has invalid model '--yolo'.",
                "Profile 'Code' runner 1 has effort 'huge', which Codex does not take.",
                "Profile 'Code' runner 1 has invalid sandbox 'two words'.",
                "Profile 'Code' runner 1: Codex has no permission.",
                "Profile 'Code' runner 2: Fable is allowed only in a review.* or council.* profile.",
                "Profile 'chat' must have at least one runner.",
                "Profile 'chat' is defined twice.",
            ]
        );
        let mut repeated = chat();
        repeated.runners.push(runner(Provider::Claude, "opus"));
        assert_eq!(
            validate(&config(vec![repeated])),
            ["Profile 'chat' runner 2 repeats an earlier runner."]
        );
    }

    fn account(signed_in: bool, remaining_pct: Option<i64>) -> AccountState {
        AccountState {
            signed_in,
            remaining_pct,
        }
    }

    #[test]
    fn accounts_skip_a_runner_only_when_signed_out_or_low_on_every_account() {
        let claude = Provider::Claude;
        assert_eq!(account_skip(claude, None, 5), None);
        assert_eq!(account_skip(claude, Some(&[]), 5), None);
        assert_eq!(
            account_skip(claude, Some(&[account(false, Some(90))]), 5),
            Some((SkipCode::SignedOut, "no claude account is signed in".into()))
        );
        let spent = [
            account(true, Some(2)),
            account(true, Some(4)),
            account(false, Some(80)),
        ];
        assert_eq!(
            account_skip(claude, Some(&spent), 5),
            Some((SkipCode::LowUsage, "usage 4% left (threshold 5%)".into()))
        );
        assert_eq!(
            account_skip(
                claude,
                Some(&[account(true, Some(2)), account(true, Some(5))]),
                5
            ),
            None
        );
        assert_eq!(
            account_skip(
                claude,
                Some(&[account(true, Some(2)), account(true, None)]),
                5
            ),
            None
        );
    }

    #[test]
    fn select_takes_the_first_runner_the_probe_passes_and_lists_each_skip() {
        let profile = Profile {
            name: "code.complex".into(),
            runners: vec![
                runner(Provider::Claude, "opus"),
                runner(Provider::Codex, "gpt-6.1-sol"),
                runner(Provider::Agy, "flash"),
            ],
        };
        let no_claude = |runner: &Runner| {
            (runner.provider == Provider::Claude)
                .then(|| (SkipCode::LowUsage, "usage 2% left".into()))
        };
        let selection = select(&profile, None, no_claude);
        assert_eq!(selection.pick, Some(1));
        assert_eq!(
            selection.skipped,
            [Skip {
                index: 0,
                code: SkipCode::LowUsage,
                text: "usage 2% left".into()
            }]
        );
        assert_eq!(
            select(&profile, Some(Provider::Agy), no_claude).pick,
            Some(2)
        );
        let nothing = select(&profile, None, |_| {
            Some((SkipCode::CliMissing, "gone".into()))
        });
        assert_eq!(nothing.pick, None);
        assert_eq!(
            nothing
                .skipped
                .iter()
                .map(|skip| skip.index)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
    }

    #[test]
    fn a_one_off_chat_takes_the_chat_profiles_runner_of_that_provider_or_the_defaults() {
        let mut codex = runner(Provider::Codex, "gpt-6.1-sol");
        codex.effort = "xhigh".into();
        codex.sandbox = Some("read-only".into());
        let chat = Profile {
            name: "chat".into(),
            runners: vec![runner(Provider::Claude, "opus"), codex],
        };
        let picked = one_off(&chat, Provider::Codex, "gpt-6-luna").unwrap();
        assert_eq!(
            (
                picked.model.as_str(),
                picked.effort.as_str(),
                picked.sandbox.as_deref()
            ),
            ("gpt-6-luna", "xhigh", Some("read-only"))
        );
        let agy = one_off(&chat, Provider::Agy, "flash").unwrap();
        assert_eq!((agy.effort.as_str(), agy.permission), ("medium", None));
        let claude_only = Profile {
            name: "chat".into(),
            runners: vec![runner(Provider::Claude, "opus")],
        };
        let fresh = one_off(&claude_only, Provider::Codex, "gpt-6-luna").unwrap();
        assert_eq!(fresh.sandbox.as_deref(), Some("workspace-write"));
        assert!(one_off(&chat, Provider::Claude, "--bad").is_err());
    }
}
