//! `swarm task land | drop [<branch>] [--force] | list | sweep [--apply]`: a chat's own worktree, which the
//! app made when the chat opened, landed by the swarm server.

use super::*;
use std::time::{Duration, Instant};

const TASK_USAGE: &str =
    "usage: swarm task land [<name>] [--merge] | drop [<name>] [--force] | list | sweep [--apply]";
/// A Claude shell call stops at 2 minutes, so land waits less, and running it again keeps waiting.
const LAND_WAIT: Duration = Duration::from_secs(100);

pub(crate) fn task(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let cwd = env::current_dir()?;
    if let [verb, flags @ ..] = args
        && verb == "sweep"
        && (flags.is_empty() || matches!(flags, [flag] if flag == "--apply"))
    {
        let min_age = env::var("SWARM_TEST_SWEEP_MIN_AGE_SECONDS")
            .ok()
            .filter(|_| cfg!(debug_assertions))
            .and_then(|value| value.parse().ok())
            .unwrap_or(3600);
        let report =
            swarm::task::sweep(&root, &cwd, !flags.is_empty(), Duration::from_secs(min_age))?;
        print_json(&report)?;
        if report
            .worktrees
            .iter()
            .any(|entry| entry.verdict == "error")
        {
            return Err("swarm: some worktrees or task records could not be removed".into());
        }
        return Ok(());
    }
    let wt = swarm::task::wt_program();
    let value = match args {
        [verb, rest @ ..] if verb == "land" => {
            let flag_count = |flag: &str| rest.iter().filter(|arg| arg.as_str() == flag).count();
            let merge = flag_count("--merge") > 0;
            // A pull request is the default, so `--pr` only names it.
            let pr = !merge;
            let names: Vec<_> = rest
                .iter()
                .filter(|arg| !matches!(arg.as_str(), "--pr" | "--merge"))
                .collect();
            if names.len() > 1
                || names.first().is_some_and(|name| name.starts_with('-'))
                || flag_count("--pr") > 1
                || flag_count("--merge") > 1
                || (merge && flag_count("--pr") > 0)
            {
                return Err(TASK_USAGE.into());
            }
            let name = names.first().map(|name| name.as_str());
            let (request, branch) = if pr {
                swarm::task::request_pull_request(&root, &cwd, name)?
            } else {
                swarm::task::request_landing(&root, &cwd, name)?
            };
            let deadline = Instant::now() + LAND_WAIT;
            loop {
                if let Some(answer) = swarm::task::take_result(&request) {
                    if answer["ok"] != true {
                        return Err(format!(
                            "swarm: landing failed: {}",
                            answer["error"].as_str().unwrap_or_default()
                        )
                        .into());
                    }
                    break answer;
                }
                if Instant::now() >= deadline {
                    let why = if request.exists() {
                        "no swarm server took the request; open the Swarm app, then run"
                    } else {
                        "swarm is still landing it; to keep waiting, run"
                    };
                    // With the name, the command works again after the worktree is gone.
                    return Err(format!(
                        "swarm: {why} `swarm task land {}{}` again",
                        branch,
                        if pr { "" } else { " --merge" }
                    )
                    .into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        [verb, rest @ ..] if verb == "drop" => {
            let force = rest.iter().any(|arg| arg == "--force");
            let names: Vec<_> = rest.iter().filter(|arg| *arg != "--force").collect();
            if names.len() > 1
                || names.first().is_some_and(|name| name.starts_with('-'))
                || rest.len() > names.len() + 1
            {
                return Err(TASK_USAGE.into());
            }
            let name = names.first().map(|name| name.as_str());
            if force && swarm::task::agent_shell() {
                return Err(format!(
                    "swarm: `--force` is the owner's command, and an agent cannot run it; ask the owner to run `swarm task drop{} --force` in their own terminal",
                    name.map(|name| format!(" {name}")).unwrap_or_default()
                )
                .into());
            }
            let task = swarm::task::drop(&root, &wt, &cwd, name, force)?;
            serde_json::json!({"dropped": task.branch, "path": task.path})
        }
        [verb] if verb == "list" => {
            let repo = swarm::task::hub(&cwd)
                .ok_or("swarm: this folder is not in a worktree hub")?
                .common
                .to_string_lossy()
                .into_owned();
            let tasks: Vec<_> = swarm::task::records(&root)
                .into_iter()
                .filter(|task| task.repo == repo)
                .collect();
            serde_json::json!({"tasks": tasks})
        }
        _ => return Err(TASK_USAGE.into()),
    };
    print_json(&value)
}
