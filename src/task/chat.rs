//! A chat's own worktree (ADR 0146): its placeholder name, its record, the name its first
//! prompt gives, and its removal when the chat never used it.

use super::*;

/// The hex digits of a chat worktree's placeholder name, `chat-<id>`.
const CHAT_ID_LEN: usize = 8;
/// The most characters of a first prompt that go into a branch name.
const SLUG_MAX: usize = 40;
/// The most words of a first prompt that go into a branch name.
const SLUG_WORDS: usize = 5;

/// A new chat worktree's folder and placeholder branch name, `chat-<8 hex>`; its branch is
/// `swarm/` and this name until the first prompt names it.
pub fn chat_placeholder() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("chat-{}", &id[..CHAT_ID_LEN])
}

/// True for the branch a chat worktree gets before its first prompt: `swarm/chat-<8 hex>`.
pub fn unnamed_chat(branch: &str) -> bool {
    branch
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_prefix("chat-"))
        .is_some_and(|id| id.len() == CHAT_ID_LEN && id.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The branch name a first prompt gives: its first five ASCII words, lowercase and joined by
/// `-`, at most 40 characters, under `swarm/`. None when the prompt has no such word.
pub fn chat_branch(prompt: &str) -> Option<String> {
    let mut slug = String::new();
    let words = prompt
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(SLUG_WORDS);
    for word in words {
        let word = word.to_ascii_lowercase();
        let extra = usize::from(!slug.is_empty()) + word.len();
        if slug.len() + extra > SLUG_MAX {
            if slug.is_empty() {
                slug = word[..SLUG_MAX].to_string();
            }
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word);
    }
    (!slug.is_empty()).then(|| branch_name(&slug))
}

/// The record of a chat's own worktree at `path`, which the app made when the chat opened, so
/// `land`, `drop`, and `sweep` treat it as a task. Only the server calls it. Local default moves
/// to origin's (ADR 0145), and the ref hook is set. Returns the record and a warning when either could not be done.
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

/// Renames an unnamed chat worktree's branch after the chat's first prompt, and moves its
/// record. A name that a local or an origin branch has gets `-2` to `-9`. A prompt with no ASCII
/// word gives `untitled-<id>`, so a chat that had a prompt never reads as unnamed. The folder keeps
/// its name, so the agent's folder stays. None for a folder that is not an unnamed chat worktree.
pub fn name_chat(root: &Path, cwd: &Path, prompt: &str) -> Result<Option<String>, String> {
    // One git call settles every send after the first, and every folder that is no chat worktree.
    let head = git(cwd, &["symbolic-ref", "-q", "--short", "HEAD"]);
    if !head.as_deref().is_some_and(unnamed_chat) {
        return Ok(None);
    }
    let Ok((hub, task)) = task_here(root, cwd) else {
        return Ok(None);
    };

    let id = &task.branch[task.branch.len() - CHAT_ID_LEN..];
    let base = chat_branch(prompt)
        .filter(|name| !unnamed_chat(name))
        .unwrap_or_else(|| branch_name(&format!("untitled-{id}")));
    let taken = |name: &str| {
        branch_state(&hub.common, name) != BranchState::Absent
            || git(
                &hub.common,
                &[
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/remotes/origin/{name}"),
                ],
            )
            .is_some()
    };
    let branch = std::iter::once(base.clone())
        .chain((2..=9).map(|n| format!("{base}-{n}")))
        .find(|name| !taken(name))
        .ok_or_else(|| format!("{base} and its -2 to -9 names are taken"))?;

    // The new record goes first, so a failed write leaves the branch and its record as they were.
    let renamed = Task {
        branch: branch.clone(),
        ..task.clone()
    };
    let new_record = record_path(root, &task.repo, &branch);
    write_json(&new_record, &renamed)
        .map_err(|error| format!("cannot write the task record: {error}"))?;
    if let Err(error) = run(
        Path::new("git"),
        Path::new(&task.path),
        &["branch", "-m", &task.branch, &branch],
        false,
    ) {
        let _ = std::fs::remove_file(&new_record);
        return Err(error);
    }
    let _ = std::fs::remove_file(record_path(root, &task.repo, &task.branch));
    Ok(Some(branch))
}

/// Removes a chat worktree that a failed chat start left, when it holds no change; `git worktree
/// remove` without `--force` refuses one that does. Its branch goes only when `git branch -d`
/// finds it merged, so no commit is lost.
pub fn remove_failed_chat(path: &Path) -> Result<(), String> {
    let hub = hub(path).ok_or("this folder is not in a worktree hub")?;
    let branch = git(path, &["symbolic-ref", "-q", "--short", "HEAD"])
        .filter(|branch| unnamed_chat(branch))
        .ok_or("this folder is not an unnamed chat worktree")?;
    let folder = path.to_string_lossy();
    run(
        Path::new("git"),
        &hub.common,
        &["worktree", "remove", &folder],
        false,
    )?;
    let _ = run(
        Path::new("git"),
        &hub.common,
        &["branch", "-d", &branch],
        false,
    );
    Ok(())
}

/// `drop` for a chat worktree whose chat ended before its first prompt, so its branch still has
/// no name. None for any other folder. `drop` still refuses a worktree with changes or commits.
pub fn drop_unnamed_chat(root: &Path, wt: &Path, cwd: &Path) -> Result<Option<Task>, String> {
    match task_here(root, cwd) {
        Ok((_, task)) if unnamed_chat(&task.branch) => drop(root, wt, cwd, None).map(Some),
        _ => Ok(None),
    }
}
