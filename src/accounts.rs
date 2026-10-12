use crate::providers::Provider;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

pub(crate) const BUNDLED: &str = "version = 1\n";

#[derive(Clone, Debug)]
pub struct Entry {
    pub provider: Provider,
    pub name: String,
    pub home: PathBuf,
}

pub(crate) struct Metadata {
    pub accounts: Vec<Entry>,
    pub revision: String,
    before: String,
}

fn path() -> Result<PathBuf, String> {
    crate::paths::root_dir()
        .map(|root| root.join("accounts.toml"))
        .map_err(|error| error.to_string())
}

fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let invalid = || "Swarm account metadata is invalid".to_string();
    let document: DocumentMut = text.parse().map_err(|_| invalid())?;
    if document.get("version").and_then(Item::as_integer) != Some(1)
        || document
            .iter()
            .any(|(key, _)| !["version", "accounts"].contains(&key))
    {
        return Err(invalid());
    }
    let mut accounts = Vec::new();
    let mut names = BTreeSet::new();
    if let Some(item) = document.get("accounts") {
        let entries = item.as_array_of_tables().ok_or_else(invalid)?;
        for entry in entries {
            if entry.len() != 3
                || entry
                    .iter()
                    .any(|(key, _)| !["provider", "name", "home"].contains(&key))
            {
                return Err(invalid());
            }
            let provider = entry
                .get("provider")
                .and_then(Item::as_str)
                .and_then(Provider::parse)
                .filter(|provider| provider.has_accounts())
                .ok_or_else(invalid)?;
            let name = entry
                .get("name")
                .and_then(Item::as_str)
                .filter(|name| crate::config::valid_account_name(name))
                .ok_or_else(invalid)?;
            let home = PathBuf::from(
                entry
                    .get("home")
                    .and_then(Item::as_str)
                    .ok_or_else(invalid)?,
            );
            if !home.is_absolute() || !names.insert((provider.id(), name.to_string())) {
                return Err(invalid());
            }
            accounts.push(Entry {
                provider,
                name: name.into(),
                home,
            });
        }
    }
    Ok(accounts)
}

fn load_at(path: &Path) -> Result<Metadata, String> {
    let before = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && std::fs::symlink_metadata(path).is_err() =>
        {
            String::new()
        }
        Err(_) => return Err("Cannot read Swarm account metadata".into()),
    };
    let effective = if before.is_empty() && !path.exists() {
        BUNDLED
    } else {
        &before
    };
    Ok(Metadata {
        accounts: parse(effective)?,
        revision: crate::config::revision(effective.as_bytes()),
        before,
    })
}

pub(crate) fn load() -> Result<Metadata, String> {
    load_at(&path()?)
}

fn encode(accounts: &[Entry]) -> String {
    let mut document = DocumentMut::new();
    document["version"] = value(1);
    if !accounts.is_empty() {
        let mut tables = ArrayOfTables::new();
        for entry in accounts {
            let mut table = Table::new();
            table["provider"] = value(entry.provider.id());
            table["name"] = value(&entry.name);
            table["home"] = value(entry.home.to_string_lossy().as_ref());
            tables.push(table);
        }
        document["accounts"] = Item::ArrayOfTables(tables);
    }
    document.to_string()
}

fn update(
    expected: &str,
    change: impl FnOnce(&mut Vec<Entry>) -> Result<bool, String>,
) -> Result<String, String> {
    let path = path()?;
    crate::managed::with_lock(&path.with_extension("lock"), || {
        let mut metadata = load_at(&path)?;
        if metadata.revision != expected {
            return Err("Swarm accounts changed on disk; reload and try again".into());
        }
        if !change(&mut metadata.accounts)? {
            return Ok(metadata.revision);
        }
        let text = encode(&metadata.accounts);
        crate::managed::write_text(&path, &metadata.before, &text)?;
        Ok(crate::config::revision(text.as_bytes()))
    })
}

