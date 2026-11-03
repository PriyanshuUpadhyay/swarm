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
/// The first line of the ref hook swarm writes, so it never replaces another tool's hook.
const HOOK_MARK: &str = "# swarm task guard (ADR 0135)";

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

/// The hub whose main worktree holds `path` (resolved against `cwd`), when that path is main's
/// own content.
pub fn main_target(cwd: &Path, path: &Path) -> Option<Hub> {
    let resolved = resolve(cwd, path);
    hub(&resolved).filter(|hub| hub.guards(&resolved))
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
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, serde_json::to_string_pretty(value)? + "\n")?;
    std::fs::rename(tmp, path)
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

/// `swarm task start <slug>`: a worktree on `swarm/<slug>` from the default branch, its record,
/// and the ref hook. Returns the record and a warning when the hook could not be set.
pub fn start(
    root: &Path,
    wt: &Path,
    cwd: &Path,
    slug: &str,
    owner: (Option<String>, Option<String>),
) -> Result<(Task, Option<String>), String> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "task name {slug:?} must use a-z, 0-9, and -, like fix-login"
        ));
    }
    let hub = hub(cwd).ok_or("this folder is not in a worktree hub (a bare .bare with wt/)")?;
    let branch = format!("{PREFIX}{slug}");
    let created = run(
        wt,
        &hub.main,
        &[
            "switch",
            "--create",
            &branch,
            "--base",
            &hub.default,
            "--no-cd",
            "--format",
            "json",
        ],
        false,
    )?;
    let path = created
        .lines()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .and_then(|value| value["path"].as_str().map(str::to_string))
        .ok_or_else(|| format!("Worktrunk did not report a path: {created}"))?;
    let repo = hub.common.to_string_lossy().into_owned();
    let task = Task {
        branch: branch.clone(),
        path,
        repo: repo.clone(),
        default: hub.default.clone(),
        session: owner.0,
        agent: owner.1,
        created_unix_s: now(),
    };
    write_json(&record_path(root, &repo, &branch), &task).map_err(|error| error.to_string())?;
    Ok((task, install_hook(&hub)))
}

/// Write the ref hook into the hub's hooks folder. Returns why it was not written, when it was not.
pub fn install_hook(hub: &Hub) -> Option<String> {
    if let Some(path) = git(&hub.common, &["config", "--get", "core.hooksPath"]) {
        return Some(format!(
            "core.hooksPath is {path}, so the ref hook is not set; the guard wall still blocks agent moves of {}",
            hub.default
        ));
    }
    // The name goes into a shell script, so only plain branch names are written there.
    if !hub
        .default
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
    {
        return Some(format!(
            "the default branch {:?} has characters a hook script would read as shell, so the ref hook is not set",
            hub.default
        ));
    }
    let file = hub.common.join("hooks").join("reference-transaction");
    // Read as bytes, so a hook that is not UTF-8 still counts as another tool's.
    if let Ok(bytes) = std::fs::read(&file)
        && !String::from_utf8_lossy(&bytes).contains(HOOK_MARK)
    {
        return Some(format!(
            "{} is another tool's hook, so swarm's ref hook is not set",
            file.display()
        ));
    }
    let markers = AGENT_MARKERS
        .iter()
        .map(|name| format!("${{{name}:-}}"))
        .collect::<String>();
    let script = format!(
        "#!/bin/sh\n{HOOK_MARK}: an agent shell cannot move {default}; `swarm task land` asks swarm to.\n\
         [ \"$1\" = prepared ] || {{ cat >/dev/null; exit 0; }}\n\
         refused=0\n\
         while read -r old new ref; do\n  [ \"$ref\" = refs/heads/{default} ] && refused=1\ndone\n\
         [ \"$refused\" = 1 ] && [ -n \"{markers}\" ] || exit 0\n\
         echo \"swarm: agents cannot move {default}; edit in a task worktree and run swarm task land\" >&2\n\
         exit 1\n",
        default = hub.default
    );
    let written = std::fs::create_dir_all(file.parent()?)
        .and_then(|()| std::fs::write(&file, script))
        .and_then(|()| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
        });
    written
        .err()
        .map(|error| format!("cannot write {}: {error}", file.display()))
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
        .ok_or_else(|| format!("{top} is not a task worktree that `swarm task start` made"))?;
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
        .ok_or_else(|| format!("{branch} is not a task that `swarm task start` made here"))?;
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

#[path = "task/landing.rs"]
mod landing;
pub use landing::{request_landing, serve_landings, take_result};

#[cfg(test)]
#[path = "task/tests.rs"]
pub(crate) mod tests;
