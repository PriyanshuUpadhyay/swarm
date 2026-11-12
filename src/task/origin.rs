//! Origin landing keeps the default branch unchanged until the remote accepts the push.

use super::*;

pub(super) fn sync_default(hub: &Hub, target: &str, scrub: bool) -> Result<(), String> {
    let local =
        git(&hub.main, &["rev-parse", &hub.default]).ok_or("cannot read local default branch")?;
    let remote =
        git(&hub.main, &["rev-parse", target]).ok_or("cannot read fetched default branch")?;
    let dirty = git(
        &hub.main,
        &["status", "--porcelain", "--untracked-files=no"],
    )
    .ok_or("cannot read main's tracked changes")?;
    if !dirty.is_empty() {
        return Err(format!(
            "local {} has tracked changes; it was not moved",
            hub.default
        ));
    }
    if local == remote {
        return Ok(());
    }
    if git(&hub.main, &["merge-base", "--is-ancestor", &local, &remote]).is_none() {
        return Err(format!(
            "local {} has commits origin/{} lacks; it was not moved; reconcile it with origin first",
            hub.default, hub.default
        ));
    }
    run(
        Path::new("git"),
        &hub.main,
        &["merge", "--ff-only", &remote],
        scrub,
    )
    .map(|_| ())
    .map_err(|error| format!("local {} could not fast-forward: {error}", hub.default))
}

fn remote_git(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("git did not start: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

pub(super) fn land(task: &Task, hub: &Hub, wt: &Path, base: &str) -> Result<String, String> {
    sync_default(hub, base, true)?;
    let path = Path::new(&task.path);
    let fetched = git(path, &["rev-parse", base]).ok_or("cannot read fetched origin commit")?;
    // Worktrunk must run its merge hooks before the push, without moving main first.
    let target = format!("swarm-landing/{}", key(&task.repo, &task.branch));
    run(
        Path::new("git"),
        path,
        &["branch", "-f", &target, &fetched],
        true,
    )?;
    let result = (|| {
        let output = run(
            wt,
            path,
            &[
                "merge",
                &target,
                "--no-squash",
                "--no-commit",
                "--no-remove",
                "--ff",
                "--verify",
                "--format",
                "json",
            ],
            true,
        )?;
        let landed = git(path, &["rev-parse", &target]).ok_or("cannot read landing commit")?;
        // Explicit refspec and no force: a concurrent remote update must refuse this push.
        remote_git(path, &["push", "origin", &format!("{landed}:refs/heads/{}", hub.default)])
            .map_err(|error| format!("origin push was refused; the task stays and local {} was not advanced to the task; run `swarm task land` again: {error}", hub.default))?;
        sync_default(hub, &landed, true)
            .map_err(|error| format!("origin accepted {landed}, but local sync failed; the task stays; land again: {error}"))?;
        let removed = run(
            wt,
            &hub.main,
            &["remove", &task.branch, "--foreground", "--format", "json"],
            true,
        ).map_err(|error| format!("origin and local {} accepted {landed}, but task cleanup failed; run `swarm task drop {}`: {error}", hub.default, task.branch))?;
        Ok(format!(
            "{output}\n{removed}\norigin/{} and local {} are at {landed}",
            hub.default, hub.default
        ))
    })();
    let cleanup = run(
        Path::new("git"),
        &hub.main,
        &["branch", "-D", &target],
        true,
    );
    match cleanup {
        Ok(_) => result,
        Err(error) => Err(format!(
            "could not remove temporary landing branch {target}: {error}; landing result: {result:?}"
        )),
    }
}

pub(super) fn pull_request(task: &Task, hub: &Hub, base: &str) -> Result<String, String> {
    let warning = sync_default(hub, base, true).err();
    let path = Path::new(&task.path);
    remote_git(path, &["push", "--set-upstream", "origin", &task.branch])?;
    let gh = |args: &[&str]| -> Result<String, String> {
        let output = Command::new("gh")
            .current_dir(path)
            .args(args)
            .env("GH_PROMPT_DISABLED", "1")
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| format!("gh did not start: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().into())
    };
    // A retry after a lost answer reuses the open PR instead of creating another one.
    let existing = gh(&[
        "pr",
        "list",
        "--head",
        &task.branch,
        "--base",
        &hub.default,
        "--state",
        "open",
        "--json",
        "url",
        "--jq",
        ".[0].url // empty",
    ]);
    let url = match existing {
        Ok(url) if !url.is_empty() => url,
        _ => {
            let title = git(
                path,
                &["log", "--reverse", "--format=%s", &format!("{base}..HEAD")],
            )
            .and_then(|log| log.lines().next().map(str::to_owned))
            .unwrap_or_else(|| task.branch.clone());
            gh(&[
                "pr",
                "create",
                "--base",
                &hub.default,
                "--head",
                &task.branch,
                "--title",
                &title,
                "--body",
                &format!(
                    "Task: {}\n\nCreated by `swarm task land --pr`. The task worktree stays until the PR is merged.",
                    task.branch
                ),
            ])?
        }
    };
    Ok(match warning {
        Some(warning) => format!("{url}\nwarning: {warning}"),
        None => url,
    })
}
