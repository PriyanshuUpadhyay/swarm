//! Landing a task (ADR 0135): an agent leaves a request in `<swarm home>/landings/`, and the
//! swarm server, outside the agent's sandbox, merges it into main with Worktrunk.

use super::*;

#[derive(serde::Serialize, serde::Deserialize)]
struct LandingRequest {
    #[serde(flatten)]
    task: Task,
    #[serde(default)]
    pr: bool,
}

/// `swarm task land`, agent side: check the task and leave a request for the server. It returns
/// the request and the task's branch.
pub fn request_landing(
    root: &Path,
    cwd: &Path,
    name: Option<&str>,
) -> Result<(PathBuf, String), String> {
    request(root, cwd, name, false)
}

/// Publish a PR request for the server; the task remains available after it succeeds.
pub fn request_pull_request(
    root: &Path,
    cwd: &Path,
    name: Option<&str>,
) -> Result<(PathBuf, String), String> {
    request(root, cwd, name, true)
}

fn request(
    root: &Path,
    cwd: &Path,
    name: Option<&str>,
    pr: bool,
) -> Result<(PathBuf, String), String> {
    let dir = root.join("landings");
    let (_, task) = find_task(root, cwd, name)?;
    let name = request_key(&task.repo, &task.branch, pr);
    let request = dir.join(format!("{name}.json"));
    // A land run again after its wait ran out keeps waiting on the first request, before the
    // checks below, which a landing in progress can change.
    if dir.join(format!("{name}.taking")).exists() {
        return Ok((request, task.branch));
    }
    let path = Path::new(&task.path);
    if !git(path, &["status", "--porcelain"])
        .ok_or("git status failed")?
        .is_empty()
    {
        return Err("commit your changes first; land takes only commits".into());
    }
    let ahead = git(
        path,
        &["rev-list", "--count", &format!("{}..HEAD", task.default)],
    )
    .ok_or("git rev-list failed")?;
    if ahead == "0" {
        // The record stays after a landing, so a land run again after its wait ran out finds the
        // landed branch and the answer; the answer is returned, not deleted.
        if request.with_extension("result.json").exists() {
            return Ok((request, task.branch));
        }
        return Err(format!(
            "{} has no commit that {} lacks, so there is nothing to land; commit first, or run `swarm task drop`",
            task.branch, task.default
        ));
    }
    // The record is still here, so an answer left here is from an earlier task of this name, or a
    // failed landing that this run tries again.
    let _ = std::fs::remove_file(request.with_extension("result.json"));
    write_json(
        &request,
        &LandingRequest {
            task: task.clone(),
            pr,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok((request, task.branch))
}

fn request_key(repo: &str, branch: &str, pr: bool) -> String {
    let key = key(repo, branch);
    if pr { format!("{key}-pr") } else { key }
}

/// The server's answer to one request, once it is there. Reading it removes it.
pub fn take_result(request: &Path) -> Option<serde_json::Value> {
    let file = request.with_extension("result.json");
    let value = serde_json::from_str(&std::fs::read_to_string(&file).ok()?).ok()?;
    let _ = std::fs::remove_file(file);
    Some(value)
}

/// One pass of the server's landing worker: land each request in turn, under one lock across
/// servers, so two landings never write main at once.
pub fn serve_landings(root: &Path, wt: &Path) {
    let dir = root.join("landings");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut requests: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "json")
                && !path.to_string_lossy().ends_with(".result.json")
        })
        .collect();
    requests.sort();
    if requests.is_empty() && takings(&dir).is_empty() {
        return;
    }
    // The OS drops the lock when its server stops, so a stopped server blocks no landing.
    let Ok(lock) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".lock"))
    else {
        return;
    };
    if lock.try_lock().is_err() {
        return;
    }
    // This server holds the lock, so a `.taking` file is from a server that stopped mid-landing;
    // it goes back in the queue.
    for taking in takings(&dir) {
        if std::fs::rename(&taking, taking.with_extension("json")).is_ok() {
            requests.push(taking.with_extension("json"));
        }
    }
    requests.sort();
    requests.dedup();
    for request in requests {
        let taking = request.with_extension("taking");
        if std::fs::rename(&request, &taking).is_err() {
            continue;
        }
        let answer = match land(root, wt, &taking) {
            Ok((output, next)) => {
                Some(serde_json::json!({"ok": true, "output": output, "next": next}))
            }
            Err(error) => Some(serde_json::json!({"ok": false, "error": error})),
        };
        if let Some(answer) = answer {
            let _ = write_json(&request.with_extension("result.json"), &answer);
        }
        let _ = std::fs::remove_file(&taking);
    }
    std::mem::drop(lock);
}

fn takings(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| Some(entry.ok()?.path()))
                .filter(|path| path.extension().is_some_and(|ext| ext == "taking"))
                .collect()
        })
        .unwrap_or_default()
}

/// The landing's output, and what the agent does next. Code owns this rule (ADR 0151): no landing
/// removes the worktree; the agent asks the owner first.
fn land(root: &Path, wt: &Path, request: &Path) -> Result<(String, String), String> {
    let text = std::fs::read_to_string(request).map_err(|error| error.to_string())?;
    let asked: LandingRequest = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    // Only a chat worktree that swarm recorded lands, and only from its own folder.
    let task = read_record(root, &asked.task.repo, &asked.task.branch)
        .filter(|task| task.path == asked.task.path && task.branch.starts_with(PREFIX))
        .ok_or("no task record matches this request")?;
    // The record is a file an agent can write, so the hub, the target branch, and the task's
    // branch come from git, not from it.
    let path = Path::new(&task.path);
    let hub = hub(path)
        .filter(|hub| hub.common.to_string_lossy() == task.repo)
        .ok_or("the task folder is not in the hub its record names")?;
    if git(path, &["symbolic-ref", "-q", "--short", "HEAD"]).as_deref() != Some(&task.branch) {
        return Err(format!("the task folder is not on {}", task.branch));
    }
    let base = fresh_origin_base(&hub.common, &hub.default)?;
    // The server writes the ref hook too, because a sandboxed agent cannot write the hub's hooks.
    let _ = install_hook(&hub);
    if let Some(base) = base {
        let output = if asked.pr {
            origin::pull_request(&task, &hub, &base)?
        } else {
            origin::land(&task, &hub, wt, &base)?
        };
        return Ok((output, next(&task.branch, asked.pr)));
    }
    if asked.pr {
        return Err(
            "this hub has no origin; use `swarm task land --merge` for a local landing".into(),
        );
    }
    let output = run(
        wt,
        path,
        &[
            "merge",
            &hub.default,
            "--no-squash",
            "--no-commit",
            "--no-remove",
            "--format",
            "json",
        ],
        true,
    )?;
    Ok((output, next(&task.branch, false)))
}

fn next(branch: &str, pr: bool) -> String {
    if pr {
        "the worktree stays; after the PR merges, ask the owner, and after approval run `swarm task sweep --apply` from another folder of the hub (`swarm task drop` refuses a squash or rebase merge; only the owner can run it with `--force`)".into()
    } else {
        format!(
            "the worktree stays; ask the owner, and after approval run `swarm task drop {branch}`"
        )
    }
}