/// Allocate a private account home only when the supplied revision still matches.
/// Registration stays on disk if the later pane open or login fails.
/// A name that already has a home in `$HOME` keeps it: an entry that an older build wrote, or a
/// profile found by reference. A new home there would hide the login the owner already has.
pub fn register(provider: Provider, name: &str, expected: &str) -> Result<(Entry, String), String> {
    if !provider.has_accounts() {
        return Err("This provider has no Swarm account source".into());
    }
    if !crate::config::valid_account_name(name) {
        return Err("Invalid account name".into());
    }
    let root = crate::paths::root_dir().map_err(|error| error.to_string())?;
    let root = std::fs::canonicalize(root).map_err(|_| "Swarm home is unavailable")?;
    let home = root.join("accounts").join(provider.id()).join(name);
    home.to_str().ok_or("Account home path must be UTF-8")?;
    let older = std::env::var_os("HOME")
        .and_then(|home| std::fs::canonicalize(home).ok())
        .map(|home| match provider {
            Provider::Codex => home.join(format!(".codex-{name}")),
            _ => home.join(".claude/.profiles").join(name),
        })
        .filter(|older| older.to_str().is_some());
    let mut entry = Entry {
        provider,
        name: name.into(),
        home,
    };
    let revision = update(expected, |accounts| {
        let registered = accounts
            .iter()
            .find(|account| account.provider == provider && account.name == name)
            .map(|account| account.home.clone());
        let add = |accounts: &mut Vec<Entry>, entry: &Entry| {
            accounts.push(entry.clone());
            accounts.sort_by(|left, right| {
                (left.provider.id(), &left.name).cmp(&(right.provider.id(), &right.name))
            });
        };
        match (&registered, older) {
            (Some(found), Some(older)) if *found == older => {
                entry.home = older;
                return Ok(false);
            }
            (Some(found), _) if *found != entry.home => {
                return Err("This account name is already registered".into());
            }
            (None, Some(older)) if older.is_dir() => {
                entry.home = older;
                add(accounts, &entry);
                return Ok(true);
            }
            _ => (),
        }
        // A registered name comes here too, so a removed folder or an open mode is repaired.
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        for directory in [
            root.join("accounts"),
            root.join("accounts").join(provider.id()),
            entry.home.clone(),
        ] {
            match std::fs::symlink_metadata(&directory) {
                Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => {
                    return Err("Account home must be a directory, not a symlink".into());
                }
                Ok(metadata)
                    if directory == entry.home
                        && metadata.permissions().mode() & 0o777 != 0o700 =>
                {
                    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                        .map_err(|_| "Cannot make account home private")?;
                }
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::DirBuilder::new()
                        .mode(0o700)
                        .create(&directory)
                        .map_err(|_| "Cannot create account home")?;
                }
                Err(_) => return Err("Cannot read account home".into()),
            }
        }
        if registered.is_some() {
            return Ok(false);
        }
        add(accounts, &entry);
        Ok(true)
    })?;
    Ok((entry, revision))
}

/// Reset only the Swarm overlay when its revision matches.
/// Provider homes, credentials and usage caches are untouched.
pub fn reset(expected: &str) -> Result<String, String> {
    update(expected, |accounts| {
        accounts.clear();
        Ok(true)
    })
}

pub(crate) fn merge(
    provider: Provider,
    native: Vec<(String, PathBuf)>,
    metadata: &[Entry],
) -> Vec<(String, PathBuf)> {
    // The native default keeps its name; owner references outrank other discovered names.
    let (defaults, others): (Vec<_>, Vec<_>) =
        native.into_iter().partition(|(name, _)| name == "default");
    let entries = metadata
        .iter()
        .filter(|entry| entry.provider == provider)
        .map(|entry| (entry.name.clone(), entry.home.clone()));
    let mut names = BTreeSet::new();
    let mut homes = BTreeSet::new();
    defaults
        .into_iter()
        .chain(entries)
        .chain(others)
        .filter_map(|(name, path)| {
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if names.contains(&name) || homes.contains(&path) {
                return None;
            }
            names.insert(name.clone());
            homes.insert(path.clone());
            Some((name, path))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_accepts_only_native_nonsecret_references() {
        assert!(parse(BUNDLED).unwrap().is_empty());
        let work =
            "version = 1\n[[accounts]]\nprovider = 'codex'\nname = 'work'\nhome = '/tmp/work'\n";
        assert_eq!(parse(work).unwrap()[0].home, PathBuf::from("/tmp/work"));
        for text in [
            "",
            "version = 2\n",
            "version = '1'\n",
            "version = 1\naccounts = []\n",
        ] {
            assert!(parse(text).is_err());
        }
        for text in [
            work.replace("'/tmp/work'", "'relative'"),
            work.replace("'codex'", "'agy'"),
            work.replace("'work'", "'default'"),
            format!("{work}token = 'secret'\n"),
            format!("{work}{}", work.replace("version = 1\n", "")),
        ] {
            assert!(parse(&text).is_err());
        }
    }

    #[test]
    fn metadata_names_keep_the_native_default_and_deduplicate_active_references() {
        let metadata = [
            Entry {
                provider: Provider::Codex,
                name: "work".into(),
                home: "/tmp/external-native-home".into(),
            },
            Entry {
                provider: Provider::Codex,
                name: "personal".into(),
                home: "/tmp/default-native-home".into(),
            },
        ];
        let native = vec![
            ("default".into(), "/tmp/default-native-home".into()),
            ("current".into(), "/tmp/external-native-home".into()),
            ("work".into(), "/tmp/old-native-home".into()),
        ];
        assert_eq!(
            merge(Provider::Codex, native, &metadata),
            vec![
                ("default".into(), "/tmp/default-native-home".into()),
                ("work".into(), "/tmp/external-native-home".into())
            ]
        );
    }
}
