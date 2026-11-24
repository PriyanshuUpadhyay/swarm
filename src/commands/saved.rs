//! A saved `/` menu list for Claude and Codex, kept while the files they load it from are unchanged.
//! The probe asks the CLI (0.25 to 0.5 s), so a new chat, a new server, or another chat in the same
//! folder would ask again; a stat walk of the same files takes a few ms. The CLI stays the owner of
//! the loading rules: this module only notices that its inputs moved, and a list older than
//! `MAX_AGE` is read again in case a moved input lies outside the walk.

use super::*;
use sha2::{Digest, Sha256};

const MAX_AGE_SECONDS: u64 = 3600;
/// A walk that meets more entries than this gives no fingerprint, so the CLI is asked.
const MAX_ENTRIES: usize = 20_000;

/// The saved list for this read, or None when there is none or one of its inputs changed.
pub(super) fn load(provider: Provider, env: &BTreeMap<String, String>, cwd: &Path) -> Option<List> {
    let (path, fingerprint) = slot(provider, env, cwd)?;
    let saved: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let fresh = now().saturating_sub(saved["at"].as_u64()?) < MAX_AGE_SECONDS;
    (fresh && saved["fingerprint"] == fingerprint.as_str())
        .then(|| serde_json::from_value(saved["list"].clone()).ok())
        .flatten()
}

/// Saves a clean list; a list with a note is read again next time.
pub(super) fn save(provider: Provider, env: &BTreeMap<String, String>, cwd: &Path, list: &List) {
    if list.note.is_some() {
        return;
    }
    let Some((path, fingerprint)) = slot(provider, env, cwd) else {
        return;
    };
    let text = json!({"fingerprint": fingerprint, "at": now(), "list": list}).to_string();
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_ok()
    {
        let _ = crate::store::write_atomic(&path, &text);
    }
}

/// The file for this provider, folder, and account, and the fingerprint of the list's inputs.
fn slot(
    provider: Provider,
    env: &BTreeMap<String, String>,
    cwd: &Path,
) -> Option<(PathBuf, String)> {
    let key = json!([provider.id(), cwd, env]).to_string();
    let path = crate::paths::root_dir()
        .ok()?
        .join("commands-cache")
        .join(format!("{:x}.json", Sha256::digest(&key)));
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let mut walk = Walk::default();
    walk.text.push_str(&key);
    let (program, roots) = match provider {
        Provider::Claude => ("claude", claude_roots(env, &home, cwd)),
        Provider::Codex => ("codex", codex_roots(env, &home, cwd)),
        _ => return None,
    };
    walk.file(&executable(program, env)?);
    for (root, depth) in roots {
        walk.tree(&root, depth);
    }
    (walk.entries <= MAX_ENTRIES).then(|| (path, format!("{:x}", Sha256::digest(&walk.text))))
}

/// Claude's config folder, its plugins' folders, managed settings, and `.claude` from `cwd` up to
/// the repository root (code.claude.com/docs/en/skills, "Choose where skills load").
fn claude_roots(env: &BTreeMap<String, String>, home: &Path, cwd: &Path) -> Vec<(PathBuf, usize)> {
    let config = env
        .get("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let mut roots = vec![
        (config.join("settings.json"), 0),
        (config.join("settings.local.json"), 0),
        (config.join("skills"), 3),
        (config.join("commands"), 4),
        (config.join("workflows"), 1),
        (config.join("plugins/installed_plugins.json"), 0),
        (config.join("plugins/known_marketplaces.json"), 0),
        (PathBuf::from("/Library/Application Support/ClaudeCode"), 3),
    ];
    let installed = std::fs::read(config.join("plugins/installed_plugins.json"))
        .ok()
        .and_then(|text| serde_json::from_slice::<Value>(&text).ok());
    let plugins = installed
        .as_ref()
        .and_then(|value| value["plugins"].as_object());
    for install in plugins
        .into_iter()
        .flat_map(|map| map.values())
        .flat_map(|v| v.as_array().into_iter().flatten())
    {
        if let Some(dir) = install["installPath"].as_str() {
            let dir = PathBuf::from(dir);
            roots.extend([
                (dir.join(".claude-plugin"), 1),
                (dir.join("skills"), 3),
                (dir.join("commands"), 4),
            ]);
        }
    }
    for folder in project_folders(cwd) {
        let dot = folder.join(".claude");
        roots.extend([
            (dot.join("settings.json"), 0),
            (dot.join("settings.local.json"), 0),
            (dot.join("skills"), 3),
            (dot.join("commands"), 4),
            (dot.join("workflows"), 1),
        ]);
    }
    roots
}

/// Codex's home, the user's and admin's skill folders, and `.agents/skills` from `cwd` up to the
/// repository root.
fn codex_roots(env: &BTreeMap<String, String>, home: &Path, cwd: &Path) -> Vec<(PathBuf, usize)> {
    let codex_home = env
        .get("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
        .unwrap_or_else(|| home.join(".codex"));
    let mut roots = vec![
        (codex_home.join("config.toml"), 0),
        (codex_home.join("skills"), 6),
        (codex_home.join("plugins"), 7),
        (home.join(".agents/skills"), 6),
        (PathBuf::from("/etc/codex/skills"), 6),
    ];
    for folder in project_folders(cwd) {
        roots.push((folder.join(".agents/skills"), 6));
    }
    roots
}

/// `cwd` and its parents up to the folder that holds `.git`, or `cwd` alone outside a repository.
fn project_folders(cwd: &Path) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for folder in cwd.ancestors() {
        folders.push(folder.to_path_buf());
        if folder.join(".git").exists() {
            return folders;
        }
    }
    folders.truncate(1);
    folders
}

/// The CLI that the probe starts: an override variable, or the first match on PATH.
fn executable(program: &str, env: &BTreeMap<String, String>) -> Option<PathBuf> {
    let variable = format!("SWARM_{}_CMD", program.to_uppercase());
    if let Ok(path) = std::env::var(&variable) {
        return Some(PathBuf::from(path));
    }
    let path = env
        .get("PATH")
        .cloned()
        .or_else(|| std::env::var("PATH").ok())?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

/// The names, sizes, and change times of the walked files, as one text to hash.
#[derive(Default)]
struct Walk {
    text: String,
    entries: usize,
}

impl Walk {
    fn file(&mut self, path: &Path) {
        // `metadata` follows symlinks, so a linked skill counts by its target, and a moved link
        // target counts as a change of the link's own path.
        let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let stamp = std::fs::metadata(&target).ok().map(|meta| {
            use std::os::unix::fs::MetadataExt;
            (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.ctime())
        });
        self.text.push_str(&format!(
            "\n{}>{}:{stamp:?}",
            path.display(),
            target.display()
        ));
        self.entries += 1;
    }

    fn tree(&mut self, path: &Path, depth: usize) {
        self.file(path);
        if depth == 0 || self.entries > MAX_ENTRIES || !path.is_dir() {
            return;
        }
        let mut names: Vec<_> = std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        names.sort();
        for child in names {
            self.tree(&child, depth - 1);
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}
