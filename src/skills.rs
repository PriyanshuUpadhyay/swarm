use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const KIT_COMMIT: &str = "ec8b85f0953292c72853a3428a2ee395077ff8c3";
const TASTE_COMMIT: &str = "ce26fc25c0e5e8cab638f883de62d9a86ee5e45b";
const EMIL_COMMIT: &str = "e8a175de22ae1e49370fc144c1f3bb9aeedf988d";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillLink {
    pub name: String,
    pub path: String,
}

pub fn link_catalog() -> Vec<SkillLink> {
    let mut catalog: Vec<_> = [
        "cleanup-gate",
        "council",
        "decisions",
        "deliver",
        "engineering-standards",
        "flow",
        "ink",
        "minimize-reader-load",
        "orchestrate-agy",
        "orchestrate-claude",
        "orchestrate-codex",
        "pair",
        "prove-it-works",
        "research",
        "review-check",
        "review-walk",
        "sequence-verifiable-units",
        "web-search",
    ]
    .into_iter()
    .map(|name| SkillLink {
        name: name.into(),
        path: format!("kit/skills/{name}"),
    })
    .collect();
    for (name, path) in [
        ("swarm-orchestrator", "swarm-orchestrator"),
        ("swarm-voice", "swarm-voice"),
        ("taste-skill", "kit/vendor/taste-skill/skills/taste-skill"),
        ("animate", "kit/vendor/emil-skills/skills/animate"),
        ("break-ui", "kit/vendor/emil-skills/skills/break-ui"),
    ] {
        catalog.push(SkillLink {
            name: name.into(),
            path: path.into(),
        });
    }
    catalog.sort_by(|a, b| a.name.cmp(&b.name));
    catalog
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillFile {
    pub path: String,
    pub mode: u32,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillDirectory {
    pub path: String,
    pub mode: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub content_id: String,
    pub kit_commit: String,
    pub vendor_commits: BTreeMap<String, String>,
    pub catalog: Vec<SkillLink>,
    pub files: Vec<SkillFile>,
    pub directories: Vec<SkillDirectory>,
}

fn relative_path(value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part.starts_with(".git"))
        || Path::new(value)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("skills: invalid relative path {value:?}").into());
    }
    Ok(())
}

