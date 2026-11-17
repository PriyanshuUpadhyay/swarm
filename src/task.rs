//! Agent task worktrees (ADR 0135). A hub is a repository whose git common dir is bare, with each
//! branch in its own worktree. Its main worktree, the one with the default branch, is read-only
//! for agents: an agent makes a task worktree with Worktrunk, and swarm lands and removes it.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// The branch prefix of every task worktree.
pub const PREFIX: &str = "swarm/";
/// Agents may write main's `tmp/`: it is ignored scratch where kit runs live, and the sidebar
/// reads step runs there (ADR 0046).
pub const SCRATCH: &str = "tmp";

#[derive(Debug, Clone, PartialEq)]
pub struct Hub {
    /// The bare common dir, canonical.
    pub common: PathBuf,
    pub default: String,
    /// The worktree that has the default branch checked out, canonical.
    pub main: PathBuf,
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// `path` made absolute against `cwd`, with a leading `~` as the home folder, `.` and `..`
/// folded, and the longest existing prefix canonical, so `/tmp/x` and `/private/tmp/x` compare
/// equal and a new file still resolves.
pub fn resolve(cwd: &Path, path: &Path) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = match (path.strip_prefix("~"), home) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.to_path_buf(),
    };
    let mut joined = PathBuf::new();
    for part in cwd.join(path).components() {
        match part {
            // `..` after a symlink leaves the link's target, as the OS does, not the link's folder.
            Component::ParentDir => {
                if let Ok(real) = std::fs::canonicalize(&joined) {
                    joined = real;
                }
                joined.pop();
            }
            Component::CurDir => {}
            other => joined.push(other),
        }
    }
    let mut rest = Vec::new();
    let mut base = joined.as_path();
    loop {
        if let Ok(real) = std::fs::canonicalize(base) {
            return rest.iter().rev().fold(real, |acc, name| acc.join(name));
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                base = parent;
            }
            _ => return joined,
        }
    }
}

