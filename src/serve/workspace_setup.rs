//! Server workspace setup.

use super::*;

pub(super) fn setup_frame(app: &App, workspace: &str, payload: Value) {
    let mut payload = payload;
    payload["workspace"] = workspace.into();
    app.hub.lock().unwrap().publish(Frame::new(
        "setup",
        payload,
        Some(&format!("setup:{workspace}")),
    ));
}
pub(super) fn setup_text(pending: &mut Vec<u8>, bytes: &[u8], eof: bool) -> String {
    pending.extend_from_slice(bytes);
    let mut text = String::new();
    let mut consumed = 0;
    while consumed < pending.len() {
        match std::str::from_utf8(&pending[consumed..]) {
            Ok(rest) => {
                text.push_str(rest);
                consumed = pending.len();
            }
            Err(err) => {
                let valid = consumed + err.valid_up_to();
                text.push_str(std::str::from_utf8(&pending[consumed..valid]).unwrap());
                consumed = valid;
                if let Some(size) = err.error_len() {
                    text.push('\u{fffd}');
                    consumed += size;
                } else if eof {
                    text.push('\u{fffd}');
                    consumed = pending.len();
                } else {
                    break;
                }
            }
        }
    }
    pending.drain(..consumed);
    text
}
pub(super) async fn run_setup(
    app: &App,
    workspace: &str,
    script: &str,
    timeout: Duration,
) -> Value {
    use tokio::io::AsyncReadExt;
    setup_frame(app, workspace, json!({"state":"running"}));
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .args(["-c", script])
        .current_dir(workspace)
        .env("PWD", workspace)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    let result = match command.spawn() {
        Err(err) => json!({"state":"failed","exit_code":null,"error":err.to_string()}),
        Ok(mut child) => {
            let mut group = ActionProcessGroup(Some(child.id().expect("running setup") as i32));
            let mut stdout = child.stdout.take().expect("piped setup stdout");
            let mut stderr = child.stderr.take().expect("piped setup stderr");
            let finished = tokio::time::timeout(timeout, async {
                let mut out_buffer = [0; 4096];
                let mut err_buffer = [0; 4096];
                let (mut out_pending, mut err_pending) = (Vec::new(), Vec::new());
                let (mut out_open, mut err_open) = (true, true);
                let mut status = None;
                while out_open || err_open || status.is_none() {
                    tokio::select! {
                        waited = child.wait(), if status.is_none() => {
                            status = Some(waited?);
                            // Close inherited pipes before draining the last output.
                            drop(ActionProcessGroup(group.0.take()));
                        }
                        read = stdout.read(&mut out_buffer), if out_open => {
                            let size = read?;
                            out_open = size > 0;
                            let text = setup_text(&mut out_pending, &out_buffer[..size], !out_open);
                            if !text.is_empty() { setup_frame(app, workspace, json!({"state":"output","channel":"stdout","text":text})); }
                        }
                        read = stderr.read(&mut err_buffer), if err_open => {
                            let size = read?;
                            err_open = size > 0;
                            let text = setup_text(&mut err_pending, &err_buffer[..size], !err_open);
                            if !text.is_empty() { setup_frame(app, workspace, json!({"state":"output","channel":"stderr","text":text})); }
                        }
                    }
                }
                Ok::<_, std::io::Error>(status.expect("setup was reaped"))
            }).await;
            drop(ActionProcessGroup(group.0.take()));
            match finished {
                Ok(Ok(status)) => {
                    json!({"state":if status.success() {"completed"} else {"failed"},"exit_code":status.code()})
                }
                other => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    match other {
                        Err(_) => {
                            json!({"state":"timeout","exit_code":null,"error":"Setup exceeded its time limit"})
                        }
                        Ok(Err(err)) => {
                            json!({"state":"failed","exit_code":null,"error":err.to_string()})
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    };
    setup_frame(app, workspace, result.clone());
    result
}

#[cfg(test)]
#[path = "workspace_setup/tests.rs"]
mod tests;