fn files_in(root: &Path, directory: &Path, paths: &mut Vec<(String, bool)>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("skills: non-UTF-8 path")?;
        if name.starts_with(".git")
            || name == "__pycache__"
            || name.ends_with(".pyc")
            || (directory == root && name == "manifest.json")
        {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type()?;
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .ok_or("skills: non-UTF-8 path")?;
        relative_path(relative)?;
        if kind.is_dir() {
            paths.push((relative.into(), true));
            files_in(root, &path, paths)?;
        } else if kind.is_file() {
            paths.push((relative.into(), false));
        } else {
            return Err(format!(
                "skills: source must contain only plain files and directories: {}",
                path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn tree_content(root: &Path) -> Result<(Vec<SkillFile>, Vec<SkillDirectory>, String)> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err("skills: source must be a plain directory".into());
    }
    let mut paths = Vec::new();
    files_in(root, root, &mut paths)?;
    paths.sort();
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut content = Sha256::new();
    for (path, is_directory) in paths {
        let full = root.join(&path);
        let mode = fs::metadata(&full)?.permissions().mode() & 0o777;
        let bytes = if is_directory {
            Vec::new()
        } else {
            fs::read(&full)?
        };
        // Lengths frame the path and bytes so distinct records cannot share an encoding.
        content.update([u8::from(is_directory)]);
        content.update((path.len() as u64).to_be_bytes());
        content.update(path.as_bytes());
        content.update(mode.to_be_bytes());
        content.update((bytes.len() as u64).to_be_bytes());
        content.update(&bytes);
        if is_directory {
            directories.push(SkillDirectory { path, mode });
        } else {
            files.push(SkillFile {
                path,
                mode,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
            });
        }
    }
    Ok((files, directories, format!("{:x}", content.finalize())))
}

impl Manifest {
    pub fn from_tree(root: &Path) -> Result<Self> {
        let (files, directories, content_id) = tree_content(root)?;
        let manifest = Self {
            schema: 1,
            content_id,
            kit_commit: KIT_COMMIT.into(),
            vendor_commits: BTreeMap::from([
                ("taste-skill".into(), TASTE_COMMIT.into()),
                ("emil-skills".into(), EMIL_COMMIT.into()),
            ]),
            catalog: link_catalog(),
            files,
            directories,
        };
        manifest.validate(root)?;
        Ok(manifest)
    }

    pub fn read(root: &Path) -> Result<Self> {
        if !fs::symlink_metadata(root.join("manifest.json"))?.is_file() {
            return Err("skills: manifest must be a plain file".into());
        }
        let manifest: Self = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
        manifest.validate(root)?;
        Ok(manifest)
    }

    pub fn validate(&self, root: &Path) -> Result<()> {
        if self.schema != 1 {
            return Err(format!("skills: unsupported manifest schema {}", self.schema).into());
        }
        let mut paths = BTreeSet::new();
        for file in &self.files {
            relative_path(&file.path)?;
            if !paths.insert(&file.path) {
                return Err(format!("skills: duplicate file path {}", file.path).into());
            }
        }
        for directory in &self.directories {
            relative_path(&directory.path)?;
            if !paths.insert(&directory.path) {
                return Err(format!("skills: duplicate path {}", directory.path).into());
            }
        }
        let mut names = BTreeSet::new();
        for link in &self.catalog {
            relative_path(&link.name)?;
            relative_path(&link.path)?;
            if link.name.contains('/') || !names.insert(&link.name) {
                return Err(
                    format!("skills: duplicate or invalid skill name {}", link.name).into(),
                );
            }
            if !self
                .files
                .iter()
                .any(|file| file.path == format!("{}/SKILL.md", link.path))
            {
                return Err(format!("skills: missing catalog SKILL.md for {}", link.name).into());
            }
        }
        if self.catalog != link_catalog() {
            return Err("skills: manifest catalog differs from the link catalog".into());
        }
        let (files, directories, content_id) = tree_content(root)?;
        if self.files != files || self.directories != directories || self.content_id != content_id {
            return Err("skills: manifest paths, modes or bytes do not match the source".into());
        }
        Ok(())
    }
}

pub fn write_manifest(source: &Path, out: &Path) -> Result<()> {
    let manifest = Manifest::from_tree(source)?;
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    fs::write(out, bytes)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshOutcome {
    Updated,
    Unchanged,
}

// O_NOFOLLOW prevents the lock or a source file from following a link at its final component.
#[cfg(target_os = "macos")]
const NOFOLLOW: i32 = 0x100;
#[cfg(not(target_os = "macos"))]
const NOFOLLOW: i32 = 0x20000;

fn source_from_helper(helper: &Path) -> Result<std::path::PathBuf> {
    let helper = helper.canonicalize().map_err(|error| {
        format!(
            "skills: cannot resolve helper {}: {error}",
            helper.display()
        )
    })?;
    let contents = helper
        .parent()
        .and_then(Path::parent)
        .ok_or("skills: helper has no bundle parent")?;
    let source = contents.join("Resources/Skills");
    let resolved = source.canonicalize().map_err(|error| {
        format!(
            "skills: missing bundle source {}: {error}",
            source.display()
        )
    })?;
    if !resolved.starts_with(contents) {
        return Err(format!("skills: bundle source escapes {}", contents.display()).into());
    }
    Ok(source)
}

fn remove_leaf(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn complete(path: &Path) -> Option<Manifest> {
    if fs::symlink_metadata(path).ok()?.is_dir() {
        Manifest::read(path).ok()
    } else {
        None
    }
}

fn write_version(root: &Path, content_id: &str) -> Result<()> {
    use std::io::Write;
    let temporary = root.join(".skills-version-next");
    remove_leaf(&temporary)?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(format!("{content_id}\n").as_bytes())?;
    file.sync_all()?;
    fs::rename(&temporary, root.join("skills-version"))?;
    fs::File::open(root)?.sync_all()?;
    Ok(())
}

fn recover(root: &Path) -> Result<()> {
    let destination = root.join("skills");
    let previous = root.join(".skills-previous");
    let stage = root.join(".skills-stage");
    if fs::symlink_metadata(&previous).is_ok() {
        if let Some(manifest) = complete(&destination) {
            write_version(root, &manifest.content_id)?;
            remove_leaf(&previous)?;
        } else if let Some(manifest) = complete(&previous) {
            remove_leaf(&destination)?;
            fs::rename(&previous, &destination)?;
            write_version(root, &manifest.content_id)?;
        } else if let Some(manifest) = complete(&stage) {
            remove_leaf(&destination)?;
            fs::rename(&stage, &destination)?;
            write_version(root, &manifest.content_id)?;
            remove_leaf(&previous)?;
        } else {
            remove_leaf(&previous)?;
        }
    }
    if complete(&destination).is_none()
        && let Some(manifest) = complete(&stage)
    {
        remove_leaf(&destination)?;
        fs::rename(&stage, &destination)?;
        write_version(root, &manifest.content_id)?;
    }
    remove_leaf(&stage)?;
    remove_leaf(&root.join(".skills-version-next"))?;
    Ok(())
}

fn stage_tree(source: &Path, stage: &Path, manifest: &Manifest) -> Result<()> {
    fs::create_dir(stage)?;
    for directory in &manifest.directories {
        fs::create_dir_all(stage.join(&directory.path))?;
    }
    for entry in &manifest.files {
        let mut input = fs::OpenOptions::new()
            .read(true)
            .custom_flags(NOFOLLOW)
            .open(source.join(&entry.path))?;
        if !input.metadata()?.is_file() {
            return Err(format!("skills: source is not a plain file: {}", entry.path).into());
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.join(&entry.path))?;
        std::io::copy(&mut input, &mut output)?;
        output.set_permissions(fs::Permissions::from_mode(entry.mode))?;
        output.sync_all()?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("manifest.json"))?;
    let mut input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW)
        .open(source.join("manifest.json"))?;
    std::io::copy(&mut input, &mut file)?;
    file.set_permissions(fs::Permissions::from_mode(
        input.metadata()?.permissions().mode() & 0o777,
    ))?;
    file.sync_all()?;
    for directory in manifest.directories.iter().rev() {
        fs::set_permissions(
            stage.join(&directory.path),
            fs::Permissions::from_mode(directory.mode),
        )?;
        fs::File::open(stage.join(&directory.path))?.sync_all()?;
    }
    manifest.validate(stage)?;
    fs::File::open(stage)?.sync_all()?;
    Ok(())
}

/// Refresh from a bundled helper into an already selected home; callers own home selection.
/// Kept explicit so fixtures can exercise cask links and separate homes without global env writes.
pub fn refresh_from(helper: &Path, root: &Path) -> Result<RefreshOutcome> {
    let source = source_from_helper(helper);
    fs::create_dir_all(root)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(NOFOLLOW)
        .open(root.join(".skills-lock"))?;
    lock.try_lock()
        .map_err(|error| format!("skills: cannot take home refresh lock: {error}"))?;
    recover(root)?;
    let source = source?;
    let manifest = Manifest::read(&source).map_err(|error| {
        format!(
            "skills: invalid bundle source {}: {error}",
            source.display()
        )
    })?;
    let destination = root.join("skills");
    if complete(&destination).is_some_and(|installed| installed.content_id == manifest.content_id) {
        use std::io::Read;
        let mut version = String::new();
        let matches = fs::OpenOptions::new()
            .read(true)
            .custom_flags(NOFOLLOW)
            .open(root.join("skills-version"))
            .and_then(|mut file| file.read_to_string(&mut version))
            .is_ok()
            && version.trim() == manifest.content_id;
        if !matches {
            write_version(root, &manifest.content_id)?;
        }
        return Ok(RefreshOutcome::Unchanged);
    }
    let stage = root.join(".skills-stage");
    if let Err(error) = stage_tree(&source, &stage, &manifest) {
        let _ = remove_leaf(&stage);
        return Err(format!("skills: cannot stage bundle: {error}").into());
    }
    let previous = root.join(".skills-previous");
    let had_previous = fs::symlink_metadata(&destination).is_ok();
    if had_previous {
        fs::rename(&destination, &previous)?;
    }
    if let Err(error) =
        fs::rename(&stage, &destination).and_then(|()| fs::File::open(root)?.sync_all())
    {
        if had_previous {
            if fs::symlink_metadata(&destination).is_ok() {
                remove_leaf(&destination)?;
            }
            fs::rename(&previous, &destination)?;
        }
        return Err(format!("skills: cannot promote bundle: {error}").into());
    }
    if let Err(error) = write_version(root, &manifest.content_id) {
        remove_leaf(&destination)?;
        if had_previous {
            fs::rename(&previous, &destination)?;
        }
        return Err(format!("skills: cannot write version after promotion: {error}").into());
    }
    remove_leaf(&previous)?;
    Ok(RefreshOutcome::Updated)
}

pub fn refresh() -> Result<RefreshOutcome> {
    let destination = crate::paths::skills_dir()?;
    refresh_from(
        &std::env::current_exe()?,
        destination.parent().ok_or("skills: home has no parent")?,
    )
}