/// The hub that holds `path`, or None for a plain clone or a path outside git.
pub fn hub(path: &Path) -> Option<Hub> {
    let mut dir = resolve(Path::new("/"), path);
    while !dir.is_dir() {
        dir = dir.parent()?.to_path_buf();
    }
    let common = git(
        &dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common = std::fs::canonicalize(common).ok()?;
    if git(&common, &["config", "--bool", "core.bare"]).as_deref() != Some("true") {
        return None;
    }
    let default = default_branch(&common)?;
    let listing = git(&common, &["worktree", "list", "--porcelain"])?;
    let want = format!("branch refs/heads/{default}");
    let main = listing.split("\n\n").find_map(|block| {
        let path = block
            .lines()
            .find_map(|line| line.strip_prefix("worktree "))?;
        block
            .lines()
            .any(|line| line == want)
            .then(|| PathBuf::from(path))
    })?;
    Some(Hub {
        common,
        default,
        main: std::fs::canonicalize(main).ok()?,
    })
}

/// The repository's default branch: the one `origin/HEAD` names, or else, in a bare hub, the
/// one its `HEAD` names. A plain clone's `HEAD` is whatever its main checkout has, so it does not
/// count.
pub fn default_branch(dir: &Path) -> Option<String> {
    git(
        dir,
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"],
    )
    .and_then(|name| name.split_once('/').map(|(_, branch)| branch.to_string()))
    .or_else(|| {
        let common = git(
            dir,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        (git(Path::new(&common), &["config", "--bool", "core.bare"]).as_deref() == Some("true"))
            .then(|| {
                git(
                    Path::new(&common),
                    &["symbolic-ref", "-q", "--short", "HEAD"],
                )
            })?
    })
}

impl Hub {
    /// Whether a resolved path is main's own content (inside main and outside its scratch), or a
    /// file of the common dir that moves the default branch or turns the ref hook off.
    pub fn guards(&self, resolved: &Path) -> bool {
        let common = |name: &str| resolved.starts_with(self.common.join(name));
        (resolved.starts_with(&self.main) && !resolved.starts_with(self.main.join(SCRATCH)))
            || common(&format!("refs/heads/{}", self.default))
            || common("packed-refs")
            // Main's HEAD and index live here; a write there can take main off the default branch.
            || common("worktrees")
            || common("HEAD")
            // `default_branch` reads these; a write there points the hub at another branch.
            || common("refs/remotes/origin/HEAD")
            || common("hooks")
            || common("config")
    }
}

/// A task's ownership record, one JSON file in `<swarm home>/tasks/`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub branch: String,
    pub path: String,
    pub repo: String,
    pub default: String,
    /// The swarm session and agent that started the task; absent outside swarm.
    pub session: Option<String>,
    pub agent: Option<String>,
    pub created_unix_s: u64,
}

/// The file name of a task's record and of its landing request: the hub and the branch, so two
/// hubs with the same branch name do not meet.
pub fn key(repo: &str, branch: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(repo.as_bytes());
    let hash: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("{hash}-{}", branch.replace('/', "-"))
}

pub fn record_path(root: &Path, repo: &str, branch: &str) -> PathBuf {
    root.join("tasks")
        .join(format!("{}.json", key(repo, branch)))
}

pub fn read_record(root: &Path, repo: &str, branch: &str) -> Option<Task> {
    let text = std::fs::read_to_string(record_path(root, repo, branch)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn records(root: &Path) -> Vec<Task> {
    let Ok(entries) = std::fs::read_dir(root.join("tasks")) else {
        return Vec::new();
    };
    let mut tasks: Vec<Task> = entries
        .filter_map(|entry| std::fs::read_to_string(entry.ok()?.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    tasks.sort_by_key(|task| task.created_unix_s);
    tasks
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::store::write_atomic(path, &(serde_json::to_string_pretty(value)? + "\n"))
}

/// Worktrunk. `wt` on PATH comes first, because the owner may wrap it with a policy; a server
/// started by the app can lack the owner's PATH, so the install folders follow.
pub fn wt_program() -> PathBuf {
    let on_path = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("wt"))
            .find(|path| path.is_file())
    });
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    on_path
        .into_iter()
        .chain([
            home.join(".local/bin/wt"),
            home.join(".cargo/bin/wt"),
            PathBuf::from("/opt/homebrew/bin/wt"),
        ])
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("wt"))
}

/// The variables that mark an agent's shell. The ref hook refuses them; landing removes them.
pub const AGENT_MARKERS: [&str; 4] = [
    "SWARM_AGENT_ID",
    "CLAUDECODE",
    "CODEX_THREAD_ID",
    "CODEX_SANDBOX",
];

fn run(program: &Path, dir: &Path, args: &[&str], scrub: bool) -> Result<String, String> {
    let mut command = Command::new(program);
    command.arg("-C").arg(dir).args(args);
    command.stdin(std::process::Stdio::null());
    if scrub {
        for name in AGENT_MARKERS {
            command.env_remove(name);
        }
    }
    let output = command
        .output()
        .map_err(|error| format!("{} did not start: {error}", program.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// True for the branch a chat worktree gets before its first prompt: `swarm/chat-<8 hex>`.
pub fn unnamed_chat(branch: &str) -> bool {
    branch
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_prefix("chat-"))
        .is_some_and(|id| id.len() == 8 && id.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The branch name a first prompt gives: its first five ASCII words, lowercase and joined by
/// `-`, at most 40 characters, under `swarm/`. None when the prompt has no such word.
pub fn chat_branch(prompt: &str) -> Option<String> {
    let mut slug = String::new();
    let words = prompt
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(5);
    for word in words {
        let word = word.to_ascii_lowercase();
        let extra = usize::from(!slug.is_empty()) + word.len();
        if slug.len() + extra > 40 {
            if slug.is_empty() {
                slug = word[..40].to_string();
            }
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word);
    }
    (!slug.is_empty()).then(|| format!("{PREFIX}{slug}"))
}

/// The record of a chat's own worktree at `path`, which the app made when the chat opened, so
/// `land`, `drop`, and `sweep` treat it as a task. Only the server calls it. Local default moves to origin's (ADR 0145), and
/// the ref hook is set. Returns the record and a warning when either could not be done.
pub fn record_chat(
    root: &Path,
    path: &Path,
    session: Option<String>,
) -> Result<(Task, Option<String>), String> {
    let hub = hub(path).ok_or("this folder is not in a worktree hub (a bare .bare with wt/)")?;
    let branch = git(path, &["symbolic-ref", "-q", "--short", "HEAD"])
        .filter(|branch| branch.starts_with(PREFIX))
        .ok_or_else(|| format!("{} is not on a {PREFIX} branch", path.display()))?;
    let repo = hub.common.to_string_lossy().into_owned();
    let sync_warning = git(&hub.common, &["remote", "get-url", "origin"]).and_then(|_| {
        // The server calls this, and it moves main as landing does, so agent markers go.
        origin::sync_default(&hub, &format!("origin/{}", hub.default), true).err()
    });
    let task = Task {
        branch: branch.clone(),
        path: path.to_string_lossy().into_owned(),
        repo: repo.clone(),
        default: hub.default.clone(),
        session,
        agent: Some("orchestrator".into()),
        created_unix_s: now(),
    };
    write_json(&record_path(root, &repo, &branch), &task)
        .map_err(|error| format!("cannot write the task record: {error}"))?;
    let warnings: Vec<_> = [sync_warning, install_hook(&hub)]
        .into_iter()
        .flatten()
        .collect();
    Ok((task, (!warnings.is_empty()).then(|| warnings.join("; "))))
}

/// Renames an unnamed chat worktree's branch after the chat's first prompt, with `-2` to `-9` on
/// a taken name, and moves its record. The folder keeps its name, so the agent's folder stays.
/// None when the branch already has a name or the prompt gives none.
pub fn name_chat(root: &Path, cwd: &Path, prompt: &str) -> Result<Option<String>, String> {
    let (hub, task) = task_here(root, cwd)?;
    if !unnamed_chat(&task.branch) {
        return Ok(None);
    }
    let Some(base) = chat_branch(prompt) else {
        return Ok(None);
    };
    let branch = std::iter::once(base.clone())
        .chain((2..=9).map(|n| format!("{base}-{n}")))
        .find(|name| branch_state(&hub.common, name) == BranchState::Absent)
        .ok_or_else(|| format!("{base} and its -2 to -9 names are taken"))?;
    run(
        Path::new("git"),
        Path::new(&task.path),
        &["branch", "-m", &task.branch, &branch],
        false,
    )?;
    let renamed = Task {
        branch: branch.clone(),
        ..task.clone()
    };
    write_json(&record_path(root, &task.repo, &branch), &renamed)
        .map_err(|error| format!("cannot write the task record: {error}"))?;
    let _ = std::fs::remove_file(record_path(root, &task.repo, &task.branch));
    Ok(Some(branch))
}

/// The task worktree that holds `cwd`, with its record.
fn task_here(root: &Path, cwd: &Path) -> Result<(Hub, Task), String> {
    let hub = hub(cwd).ok_or("this folder is not in a worktree hub")?;
    let top = git(cwd, &["rev-parse", "--show-toplevel"]).ok_or("this folder is not in git")?;
    let branch = git(cwd, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok_or("this worktree has no branch checked out")?;
    let repo = hub.common.to_string_lossy().into_owned();
    let task = read_record(root, &repo, &branch)
        .filter(|task| {
            resolve(Path::new("/"), Path::new(&task.path)) == resolve(cwd, Path::new(&top))
        })
        .ok_or_else(|| format!("{top} is not a chat worktree that swarm made"))?;
    Ok((hub, task))
}

/// The branch of a task name: `fix-x` and `swarm/fix-x` both give `swarm/fix-x`.
fn branch_name(name: &str) -> String {
    if name.starts_with(PREFIX) {
        name.to_string()
    } else {
        format!("{PREFIX}{name}")
    }
}

/// The task that `name` names in the hub of `cwd`, or, with no name, the task worktree that holds
/// `cwd`. A name lets an agent act from main without `cd`.
fn find_task(root: &Path, cwd: &Path, name: Option<&str>) -> Result<(Hub, Task), String> {
    let Some(name) = name else {
        return task_here(root, cwd);
    };
    let branch = branch_name(name);
    let hub = hub(cwd).ok_or("this folder is not in a worktree hub")?;
    let task = read_record(root, &hub.common.to_string_lossy(), &branch)
        .ok_or_else(|| format!("{branch} is not a chat worktree that swarm made here"))?;
    Ok((hub, task))
}

/// Why a task worktree still holds work the default branch lacks, or None.
fn unlanded(path: &Path, branch: &str, default: &str) -> Result<Option<String>, String> {
    let status = git(path, &["status", "--porcelain"]).ok_or("git status failed")?;
    if !status.is_empty() {
        return Ok(Some("it has uncommitted changes".into()));
    }
    let ahead = git(
        path,
        &["rev-list", "--count", &format!("{default}..{branch}")],
    )
    .ok_or("git rev-list failed")?;
    Ok((ahead != "0").then(|| format!("it has {ahead} commit(s) that {default} does not have")))
}

/// `swarm task drop [<name>]`: remove a task worktree that holds no work the default branch
/// lacks, and its record.
pub fn drop(root: &Path, wt: &Path, cwd: &Path, name: Option<&str>) -> Result<Task, String> {
    let (hub, task) = find_task(root, cwd, name)?;
    // The record is a file an agent can write, so the folder must be this hub's worktree of a
    // task branch before Worktrunk removes it.
    if !task.branch.starts_with(PREFIX)
        || Path::new(&task.path).is_dir()
            && (self::hub(Path::new(&task.path)).map(|found| found.common)
                != Some(hub.common.clone())
                || git(
                    Path::new(&task.path),
                    &["symbolic-ref", "-q", "--short", "HEAD"],
                )
                .as_deref()
                    != Some(&task.branch))
    {
        return Err(format!(
            "the record of {} does not match its folder",
            task.branch
        ));
    }
    if Path::new(&task.path).is_dir()
        && let Some(why) = unlanded(Path::new(&task.path), &task.branch, &hub.default)?
    {
        return Err(format!(
            "{} was not dropped, because {why}; land it with `swarm task land`, or ask the owner",
            task.branch
        ));
    }
    if Path::new(&task.path).is_dir() {
        run(
            wt,
            &hub.main,
            &["remove", &task.branch, "--foreground", "--format", "json"],
            false,
        )?;
    }
    let _ = std::fs::remove_file(record_path(root, &task.repo, &task.branch));
    Ok(task)
}

#[path = "task/branches.rs"]
mod branches;
pub use branches::{BranchState, branch_state, fresh_origin_base};
#[path = "task/hook.rs"]
mod hook;
pub use hook::install_hook;
#[path = "task/landing.rs"]
mod landing;
pub use landing::{request_landing, request_pull_request, serve_landings, take_result};
#[path = "task/origin.rs"]
mod origin;

#[path = "task/sweep.rs"]
mod sweep;
pub use sweep::{SweepEntry, SweepReport, sweep};

#[cfg(test)]
#[path = "task/tests.rs"]
pub(crate) mod tests;
