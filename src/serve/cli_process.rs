//! Server cli process.

use super::*;

pub(super) async fn run_cli(
    app: &Server,
    call: CliCall,
    text: Option<String>,
) -> Result<String, ApiError> {
    let timeout = if call.args.first().is_some_and(|arg| arg == "launch") {
        60
    } else {
        20
    };
    run_cli_with_timeout(app, call, text, Duration::from_secs(timeout)).await
}

pub(super) async fn run_cli_with_timeout(
    app: &Server,
    call: CliCall,
    text: Option<String>,
    timeout: Duration,
) -> Result<String, ApiError> {
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new(&app.cli);
    // Match ChildProcessEnvironment.swarmCall so key and answer remain owner actions even when
    // serve was opened from an agent pane. Identity never comes from the HTTP request.
    for (name, _) in std::env::vars_os() {
        let key = name.to_string_lossy();
        if [
            "CLAUDECODE",
            "CLAUDE_PID",
            "AI_AGENT",
            "SWARM_SESSION_ID",
            "SWARM_AGENT_ID",
            "SWARM_ADAPTER",
            "SWARM_PANE",
            "TMUX_PANE",
            "SWARM_PTY_PANE",
            "SWARM_PTY_HOST",
            "SUPERSET_TERMINAL_ID",
            "REX_BLOCK",
        ]
        .contains(&key.as_ref())
            || key.starts_with("HERDR_")
            || key.starts_with("CLAUDE_CODE_")
        {
            command.env_remove(name);
        }
    }
    // A chat of a removed worktree keeps its gone folder as cwd, and a spawn there fails, so its
    // close or archive runs from the swarm home; SWARM_SESSION_ID still names the chat.
    let cwd = if call.cwd.is_dir() {
        &call.cwd
    } else {
        &app.root
    };
    command
        .args(&call.args)
        .current_dir(cwd)
        .env("PWD", cwd)
        .env("SWARM_HOME", app.root.parent().expect("swarm home"))
        .env("SWARM_ADAPTER", &call.adapter)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    if let Some(session) = &call.session {
        command
            .env("SWARM_SESSION_ID", session)
            .env("SWARM_AGENT_ID", "orchestrator");
    }
    let mut child = command
        .spawn()
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let mut group = ActionProcessGroup(Some(child.id().expect("running child") as i32));
    let result = tokio::time::timeout(timeout, async {
        let mut stdin = child.stdin.take().expect("piped stdin");
        if let Some(text) = text {
            stdin.write_all(text.as_bytes()).await?;
        }
        drop(stdin);
        child.wait_with_output().await
    })
    .await;
    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Err(error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()));
        }
        Err(_) => {
            return Err(error(StatusCode::GATEWAY_TIMEOUT, "swarm action timed out"));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let message = failure_text(&output);
        return Err(error(
            StatusCode::CONFLICT,
            if message.is_empty() {
                "swarm action failed"
            } else {
                &message
            },
        ));
    }
    // Launch can create a detached pane. Only failed or cancelled calls stop their process group.
    group.0 = None;
    Ok(stdout)
}

pub(super) struct ActionProcessGroup(pub(super) Option<i32>);

impl Drop for ActionProcessGroup {
    fn drop(&mut self) {
        if let Some(group) = self.0 {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            // SAFETY: the child has its own process group; kill takes plain integers only.
            unsafe {
                kill(-group, 9);
            }
        }
    }
}

/// What a failed exit says: its stderr, else its stdout, trimmed.
pub(super) fn failure_text(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout)
    } else {
        stderr
    };
    text.trim().to_string()
}

#[cfg(test)]
#[path = "cli_process/tests.rs"]
mod tests;
