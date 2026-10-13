//! Workspace and project rules shared by the guarded HTTP actions.
use super::{
    choices::{OwnerChoices, ViewState},
    discovery,
    tree::{Project, Tree, Workspace},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug)]
pub enum Error {
    Forbidden,
    Invalid(String),
    Failed(String),
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Forbidden => f.write_str("path is outside the allowed project or workspace"),
            Self::Invalid(message) | Self::Failed(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewWorkspace {
    pub project: String,
    pub name: String,
    pub folder: Option<String>,
    pub prefix: Option<String>,
    pub mode: Mode,
    pub base: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<u64>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    NewBranch,
    ExistingBranch,
    Pr,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProject {
    pub path: String,
    pub name: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportProject {
    pub path: String,
    #[serde(rename = "as")]
    pub kind: Option<ImportAs>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportAs {
    Git,
    Folder,
}

pub fn checked_path(root: &Path, path: &str, existing: bool) -> Result<PathBuf> {
    let path = Path::new(path);
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(Error::Forbidden);
    }
    let path = normalize(path);
    let mut ancestor = path.as_path();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        if std::fs::symlink_metadata(ancestor).is_ok() {
            return Err(Error::Forbidden);
        }
        suffix.push(ancestor.file_name().ok_or(Error::Forbidden)?);
        ancestor = ancestor.parent().ok_or(Error::Forbidden)?;
    }
    if existing && !suffix.is_empty() {
        return Err(Error::Forbidden);
    }
    let mut resolved = std::fs::canonicalize(ancestor).map_err(|_| Error::Forbidden)?;
    if !resolved.is_dir() {
        return Err(Error::Forbidden);
    }
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    let home = std::fs::canonicalize(root).map_err(|_| Error::Forbidden)?;
    if resolved.starts_with(&home) {
        return Err(Error::Forbidden);
    }
    Ok(resolved)
}
fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            _ => result.push(part.as_os_str()),
        }
    }
    result
}
pub fn project<'a>(tree: &'a Tree, path: &str) -> Result<&'a Project> {
    if !Path::new(path).is_absolute() {
        return Err(Error::Forbidden);
    }
    tree.projects
        .iter()
        .find(|p| p.path == path)
        .ok_or(Error::Forbidden)
}
pub fn workspace<'a>(tree: &'a Tree, path: &str) -> Result<(&'a Project, &'a Workspace)> {
    if !Path::new(path).is_absolute() {
        return Err(Error::Forbidden);
    }
    tree.projects
        .iter()
        .find_map(|p| p.workspaces.iter().find(|w| w.path == path).map(|w| (p, w)))
        .ok_or(Error::Forbidden)
}
pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "task".into()
    } else {
        slug.chars().take(40).collect()
    }
}
pub fn branch_name(name: &str, prefix: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    let branch = format!("{}{}", prefix.trim(), slug(name));
    if branch.starts_with('-')
        || branch.ends_with('.')
        || branch.contains("..")
        || branch.contains("@{")
        || branch
            .chars()
            .any(|c| c < ' ' || c == '\u{7f}' || " ~^:?*[\\".contains(c))
        || branch
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
    {
        None
    } else {
        Some(branch)
    }
}
pub fn git(path: &str, args: &[&str]) -> Result<Output> {
    let mut command = Command::new("git");
    command
        .args(["-C", path])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");
    discovery::run_command(
        &mut command,
        if args.first() == Some(&"fetch") {
            60
        } else {
            10
        },
    )
    .map_err(Error::Failed)
}
fn output(output: Output) -> Result<String> {
    if !output.status.success() {
        let message = if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        };
        return Err(Error::Failed(
            String::from_utf8_lossy(message).trim().into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}
pub fn check_git(path: &str, args: &[&str]) -> Result<String> {
    output(git(path, args)?)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct References {
    pub default_branch: Option<String>,
    pub local: Vec<String>,
    pub remote: Vec<String>,
    pub held: BTreeSet<String>,
    pub branches: Vec<String>,
    pub bases: Vec<String>,
}
fn has_ref(path: &str, reference: &str) -> Result<bool> {
    let out = git(path, &["show-ref", "--verify", "--quiet", reference])?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => output(out).map(|_| false),
    }
}
fn default_reference(path: &str) -> Result<Option<String>> {
    let remote = git(
        path,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    )?;
    if remote.status.success() {
        let remote = output(remote)?;
        if let Some(name) = remote.strip_prefix("refs/remotes/origin/") {
            let local = format!("refs/heads/{name}");
            return Ok(Some(if has_ref(path, &local)? {
                local
            } else {
                remote
            }));
        }
    }
    for branch in ["main", "master"] {
        let reference = format!("refs/heads/{branch}");
        if has_ref(path, &reference)? {
            return Ok(Some(reference));
        }
    }
    let head = git(path, &["symbolic-ref", "--quiet", "HEAD"])?;
    if head.status.success() {
        let head = output(head)?;
        if has_ref(path, &head)? {
            return Ok(Some(head));
        }
    }
    let head = git(path, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?;
    if head.status.code() == Some(1) {
        return Ok(None);
    }
    output(head).map(Some)
}
pub fn references(common: &str) -> Result<References> {
    let raw = check_git(
        common,
        &[
            "for-each-ref",
            "--format=%(refname:lstrip=2)%00%(refname)",
            "refs/heads",
            "refs/remotes/origin",
        ],
    )?;
    let default_branch: Option<String> = default_reference(common)?.map(|s| {
        s.strip_prefix("refs/heads/")
            .or_else(|| s.strip_prefix("refs/remotes/"))
            .unwrap_or(&s)
            .into()
    });
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for line in raw.lines() {
        if let Some((name, reference)) = line.split_once('\0') {
            if reference.starts_with("refs/heads/") {
                local.push(name.into());
            } else if reference != "refs/remotes/origin/HEAD" {
                remote.push(name.into());
            }
        }
    }
    for list in [&mut local, &mut remote] {
        list.sort_by_key(|b| Some(b) != default_branch.as_ref());
    }
    let listing = git(common, &["worktree", "list", "--porcelain", "-z"])?;
    if !listing.status.success() {
        output(listing)?;
        unreachable!();
    }
    let held: BTreeSet<_> = discovery::parse_worktrees(&listing.stdout)
        .into_iter()
        .filter_map(|w| w.branch)
        .collect();
    let branches = local
        .iter()
        .filter(|b| !held.contains(*b))
        .chain(remote.iter().filter(|b| {
            let name = b.strip_prefix("origin/").unwrap_or(b);
            !local.iter().any(|b| b == name) && !held.contains(name)
        }))
        .cloned()
        .collect();
    let mut bases: Vec<String> = default_branch.clone().into_iter().collect();
    bases.extend(
        local
            .iter()
            .chain(&remote)
            .filter(|b| Some(*b) != default_branch.as_ref())
            .cloned(),
    );
    Ok(References {
        default_branch,
        local,
        remote,
        held,
        branches,
        bases,
    })
}
pub fn common(project: &Project) -> Option<&str> {
    project.id.strip_prefix("repository:")
}
pub fn defaults(project: &Project, owner: &OwnerChoices) -> (String, String, String) {
    let seed = if common(project)
        .is_some_and(|c| Path::new(c).file_name().is_some_and(|n| n == ".bare"))
    {
        "wt/".into()
    } else {
        format!("../{}-worktrees", project.name)
    };
    let settings = owner
        .project_defaults
        .get(&project.path)
        .unwrap_or(&Value::Null);
    let folder = settings["worktreeFolder"]
        .as_str()
        .unwrap_or(&seed)
        .trim()
        .to_owned();
    let prefix = settings["branchPrefix"]
        .as_str()
        .unwrap_or("swarm/")
        .trim()
        .to_owned();
    (folder, prefix, seed)
}
pub fn options(project: &Project, owner: &OwnerChoices) -> Result<Value> {
    let common = common(project)
        .ok_or_else(|| Error::Invalid("New Workspace needs a Git project".into()))?;
    let mut value =
        serde_json::to_value(references(common)?).map_err(|e| Error::Failed(e.to_string()))?;
    let (folder, prefix, seed) = defaults(project, owner);
    value["folder"] = folder.into();
    value["prefix"] = prefix.into();
    value["seedFolder"] = seed.into();
    value["seedPrefix"] = "swarm/".into();
    Ok(value)
}
pub fn create_workspace(
    root: &Path,
    project: &Project,
    owner: &OwnerChoices,
    request: &NewWorkspace,
) -> Result<(String, Value)> {
    let common = common(project)
        .ok_or_else(|| Error::Invalid("New Workspace needs a Git project".into()))?;
    checked_path(root, &project.path, true)?;
    checked_path(root, common, true)?;
    let (folder, prefix, seed) = defaults(project, owner);
    let prefix = request.prefix.as_deref().unwrap_or(&prefix).trim();
    let branch = branch_name(&request.name, prefix).ok_or_else(|| {
        Error::Invalid("Enter a name and prefix that make a valid Git branch".into())
    })?;
    let folder_setting = request.folder.as_deref().unwrap_or(&folder).trim();
    if folder_setting.is_empty() {
        return Err(Error::Invalid("Enter a worktree folder".into()));
    }
    let folder = if let Some(suffix) = folder_setting.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").ok_or(Error::Forbidden)?).join(suffix)
    } else {
        Path::new(&project.path).join(folder_setting)
    };
    let parent = checked_path(root, &folder.to_string_lossy(), false)?;
    let target = parent.join(slug(&request.name));
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(Error::Invalid(format!(
            "A folder already exists at {}",
            target.display()
        )));
    }
    let path = target.to_str().ok_or(Error::Forbidden)?;
    let refs = references(common)?;
    let mut args = vec!["worktree", "add"];
    let expected;
    match request.mode {
        Mode::NewBranch => {
            let base = request
                .base
                .as_deref()
                .unwrap_or_else(|| refs.bases.first().map(String::as_str).unwrap_or(""));
            if refs.default_branch.is_none() {
                if !base.is_empty() {
                    return Err(Error::Invalid(
                        "The first workspace starts without a base".into(),
                    ));
                }
                args.extend(["--orphan", "-b", &branch, "--", path]);
            } else {
                if !refs.bases.iter().any(|b| b == base) {
                    return Err(Error::Invalid("Select a known base branch".into()));
                }
                args.extend(["-b", &branch, "--", path, base]);
            }
            expected = branch.clone();
        }
        Mode::ExistingBranch => {
            let existing = request.branch.as_deref().unwrap_or("");
            if refs.default_branch.is_none() || !refs.branches.iter().any(|b| b == existing) {
                return Err(Error::Invalid("Select a free existing branch".into()));
            }
            expected = existing
                .strip_prefix("origin/")
                .unwrap_or(existing)
                .to_owned();
            if expected != existing {
                args.extend(["--track", "-b", &expected, "--", path, existing]);
            } else {
                args.extend(["--", path, existing]);
            }
        }
        Mode::Pr => {
            let number = request
                .pr
                .filter(|n| *n > 0 && *n <= i64::MAX as u64)
                .ok_or_else(|| Error::Invalid("Enter a positive pull request number".into()))?;
            if refs.default_branch.is_none() {
                return Err(Error::Invalid(
                    "The first workspace must start a new branch".into(),
                ));
            }
            let mut command = Command::new("gh");
            command
                .current_dir(common)
                .args(["pr", "view", &number.to_string(), "--json", "number"])
                .env("GH_PROMPT_DISABLED", "1");
            let pr = output(discovery::run_command(&mut command, 20).map_err(Error::Failed)?)?;
            let pr: Value = serde_json::from_str(&pr).map_err(|e| Error::Failed(e.to_string()))?;
            if pr["number"].as_u64() != Some(number) {
                return Err(Error::Invalid(
                    "GitHub returned a different pull request".into(),
                ));
            }
            check_git(
                common,
                &["fetch", "origin", &format!("refs/pull/{number}/head")],
            )?;
            args.extend(["-b", &branch, "--", path, "FETCH_HEAD"]);
            expected = branch.clone();
        }
    }
    std::fs::create_dir_all(&parent)?;
    check_git(common, &args)?;
    let listing = git(common, &["worktree", "list", "--porcelain", "-z"])?;
    if !listing.status.success() {
        output(listing)?;
        unreachable!();
    }
    let created = discovery::parse_worktrees(&listing.stdout)
        .into_iter()
        .find(|w| w.path == path && w.branch.as_deref() == Some(&expected))
        .ok_or_else(|| {
            Error::Failed(format!(
                "Git created {expected}, but did not list its worktree"
            ))
        })?;
    let settings = json!({"worktreeFolder": if folder_setting == seed {None} else {Some(folder_setting)},
        "branchPrefix": if prefix == "swarm/" {None} else {Some(prefix)}});
    Ok((created.path, settings))
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "state", content = "reason", rename_all = "kebab-case")]
pub enum FirstCommit {
    Made,
    SkippedNoIdentity,
    Failed(String),
}
impl FirstCommit {
    pub fn notice(&self) -> Option<String> {
        match self {
            Self::Made => None,
            Self::SkippedNoIdentity => Some("Created without a first commit, because git has no user.name and user.email. Set them, then commit once; until then workspaces start orphan branches.".into()),
            Self::Failed(reason) => Some(format!("Created without a first commit. {reason}")),
        }
    }
}
pub fn initialize_project(path: &str) -> Result<FirstCommit> {
    check_git(path, &["init", "-q"])?;
    let ignore = Path::new(path).join(".gitignore");
    let existing = match std::fs::read_to_string(&ignore) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    if !existing.lines().any(|line| line == "tmp/") {
        std::fs::write(
            &ignore,
            format!(
                "{existing}{}tmp/\n",
                if existing.is_empty() || existing.ends_with('\n') {
                    ""
                } else {
                    "\n"
                }
            ),
        )?;
    }
    for key in ["user.name", "user.email"] {
        let identity = git(path, &["config", key])?;
        if identity.status.code() == Some(1) {
            return Ok(FirstCommit::SkippedNoIdentity);
        }
        if output(identity)?.is_empty() {
            return Ok(FirstCommit::SkippedNoIdentity);
        }
    }
    check_git(path, &["add", ".gitignore"])?;
    Ok(
        match check_git(path, &["commit", "-q", "-m", "Create project"]) {
            Ok(_) => FirstCommit::Made,
            Err(e) => FirstCommit::Failed(e.to_string()),
        },
    )
}
pub fn is_repository(path: &str) -> Result<bool> {
    let out = git(path, &["rev-parse", "--git-dir"])?;
    if out.status.success() {
        return Ok(true);
    }
    if String::from_utf8_lossy(&out.stderr).contains("not a git repository") {
        return Ok(false);
    }
    output(out).map(|_| true)
}
pub fn remember(owner: &mut OwnerChoices, path: &str, project: &str) {
    owner.removed_projects.remove(path);
    owner.removed_projects.remove(project);
    if !owner.project_paths.iter().any(|p| p == path) {
        owner.project_paths.push(path.into());
    }
}
pub fn project_path(path: &str) -> String {
    discovery::resolve(path)
        .map(|common| {
            let common = Path::new(&common);
            if common
                .file_name()
                .is_some_and(|n| n == ".git" || n == ".bare")
            {
                common
                    .parent()
                    .unwrap_or(common)
                    .to_string_lossy()
                    .into_owned()
            } else {
                common.to_string_lossy().into_owned()
            }
        })
        .unwrap_or_else(|| path.into())
}
pub fn remove_project(
    owner: &mut OwnerChoices,
    state: &mut ViewState,
    project: &Project,
    paths: &[String],
) {
    owner
        .project_paths
        .retain(|p| p != &project.path && !paths.contains(p));
    owner.removed_projects.insert(project.path.clone());
    let workspaces: Vec<_> = project.workspaces.iter().map(|w| w.path.clone()).collect();
    owner.remove_project(&project.path, &workspaces);
    for path in &workspaces {
        clear_selection(state, path);
    }
}
pub fn clear_selection(state: &mut ViewState, workspace: &str) {
    if state.selected_workspace.as_deref() == Some(workspace) {
        state.selected_workspace = None;
        state.selected_child = None;
    }
    state.selected_chats.remove(workspace);
    state.tab_history.remove(workspace);
}
pub fn can_delete(project: &Project, workspace: &Workspace) -> bool {
    project.id.starts_with("repository:")
        && workspace.path != project.path
        && project.main_path.as_deref() != Some(&workspace.path)
        && !workspace.missing
        && !workspace.removed
}
pub fn removal_blocker(path: &str) -> Result<Option<String>> {
    if !check_git(path, &["status", "--porcelain"])?.is_empty() {
        return Ok(Some(
            "This workspace has uncommitted changes. Commit or remove them before deleting it."
                .into(),
        ));
    }
    let upstream = git(path, &["rev-parse", "--verify", "@{upstream}"])?;
    if upstream.status.success() {
        let count = check_git(path, &["rev-list", "--count", "@{upstream}..HEAD"])?;
        let count: u64 = count.parse().map_err(|_| {
            Error::Failed("Git could not check this workspace for unpushed commits.".into())
        })?;
        if count > 0 {
            return Ok(Some(
                "This workspace has unpushed commits. Push them before deleting it.".into(),
            ));
        }
    } else if !check_git(path, &["log", "--oneline", "HEAD", "--not", "--remotes"])?.is_empty() {
        return Ok(Some(
            "This workspace has unpushed commits and no upstream. Push them before deleting it."
                .into(),
        ));
    }
    Ok(None)
}
pub fn impact(project: &Project, workspace: &Workspace, live_agents: usize) -> Result<Value> {
    let mut dirty_files = Vec::new();
    let mut ignored = Vec::new();
    let mut blocker = None;
    if common(project).is_some() && !workspace.missing && !workspace.removed {
        let status = git(
            &workspace.path,
            &["status", "--porcelain", "--ignored=matching", "-z"],
        )?;
        if !status.status.success() {
            output(status)?;
            unreachable!();
        }
        let mut chunks = status.stdout.split(|b| *b == 0);
        while let Some(line) = chunks.next() {
            if line.len() < 4 {
                continue;
            }
            let path = String::from_utf8_lossy(&line[3..]).into_owned();
            if line.starts_with(b"!! ") {
                ignored.push(path);
            } else {
                dirty_files.push(path);
            }
            if line[0] == b'R' || line[1] == b'R' || line[0] == b'C' || line[1] == b'C' {
                chunks.next();
            }
        }
        blocker = removal_blocker(&workspace.path)?;
    }
    Ok(
        json!({"workspace":workspace.path,"liveAgents":live_agents,"confirmationRequired": live_agents > 0,
        "branch":workspace.branch,"dirtyFiles":dirty_files,"ignoredFiles":ignored,"ignoredCount":ignored.len(),
        "firstIgnoredPath":ignored.first(),"canDelete":can_delete(project,workspace),"deleteBlocker":blocker,
        "prunePaths":project.workspaces.iter().filter(|w| w.missing && !w.removed).map(|w| &w.path).collect::<Vec<_>>() }),
    )
}

