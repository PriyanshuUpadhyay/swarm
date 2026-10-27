use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    process::Command,
};
use swarm::view::transcript::{self as transcript, Record, Row, RowKind};
use swarm::view::{self, DiffPreview, TextPreview, ToolState};

fn records(events: Vec<Value>, codex: bool) -> Vec<Record> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| Record::new(event, format!("raw-{index}"), codex))
        .collect()
}
fn build(events: Vec<Value>) -> Vec<Row> {
    transcript::rows(&records(events, false), false, false)
}
fn chunk(kind: &str, text: &str, uuid: &str) -> Value {
    json!({"type": kind, "text":text, "meta":{"uuid":uuid}})
}
fn user(text: &str, uuid: &str) -> Value {
    chunk("user_message_chunk", text, uuid)
}
fn assistant(text: &str, uuid: &str) -> Value {
    chunk("agent_message_chunk", text, uuid)
}
fn system(kind: &str, text: &str, uuid: &str, parent: &str) -> Value {
    json!({"type":"system_message", "kind":kind, "text":text, "meta":{"uuid":uuid,"parent_uuid":parent}})
}
fn call(id: &str, name: &str, input: Value) -> Value {
    json!({"type":"tool_call","tool_call_id":id,"name":name,"input":input,"status":"pending"})
}
fn update(id: &str, status: &str, output: &str) -> Value {
    json!({"type":"tool_call_update","tool_call_id":id,"status":status,"content":output})
}
fn ended(reason: &str, millis: Option<i64>) -> Value {
    json!({"type":"turn_ended","reason":reason,"duration_ms":millis})
}
fn diff(id: &str, path: &str) -> Value {
    json!({"type":"tool_diff","tool_call_id":id,"path":path,"hunks":[{"old_start":1,"old_lines":1,"new_start":1,"new_lines":1,"lines":["-old","+new"]}]})
}
fn tool(row: &Row) -> &view::ToolActivity {
    row.tool.as_ref().unwrap()
}
fn kinds(rows: &[Row]) -> Vec<RowKind> {
    rows.iter().map(|row| row.kind).collect()
}
fn texts(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.text.as_str()).collect()
}
fn outputs(rows: &[Row]) -> Vec<&str> {
    rows.iter()
        .filter_map(|row| {
            row.tool
                .as_ref()?
                .output
                .as_ref()
                .map(|preview| preview.text.as_str())
        })
        .collect()
}
fn shell_input(command: &str, uuid: &str) -> Value {
    system(
        "shell_input",
        &format!("<bash-input>{command}</bash-input>"),
        uuid,
        "",
    )
}
fn shell_output(output: &str, parent: &str) -> Value {
    system("shell_output", output, "", parent)
}
fn fixture(format: &str, name: &str) -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(root.join("packages/transcript/zig-out/bin/transcript"))
        .args(["--format", format])
        .arg(root.join("packages/transcript/src/fixtures").join(name))
        .output()
        .expect("cannot run transcript; run `zig build` in packages/transcript first");
    assert!(output.status.success());
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

fn in_session(mut event: Value, session: &str) -> Value {
    event["meta"] = json!({"session_id":session});
    event
}
fn skill_body(kind: &str, text: &str, call_id: &str) -> Value {
    let mut event = system(kind, text, "body", "tool-result");
    event["meta"]["source_tool_use_id"] = json!(call_id);
    event
}
const RING: &str =
    "swarm: new message. Run swarm inbox and read each body at /home/owner/.swarm/<body_path>.";
fn inline_fixture(format: &str, lines: &[&str]) -> Vec<Value> {
    let path =
        std::env::temp_dir().join(format!("swarm-rows-fixture-{}.jsonl", uuid::Uuid::now_v7()));
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let output = Command::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/transcript/zig-out/bin/transcript"),
    )
    .args(["--format", format])
    .arg(&path)
    .output();
    std::fs::remove_file(path).unwrap();
    let output =
        output.expect("cannot run transcript; run `zig build` in packages/transcript first");
    assert!(output.status.success());
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

#[path = "view_rows/codex_and_records.rs"]
mod codex_and_records;
#[path = "view_rows/commands.rs"]
mod commands;
#[path = "view_rows/previews_and_labels.rs"]
mod previews_and_labels;
#[path = "view_rows/rows_and_tools.rs"]
mod rows_and_tools;
#[path = "view_rows/turns_and_folds.rs"]
mod turns_and_folds;
