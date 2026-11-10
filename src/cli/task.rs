//! `swarm task start <name> | land | drop [<branch>] | list` (ADR 0135): an agent's own task
//! worktree, made from the default branch by Worktrunk and landed by the swarm server.

use super::*;
use std::time::{Duration, Instant};

const TASK_USAGE: &str = "usage: swarm task start <name> | land [<name>] | drop [<name>] | list";
/// A Claude shell call stops at 2 minutes, so land waits less, and running it again keeps waiting.
const LAND_WAIT: Duration = Duration::from_secs(100);

pub(crate) fn task(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let cwd = env::current_dir()?;
    let wt = swarm::task::wt_program();
    let value = match args {
        [verb, name] if verb == "start" => {
            let owner = (
                env::var("SWARM_SESSION_ID").ok(),
                env::var("SWARM_AGENT_ID").ok(),
            );
            let (task, warning) = swarm::task::start(&root, &wt, &cwd, name, owner)?;
            serde_json::json!({"branch": task.branch, "path": task.path, "warning": warning})
        }
        [verb, rest @ ..] if verb == "land" && rest.len() <= 1 => {
            let name = rest.first().map(String::as_str);
            let (request, branch) = swarm::task::request_landing(&root, &cwd, name)?;
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
                    return Err(format!("swarm: {why} `swarm task land {}` again", branch).into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        [verb, rest @ ..] if verb == "drop" && rest.len() <= 1 => {
            let task = swarm::task::drop(&root, &wt, &cwd, rest.first().map(String::as_str))?;
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