pub fn live_targets(
    sessions: &[Value],
    listing: Option<&Value>,
    mut single: impl FnMut(&Value) -> std::result::Result<Vec<Value>, String>,
) -> Result<Vec<(String, String)>> {
    let mut children = Vec::new();
    let mut chairs = Vec::new();
    for session in sessions {
        let id = session["id"]
            .as_str()
            .ok_or_else(|| Error::Failed("session has no id".into()))?;
        let agents = if let Some(agents) = listing.and_then(|v| v[id].as_array()) {
            agents.clone()
        } else if session["adapter"]
            .as_str()
            .is_none_or(|a| a.trim().is_empty())
            || (listing.is_some() && session["agents"].as_u64() == Some(0))
        {
            Vec::new()
        } else {
            single(session).map_err(|_| {
                Error::Failed(format!(
                    "Swarm could not read the agents of “{id}”. Try again."
                ))
            })?
        };
        for agent in agents.iter().filter(|a| a["alive"] == true) {
            let agent = agent["id"]
                .as_str()
                .ok_or_else(|| Error::Failed("agent has no id".into()))?;
            let target = (id.to_owned(), agent.to_owned());
            if agent == "orchestrator" || session["chair_id"].as_str() == Some(agent) {
                chairs.push(target);
            } else {
                children.push(target);
            }
        }
    }
    children.extend(chairs);
    Ok(children)
}
