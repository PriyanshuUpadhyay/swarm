//! Tests for the background task readers.

use super::*;

fn line(value: Value) -> String {
    format!("{value}\n")
}

fn launch(call: &str, result: Value, text: &str) -> String {
    line(json!({
        "type": "user",
        "message": {"role": "user", "content": [{"tool_use_id": call, "type": "tool_result", "content": text}]},
        "toolUseResult": result,
    }))
}

fn call(id: &str, name: &str, input: Value) -> String {
    line(json!({
        "type": "assistant",
        "message": {"content": [{"type": "tool_use", "id": id, "name": name, "input": input}]},
    }))
}

fn notification(id: &str, status: &str) -> String {
    line(json!({
        "type": "queue-operation",
        "content": format!("<task-notification>\n<task-id>{id}</task-id>\n<status>{status}</status>\n<summary>Background command \"x\" {status}</summary>\n</task-notification>"),
    }))
}

#[test]
fn a_log_names_shells_and_subagents_with_their_end_state() {
    let dir = std::env::temp_dir().join(format!("swarm-bg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let live_file = dir.join("b2.output");
    std::fs::write(&live_file, "still going\n").unwrap();
    let log = [
        call("t1", "Bash", json!({"command": "sleep 30", "description": "Sleep for 30 seconds", "run_in_background": true})),
        launch("t1", json!({"backgroundTaskId": "b1"}), "Command running in background with ID: b1. Output is being written to: /tmp/claude-1/x/s/tasks/b1.output. You will be notified when it completes."),
        // Moved to the background by Ctrl+B or a timeout: no run_in_background in the input.
        call("t2", "Bash", json!({"command": "cargo test"})),
        launch("t2", json!({"backgroundTaskId": "b2"}), &format!("Command running in background with ID: b2. Output is being written to: {}. You will be notified.", live_file.display())),
        call("t3", "Agent", json!({"description": "ignored", "run_in_background": true})),
        launch("t3", json!({"isAsync": true, "agentId": "a3", "description": "Research speakers"}), "Async agent launched successfully.\nagentId: a3\noutput_file: /tmp/claude-1/x/s/tasks/a3.output\nDo NOT Read"),
        "not json\n".into(),
        launch("t0", json!({"backgroundTaskId": ""}), "Output is being written to: /tmp/claude-1/x/s/tasks/.output."),
        // A Read of a file that quotes a notice is not an end record.
        line(json!({
            "type": "user",
            "message": {"role": "user", "content": [{"tool_use_id": "t9", "type": "tool_result", "content": "<task-notification>\n<task-id>b2</task-id>\n<status>killed</status>\n</task-notification>"}]},
        })),
        notification("b1", "completed"),
    ]
    .concat();

    let found = tasks("orchestrator", &log);
    let brief: Vec<_> = found
        .iter()
        .map(|t| (t.id.as_str(), t.kind, t.title.as_str(), t.status.as_str()))
        .collect();
    assert_eq!(
        brief,
        [
            ("b1", TaskKind::Shell, "Sleep for 30 seconds", "completed"),
            ("b2", TaskKind::Shell, "cargo test", "running"),
            ("a3", TaskKind::Agent, "Research speakers", "unknown"),
        ]
    );
    assert_eq!(
        found[0].summary.as_deref(),
        Some("Background command \"x\" completed")
    );
    assert_eq!(
        found[0].output.as_deref(),
        Some(Path::new("/tmp/claude-1/x/s/tasks/b1.output"))
    );
    assert_eq!(
        found[2].output.as_deref(),
        Some(Path::new("/tmp/claude-1/x/s/tasks/a3.output"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Each task as (id, title, status, start time), for one assert per reader.
fn brief(found: &[Task]) -> Vec<(&str, &str, &str, Option<&str>)> {
    found
        .iter()
        .map(|t| {
            (
                t.id.as_str(),
                t.title.as_str(),
                t.status.as_str(),
                t.started_at.as_deref(),
            )
        })
        .collect()
}

#[test]
fn a_codex_log_names_a_running_background_terminal() {
    let record = |at: &str, payload: Value| {
        line(json!({"timestamp": at, "type": "response_item", "payload": payload}))
    };
    let call = |at: &str, id: &str, name: &str, args: Value| {
        record(
            at,
            json!({"type": "function_call", "name": name, "arguments": args.to_string(), "call_id": id}),
        )
    };
    let output = |at: &str, id: &str, text: &str| {
        record(
            at,
            json!({"type": "function_call_output", "call_id": id, "output": text}),
        )
    };

    let log = [
        // The answer comes after the wait, so the task starts at the call's time.
        call("2026-11-11T10:00:00Z", "c1", "exec_command", json!({"cmd": "npm run dev"})),
        output("2026-11-11T10:00:10Z", "c1", "Chunk ID: 1\nWall time: 10.0 seconds\nProcess running with session ID 1001\nOutput:\nready"),
        call("2026-11-11T10:01:00Z", "c2", "exec_command", json!({"cmd": "make build"})),
        output("2026-11-11T10:01:10Z", "c2", "Wall time: 10.0 seconds\nProcess running with session ID 1002\nOutput:\n"),
        call("2026-11-11T10:02:00Z", "c3", "write_stdin", json!({"session_id": 1002, "chars": ""})),
        output("2026-11-11T10:02:05Z", "c3", "Wall time: 5.0 seconds\nProcess exited with code 0\nOutput:\ndone"),
        // A command that exited, whose own output prints the running phrase, is no task.
        call("2026-11-11T10:03:00Z", "c4", "exec_command", json!({"cmd": "rg session src"})),
        output("2026-11-11T10:03:01Z", "c4", "Wall time: 0.1 seconds\nProcess exited with code 0\nOutput:\nProcess running with session ID 1003"),
    ]
    .concat();

    let found = codex_tasks("chair", &log);
    assert_eq!(
        brief(&found),
        [
            (
                "1001",
                "npm run dev",
                "running",
                Some("2026-11-11T10:00:00Z")
            ),
            (
                "1002",
                "make build",
                "completed",
                Some("2026-11-11T10:01:00Z")
            ),
        ]
    );
}

#[test]
fn an_agy_log_names_a_running_background_task_until_its_system_message() {
    let (done, failed, running) = (
        "56a8a353-52b9-4882-94bc-9f2d683ce0ef/task-32",
        "56a8a353-52b9-4882-94bc-9f2d683ce0ef/task-50",
        "56a8a353-52b9-4882-94bc-9f2d683ce0ef/task-60",
    );

    let start = |step: u32, id: &str, what: &str| {
        line(json!({
            "step_index": step, "source": "MODEL", "type": "GENERIC", "status": "RUNNING",
            "created_at": "2026-10-23T04:45:32Z",
            "content": format!("Created At: 2026-10-23T10:15:32+05:30\nTool is running as a background task with task id: {id}\nTask Description: {what}\nTask logs are available at: file:///x/.system_generated/tasks/task-32.log\nYOU MUST TAKE ONE OF THE FOLLOWING TWO ACTIONS: A) either proceed or, B) wait."),
        }))
    };

    // AGY ends the exit code with a period.
    let end = |id: &str, code: u32| {
        line(json!({
            "step_index": 40, "source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE",
            "content": format!("[Message] timestamp=2026-10-23T04:46:12Z sender={id} priority=MESSAGE_PRIORITY_HIGH content=Task id \"{id}\" finished with result:\nCommand exited with code {code}."),
        }))
    };

    let log = [
        start(32, done, "sleep 40"),
        start(50, failed, "make build"),
        start(60, running, "npm run dev"),
        end(done, 0),
        end(failed, 1),
    ]
    .concat();

    let found = agy_tasks("chair", &log);
    let at = Some("2026-10-23T04:45:32Z");
    assert_eq!(
        brief(&found),
        [
            (done, "sleep 40", "completed", at),
            (failed, "make build", "failed", at),
            (running, "npm run dev", "running", at),
        ]
    );
}
