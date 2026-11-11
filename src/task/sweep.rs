//! Explicit cleanup of merged worktrees; branches always stay (ADR 0144).

#[path = "sweep/activity.rs"]
mod activity;

use super::{BranchState, Hub, branch_state, git, hub, record_path, resolve};
use crate::view::{discovery, tree::WorkspaceFact};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A preview entry keeps its verdict after a successful removal; `removed` records the result.
#[derive(Serialize)]
pub struct SweepEntry {
    pub path: String,
    pub branch: Option<String>,
    pub verdict: &'static str,
    pub reason: String,
    pub ignored_files: Option<usize>,
    pub removed: bool,
}

#[derive(Serialize)]
pub struct SweepReport {
    pub worktrees: Vec<SweepEntry>,
}

fn network(command: &mut Command, deadline: Instant) -> Result<String, String> {
    let seconds = deadline.saturating_duration_since(Instant::now()).as_secs();
    if seconds == 0 {
        return Err("the sweep read deadline expired".into());
    }
    let output = discovery::run_command(command, seconds)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn inventory(hub: &Hub) -> Result<Vec<WorkspaceFact>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&hub.common)
        .args(["worktree", "list", "--porcelain", "-z"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("git worktree list failed".into());
    }
    // Refuse lossy path decoding before any path can become a removal target.
    std::str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    Ok(discovery::parse_worktrees(&output.stdout)
        .into_iter()
        .filter(|tree| !tree.bare)
        .collect())
}

fn fresh_default(hub: &Hub, deadline: Instant) -> Result<Option<String>, String> {
    let remotes = git(&hub.common, &["remote"]).ok_or("git remote failed")?;
    if !remotes.lines().any(|name| name == "origin") {
        return Ok(None);
    }
    let reference = format!("refs/remotes/origin/{}", hub.default);
    let refspec = format!("refs/heads/{}:{reference}", hub.default);
    network(
        Command::new("git")
            .arg("-C")
            .arg(&hub.common)
            .args(["fetch", "--quiet", "origin", &refspec])
            .env("GIT_TERMINAL_PROMPT", "0"),
        deadline,
    )
    .map_err(|e| {
        format!(
            "cannot fetch origin/{}, so the sweep stopped: {e}",
            hub.default
        )
    })?;
    // Bare clones can lack a fetch refspec. Set this remote-tracking ref explicitly and pin its
    // SHA; a fetch of an unrelated branch must not change the proof through FETCH_HEAD.
    git(
        &hub.common,
        &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
    )
    .map(Some)
    .ok_or_else(|| "cannot read the fetched default branch".into())
}

fn old_enough(path: &Path, min_age: Duration) -> Option<bool> {
    let created = std::fs::metadata(path).ok()?.created().ok().or_else(|| {
        // A worktree's HEAD reflog starts at its creation. Use the event time, not the commit's
        // timestamp: a new checkout can point at an old commit.
        let log = git(path, &["rev-parse", "--git-path", "logs/HEAD"])?;
        let text = std::fs::read_to_string(path.join(log)).ok()?;
        let header = text.lines().next()?.split('\t').next()?;
        let seconds = header.split_whitespace().rev().nth(1)?.parse().ok()?;
        UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
    })?;
    SystemTime::now()
        .duration_since(created)
        .ok()
        .map(|age| age > min_age)
}

fn ignored_files(path: &Path) -> Option<usize> {
    let paths = git(
        path,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?;
    Some(paths.bytes().filter(|byte| *byte == 0).count())
}

struct Inspection<'a> {
    hub: &'a Hub,
    cwd: &'a Path,
    default: Option<&'a str>,
    min_age: Duration,
    deadline: Instant,
    activity: &'a Result<activity::Activity, String>,
}

