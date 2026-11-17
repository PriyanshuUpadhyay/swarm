//! A chat's own worktree (ADR 0146): its placeholder name, its record, the name its first
//! prompt gives, and its removal when the chat never used it.

use super::*;

/// A chat worktree's placeholder name is this and `CHAT_ID_LEN` hex digits.
const CHAT_NAME: &str = "chat-";
const CHAT_ID_LEN: usize = 8;
/// The most words of a first prompt that go into a branch name.
const SLUG_WORDS: usize = 5;

/// A new chat worktree's folder and placeholder branch name, `chat-<8 hex>`; its branch is
/// `swarm/` and this name until the first prompt names it.
pub fn chat_placeholder() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{CHAT_NAME}{}", &id[..CHAT_ID_LEN])
}

/// True for the branch a chat worktree gets before its first prompt: `swarm/chat-<8 hex>`.
pub fn unnamed_chat(branch: &str) -> bool {
    branch
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_prefix(CHAT_NAME))
        .is_some_and(|id| id.len() == CHAT_ID_LEN && id.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The branch name a first prompt gives: its first five ASCII words, lowercase and joined by
/// `-`, at most 40 characters, under `swarm/`. None when the prompt has no such word.
fn chat_branch(prompt: &str) -> Option<String> {
    let words: Vec<_> = prompt
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(SLUG_WORDS)
        .collect();
    if words.is_empty() {
        return None;
    }

    // The workspace slug lowercases, joins with `-`, and keeps 40 characters.
    let slug = crate::view::lifecycle::slug(&words.join(" "));
    Some(branch_name(slug.trim_end_matches('-')))
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

    let sync_warning = git(&hub.common, &["remote", "get-url", "origin"])
        .and_then(|_| origin::sync_default(&hub, &format!("origin/{}", hub.default)).err());
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
/// word, or one whose nine names are all taken, gives `untitled-<id>`, so a chat that had a prompt
/// never reads as unnamed. The folder keeps its name, so the agent's folder stays. None for a
/// folder that is not an unnamed chat worktree, or whose record names another branch.
pub fn name_chat(root: &Path, cwd: &Path, prompt: &str) -> Result<Option<String>, String> {
    // One git call settles every send after the first, and every folder that is no chat worktree.
    let head = git(cwd, &["symbolic-ref", "-q", "--short", "HEAD"]);
    if !head.as_deref().is_some_and(unnamed_chat) {
        return Ok(None);
    }
    // A record is a file an agent can write, so it must name the branch that git has.
    let Ok((hub, task)) = task_here(root, cwd) else {
        return Ok(None);
    };
    if head.as_deref() != Some(task.branch.as_str()) {
        return Ok(None);
    }

    let id = &task.branch[PREFIX.len() + CHAT_NAME.len()..];
    let untitled = branch_name(&format!("untitled-{id}"));

    let base = chat_branch(prompt)
        .filter(|name| !unnamed_chat(name))
        .unwrap_or_else(|| untitled.clone());
    let names = std::iter::once(base.clone())
        .chain((2..=9).map(|n| format!("{base}-{n}")))
        .chain([untitled]);
    let renamed = names
        .filter(|name| {
            branch_state(&hub.common, name) == BranchState::Absent
                && !ref_exists(&hub.common, &format!("refs/remotes/origin/{name}"))
        })
        .find_map(|name| claim_record(root, &task, name).transpose())
        .transpose()?
        .ok_or_else(|| format!("{base}, its -2 to -9 names, and the untitled name are taken"))?;

    // The new record is this call's own claim, so a failed rename removes only that file.
    let (branch, new_record) = renamed;
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

/// Writes the record of `task` under the new branch `name`, only when no record has that name.
/// Creating the file is the claim, so two first prompts at once never share a name. None when
/// another chat claimed it first.
fn claim_record(
    root: &Path,
    task: &Task,
    name: String,
) -> Result<Option<(String, PathBuf)>, String> {
    let path = record_path(root, &task.repo, &name);
    let renamed = Task {
        branch: name.clone(),
        ..task.clone()
    };
    let text = serde_json::to_string_pretty(&renamed).map_err(|error| error.to_string())? + "\n";
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path);

    match file {
        Ok(mut file) => match std::io::Write::write_all(&mut file, text.as_bytes()) {
            Ok(()) => Ok(Some((name, path))),
            Err(error) => {
                // A half-written claim would hold the name for no chat.
                let _ = std::fs::remove_file(&path);
                Err(format!("cannot write the task record: {error}"))
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(format!("cannot write the task record: {error}")),
    }
}

/// The top folder of the git checkout that `folder` sits in when that top's name starts with
/// `chat-`, else None. A plain repository named so passes too; `drop_unnamed_chat` then checks
/// its task record. Only a path with a `chat-` part costs a git call.
pub fn chat_top(folder: &Path) -> Option<String> {
    let in_chat = |path: &Path| {
        path.components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with(CHAT_NAME))
    };
    if !in_chat(folder) {
        return None;
    }

    // The bounded call (10 s), because archive runs this for each folder.
    let top = crate::view::lifecycle::check_git(
        &folder.to_string_lossy(),
        &["rev-parse", "--show-toplevel"],
    )
    .ok()?;
    Path::new(&top)
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(CHAT_NAME))
        .then_some(top)
}

/// Removes a chat worktree that a failed chat start left, and its record, through Worktrunk, so
/// its remove hooks run. With no `--force`, a worktree with changes stays. The branch goes only
/// when it has no commit that the default branch, origin's when fetched, lacks; local main can be
/// behind origin's, where the chat started.
pub fn remove_failed_chat(root: &Path, wt: &Path, path: &Path) -> Result<(), String> {
    let hub = hub(path).ok_or("this folder is not in a worktree hub")?;
    let branch = git(path, &["symbolic-ref", "-q", "--short", "HEAD"])
        .filter(|branch| unnamed_chat(branch))
        .ok_or("this folder is not an unnamed chat worktree")?;
    let origin = format!("refs/remotes/origin/{}", hub.default);
    let base = if ref_exists(&hub.common, &origin) {
        origin
    } else {
        hub.default.clone()
    };
    let own = git(
        &hub.common,
        &["rev-list", "--count", &format!("{base}..{branch}")],
    );

    let mut args = vec![
        "remove",
        branch.as_str(),
        "--foreground",
        "--format",
        "json",
    ];
    if own.as_deref() == Some("0") {
        args.push("--force-delete");
    } else {
        args.push("--no-delete-branch");
    }
    run(wt, &hub.main, &args, false)?;
    let _ = std::fs::remove_file(record_path(root, &hub.common.to_string_lossy(), &branch));
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
