//! Profile loading, revision checks, actions, and disk writes.

use super::*;

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
    update(expected, |config| {
        let slot = config
            .profiles
            .iter_mut()
            .find(|slot| slot.name == profile.name)
            .ok_or_else(|| format!("profile '{}' is not defined", profile.name))?;
        *slot = profile;
        Ok(())
    })
}

pub enum ProfileAction<'a> {
    New(&'a str),
    Rename(&'a str, &'a str),
    Copy(&'a str, &'a str),
    Delete(&'a str),
    Reset,
    SetMinUsage(i64),
}

pub fn profile_action(action: ProfileAction<'_>, expected: &str) -> Result<String, String> {
    update(expected, |config| apply_action(config, action))
}

pub(super) fn apply_action(config: &mut Config, action: ProfileAction<'_>) -> Result<(), String> {
    let unused_name = |name: &str| {
        if config.profiles.iter().any(|profile| profile.name == name) {
            Err(format!("profile '{name}' already exists"))
        } else {
            Ok(())
        }
    };
    match action {
        ProfileAction::New(name) => {
            unused_name(name)?;
            let runner = config.profile("chat")?.runners[0].clone();
            config.profiles.push(Profile {
                name: name.into(),
                runners: vec![runner],
            });
        }
        ProfileAction::Rename(from, to) => {
            config.profile(from)?;
            unused_name(to)?;
            let profile = config
                .profiles
                .iter_mut()
                .find(|profile| profile.name == from)
                .unwrap();
            profile.name = to.into();
        }
        ProfileAction::Copy(from, to) => {
            let mut profile = config.profile(from)?.clone();
            unused_name(to)?;
            profile.name = to.into();
            config.profiles.push(profile);
        }
        ProfileAction::Delete(name) => {
            config.profile(name)?;
            if name == "chat" {
                return Err("the chat profile cannot be deleted".into());
            }
            config.profiles.retain(|profile| profile.name != name);
        }
        ProfileAction::Reset => config.profiles = parse(DEFAULT)?.profiles,
        ProfileAction::SetMinUsage(pct) => {
            config.min_usage_left_pct = u8::try_from(pct)
                .ok()
                .filter(|pct| *pct <= 100)
                .ok_or("min_usage_left_pct must be 0 to 100.")?;
        }
    }
    Ok(())
}

fn update(
    expected: &str,
    change: impl FnOnce(&mut Config) -> Result<(), String>,
) -> Result<String, String> {
    let (config, bytes) = load()?;
    update_at(&path()?, config, &bytes, expected, change)
}

pub(super) fn update_at(
    path: &Path,
    mut config: Config,
    bytes: &[u8],
    expected: &str,
    change: impl FnOnce(&mut Config) -> Result<(), String>,
) -> Result<String, String> {
    if revision(bytes) != expected {
        return Err("profiles changed on disk; reload and try again".into());
    }
    change(&mut config)?;
    let errors = validate(&config);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    write(path, &config).map(|bytes| revision(&bytes))
}

/// Writes `config`: a timestamped backup beside an existing file, then an atomic rename onto the
/// real file, because the owner may link `profiles.json` into dotfiles and a rename onto the link
/// itself would replace it.
pub(super) fn write(path: &Path, config: &Config) -> Result<Vec<u8>, String> {
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