impl Inspection<'_> {
    fn verdict(&self, tree: &WorkspaceFact) -> Result<String, String> {
        let path = resolve(Path::new("/"), Path::new(&tree.path));
        if path == self.hub.main || tree.branch.as_deref() == Some(&self.hub.default) {
            return Err("the default branch's worktree".into());
        }
        if self.cwd.starts_with(&path) {
            return Err("the command runs in this worktree".into());
        }
        if tree.locked {
            return Err("the worktree is locked".into());
        }
        if tree.missing || !path.is_dir() {
            return Err("the folder is missing; leave it to git worktree prune".into());
        }
        match self.activity {
            Ok(activity) => activity.check(&path)?,
            Err(error) => return Err(format!("agent activity is unknown: {error}")),
        }
        match old_enough(&path, self.min_age) {
            Some(true) => {}
            Some(false) => return Err("the worktree is too new".into()),
            None => return Err("the worktree's age is unknown".into()),
        }
        let Some(default) = self.default else {
            return Err("no origin; merge proof is unknown".into());
        };
        let common = git(
            &path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .map(PathBuf::from)
        .and_then(|p| std::fs::canonicalize(p).ok());
        if common.as_ref() != Some(&self.hub.common) {
            return Err("the folder no longer belongs to this hub".into());
        }
        let branch = git(&path, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
        if branch != tree.branch
            || branch.as_ref().is_some_and(|branch| {
                branch_state(&self.hub.common, branch) != BranchState::Checkout(path.clone())
            })
        {
            return Err("the checked-out branch changed or is unknown".into());
        }
        let status = git(
            &path,
            &["status", "--porcelain", "-z", "--untracked-files=all"],
        )
        .ok_or("git status failed")?;
        if !status.is_empty() {
            return Err("the worktree has uncommitted or untracked files".into());
        }
        let head =
            git(&path, &["rev-parse", "--verify", "HEAD^{commit}"]).ok_or("HEAD is unknown")?;
        if git(&path, &["merge-base", "--is-ancestor", &head, default]).is_some() {
            return Ok(format!(
                "HEAD is an ancestor of fetched origin/{}",
                self.hub.default
            ));
        }
        let branch = branch
            .ok_or("HEAD is not merged into the fetched default; no branch for a PR check")?;
        let json = network(
            Command::new("gh")
                .current_dir(&path)
                .args([
                    "pr",
                    "list",
                    "--state",
                    "merged",
                    "--head",
                    &branch,
                    "--json",
                    "headRefOid",
                ])
                .env("GH_PROMPT_DISABLED", "1"),
            self.deadline,
        )
        .map_err(|e| format!("HEAD is not in the fetched default; PR proof is unknown: {e}"))?;
        let prs: Vec<serde_json::Value> =
            serde_json::from_str(&json).map_err(|_| "merged PR proof could not be read")?;
        if prs
            .iter()
            .any(|pr| pr["headRefOid"].as_str() == Some(&head))
        {
            Ok("a merged PR has this exact HEAD".into())
        } else {
            Err("no merge proof for this HEAD".into())
        }
    }
}

/// List every Git worktree, and optionally remove only proven merged, clean, old, inactive ones.
/// The CLI supplies one hour; the test clock boundary supplies a shorter minimum age.
pub fn sweep(
    root: &Path,
    cwd: &Path,
    apply: bool,
    min_age: Duration,
) -> Result<SweepReport, String> {
    let hub = hub(cwd)
        .or_else(|| {
            cwd.ancestors()
                .find_map(|parent| hub(&parent.join(".bare")))
        })
        .ok_or("this folder is not in a worktree hub")?;
    let cwd = resolve(Path::new("/"), cwd);
    let deadline = Instant::now() + Duration::from_secs(120);
    let default = fresh_default(&hub, deadline)?;
    let trees = inventory(&hub)?;
    let activity = activity::read(root, &hub, &trees, deadline);
    let inspection = Inspection {
        hub: &hub,
        cwd: &cwd,
        default: default.as_deref(),
        min_age,
        deadline,
        activity: &activity,
    };
    let mut entries = Vec::new();
    for tree in &trees {
        let mut entry = SweepEntry {
            path: tree.path.clone(),
            branch: tree.branch.clone(),
            verdict: "keep",
            reason: String::new(),
            ignored_files: ignored_files(Path::new(&tree.path)),
            removed: false,
        };
        match inspection.verdict(tree) {
            Ok(proof) => {
                entry.verdict = "remove";
                entry.reason = proof;
            }
            Err(reason) => entry.reason = reason,
        }
        if entry.verdict == "remove" && entry.ignored_files.is_none() {
            entry.verdict = "keep";
            entry.reason = "the ignored-file count is unknown".into();
        }
        if apply && entry.verdict == "remove" {
            // The preview is no removal authority. Refresh locks, branches, changes and activity
            // at the boundary; git's own non-force removal also refuses new uncommitted work.
            let checked = inventory(&hub)?;
            let fresh_activity = activity::read(root, &hub, &checked, deadline);
            let fresh = Inspection {
                activity: &fresh_activity,
                ..inspection
            };
            let verdict = checked
                .iter()
                .find(|now| now.path == tree.path && now.branch == tree.branch)
                .ok_or_else(|| "the worktree is no longer in Git's inventory".to_owned())
                .and_then(|now| fresh.verdict(now));
            match verdict {
                Err(reason) => {
                    entry.verdict = "keep";
                    entry.reason = reason;
                }
                Ok(proof) => {
                    entry.reason = proof;
                    let removed = Command::new("git")
                        .arg("-C")
                        .arg(&hub.common)
                        .args(["worktree", "remove", "--", &tree.path])
                        .output();
                    match removed {
                        Ok(output) if output.status.success() => {
                            entry.removed = true;
                            if let Some(branch) = &tree.branch {
                                let record =
                                    record_path(root, &hub.common.to_string_lossy(), branch);
                                if let Some(task) =
                                    super::read_record(root, &hub.common.to_string_lossy(), branch)
                                    && resolve(Path::new("/"), Path::new(&task.path))
                                        == resolve(Path::new("/"), Path::new(&tree.path))
                                    && let Err(error) = std::fs::remove_file(record)
                                {
                                    entry.verdict = "error";
                                    entry.reason = format!(
                                        "worktree removed, but task record removal failed: {error}"
                                    );
                                }
                            }
                        }
                        output => {
                            entry.verdict = "error";
                            entry.reason = format!(
                                "git worktree remove failed: {}",
                                match output {
                                    Ok(output) =>
                                        String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                                    Err(error) => error.to_string(),
                                }
                            );
                        }
                    }
                }
            }
        }
        entries.push(entry);
    }
    Ok(SweepReport { worktrees: entries })
}
