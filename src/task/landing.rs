//! Landing a task (ADR 0135): an agent leaves a request in `<swarm home>/landings/`, and the
//! swarm server, outside the agent's sandbox, merges it into main with Worktrunk.

use super::*;

/// `swarm task land`, agent side: check the task and leave a request for the server.
pub fn request_landing(root: &Path, cwd: &Path, name: Option<&str>) -> Result<PathBuf, String> {
    let dir = root.join("landings");
    let (_, task) = match find_task(root, cwd, name) {
        Ok(found) => found,
        Err(error) => {
            // A land run again after a landing ended finds no record, but the answer is there.
            if let (Some(name), Some(hub)) = (name, hub(cwd)) {
                let branch = branch_name(name);
                let request = dir.join(format!(
                    "{}.json",
                    key(&hub.common.to_string_lossy(), &branch)
                ));
                if request.with_extension("result.json").exists() {
                    return Ok(request);
                }
            }
            return Err(error);
        }
    };
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
        return Err(format!(
            "{} has no commit that {} lacks, so there is nothing to land; commit first, or run `swarm task drop`",
            task.branch, task.default
        ));
    }
    let name = key(&task.repo, &task.branch);
    let request = dir.join(format!("{name}.json"));
    // A land run again after its wait ran out keeps waiting on the first request.
    if dir.join(format!("{name}.taking")).exists() {
        return Ok(request);
    }
    // The record is still here, so an answer left here is from an earlier task of this name, or a
    // failed landing that this run tries again.
    let _ = std::fs::remove_file(request.with_extension("result.json"));
    write_json(&request, &task).map_err(|error| error.to_string())?;
    Ok(request)
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
    let lock = dir.join(".lock");
    // A server that died mid-landing leaves its lock; a landing never takes this long.
    if std::fs::metadata(&lock)
        .and_then(|meta| meta.modified())
        .is_ok_and(|at| at.elapsed().is_ok_and(|age| age.as_secs() > 1800))
    {
        let _ = std::fs::remove_file(&lock);
    }
    if std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .is_err()
    {
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
            Ok((task, output)) => {
                let _ = write_json(
                    &request.with_extension("result.json"),
                    &serde_json::json!({"ok": true, "output": output}),
                );
                // After the answer, so a land run again finds either the record or the answer.
                let _ = std::fs::remove_file(record_path(root, &task.repo, &task.branch));
                None
            }
            Err(error) => Some(serde_json::json!({"ok": false, "error": error})),
        };
        if let Some(answer) = answer {
            let _ = write_json(&request.with_extension("result.json"), &answer);
        }
        let _ = std::fs::remove_file(&taking);
    }
    let _ = std::fs::remove_file(&lock);
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

fn land(root: &Path, wt: &Path, request: &Path) -> Result<(Task, String), String> {
    let text = std::fs::read_to_string(request).map_err(|error| error.to_string())?;
    let asked: Task = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    // Only a task that `swarm task start` recorded lands, and only from its own folder.
    let task = read_record(root, &asked.repo, &asked.branch)
        .filter(|task| task.path == asked.path && task.branch.starts_with(PREFIX))
        .ok_or("no task record matches this request")?;
    let output = run(
        wt,
        Path::new(&task.path),
        &[
            "merge",
            &task.default,
            "--no-squash",
            "--no-commit",
            "--format",
            "json",
        ],
        true,
    )?;
    Ok((task, output))
}
