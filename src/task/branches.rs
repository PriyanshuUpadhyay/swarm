//! What a branch name meets in a hub, and the base new work starts from (ADR 0142).

use super::git;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What a branch name meets in a repository. Both create paths, the sidebar "+" and
/// `swarm task start`, decide from this, so a left-over branch opens or reopens instead of failing.
#[derive(Debug, Clone, PartialEq)]
pub enum BranchState {
    /// No local branch has this name.
    Absent,
    /// The branch exists and no worktree has it checked out.
    Free,
    /// The worktree that has the branch checked out.
    Checkout(PathBuf),
}

pub fn branch_state(dir: &Path, branch: &str) -> BranchState {
    let reference = format!("refs/heads/{branch}");
    if git(dir, &["show-ref", "--verify", "--quiet", &reference]).is_none() {
        return BranchState::Absent;
    }
    let listing = git(dir, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    let want = format!("branch {reference}");
    listing
        .split("\n\n")
        .find_map(|block| {
            let path = block
                .lines()
                .find_map(|line| line.strip_prefix("worktree "))?;
            block
                .lines()
                .any(|line| line == want)
                .then(|| PathBuf::from(path))
        })
        .map_or(BranchState::Free, BranchState::Checkout)
}

/// `origin/<default>` after a fetch, when the repository has that remote branch; else None, and
/// new work starts from the local default branch. A failed fetch is an error, so new work never
/// starts from an old base without a word.
pub fn fresh_origin_base(dir: &Path, default: &str) -> Result<Option<String>, String> {
    if git(dir, &["remote", "get-url", "origin"]).is_none() {
        return Ok(None);
    }
    let fetched = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "fetch",
            "--quiet",
            "origin",
            &format!("+refs/heads/{default}:refs/remotes/origin/{default}"),
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("git fetch did not start: {error}"))?;
    if !fetched.status.success() {
        return Err(format!(
            "git could not fetch origin/{default}, so the task operation stopped; try again: {}",
            String::from_utf8_lossy(&fetched.stderr).trim()
        ));
    }
    Ok(Some(format!("origin/{default}")))
}
