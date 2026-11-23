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
        return loaded(&path, bytes);
    }
    let old = old_path()?;
    let Some(bytes) = read(&old)? else {
        return Ok((built_in(), DEFAULT.as_bytes().to_vec()));
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", old.display()))?;
    let config = import(&value, &old.to_string_lossy())?;
    with_profile_lock(&path, || {
        if let Some(bytes) = read(&path)? {
            return loaded(&path, bytes);
        }
        let bytes = write(&path, &config)?;
        Ok((config, bytes))
    })
}

fn loaded(path: &Path, bytes: Vec<u8>) -> Result<(Config, Vec<u8>), String> {
    let text =
        std::str::from_utf8(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    let config = parse(text).map_err(|error| format!("{}: {error}", path.display()))?;
    warn_if_old_is_newer(path, &config);
    Ok((config, bytes))
}

fn with_profile_lock<T>(
    path: &Path,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let parent = path.parent().ok_or("profiles path has no folder")?;
    std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    crate::managed::with_lock(&path.with_extension("lock"), change)
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
    with_profile_lock(path, || {
        let on_disk = read(path)?;
        if revision(bytes) != expected || on_disk.as_deref().unwrap_or(DEFAULT.as_bytes()) != bytes
        {
            return Err("profiles changed on disk; reload and try again".into());
        }
        change(&mut config)?;
        let errors = validate(&config);
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        write(path, &config).map(|bytes| revision(&bytes))
    })
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
    prune_backups(path);
    Ok(text.into_bytes())
}

fn prune_backups(path: &Path) {
    const BACKUP_LIMIT: usize = 10;
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let prefix = format!("{name}.");

    let Some(entries) = path.parent().and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return;
    };

    let mut backups: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let stamp = name.to_str()?.strip_prefix(&prefix)?.strip_suffix(".bak")?;
            let stamp: u64 = stamp.parse().ok()?;
            entry
                .file_type()
                .ok()?
                .is_file()
                .then(|| (stamp, entry.path()))
        })
        .collect();

    backups.sort_unstable_by(|a, b| b.cmp(a));
    for (_, path) in backups.into_iter().skip(BACKUP_LIMIT) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swarm-profiles-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn audit_regression_profile_save_checks_the_revision_on_disk() {
        let dir = folder();
        let path = dir.join("profiles.json");
        let config = built_in();
        let bytes = write(&path, &config).unwrap();
        let expected = revision(&bytes);
        let rename = |config: &mut Config| {
            let lock = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path.with_extension("lock"))
                .unwrap();
            assert!(matches!(
                lock.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            apply_action(config, ProfileAction::New("first"))
        };

        update_at(&path, config.clone(), &bytes, &expected, rename).unwrap();
        let stale = |config: &mut Config| apply_action(config, ProfileAction::New("second"));
        let result = update_at(&path, config, &bytes, &expected, stale);
        let saved = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(result.is_err());
        assert!(saved.profile("first").is_ok());
    }

    #[test]
    fn audit_regression_profile_backups_keep_only_the_newest_ten() {
        let dir = folder();
        let path = dir.join("profiles.json");
        let config = built_in();
        write(&path, &config).unwrap();
        for stamp in 1..=12 {
            std::fs::write(dir.join(format!("profiles.json.{stamp}.bak")), "backup").unwrap();
        }
        std::fs::write(dir.join("profiles.json.manual.bak"), "owner copy").unwrap();

        write(&path, &config).unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(names.contains(&"profiles.json.manual.bak".into()));
        assert!(names.contains(&"profiles.json.12.bak".into()));
        assert!(!names.contains(&"profiles.json.3.bak".into()));
        assert_eq!(names.len(), 12); // Ten backups, the owner copy, and the profile.
    }
}
