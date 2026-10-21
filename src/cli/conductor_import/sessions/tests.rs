use super::*;

#[test]
fn log_line_turns_conductor_rows_into_claude_records() {
    let user = log_line("user", "plan it", "m1", "2026-06-22T08:24:58.733Z").unwrap();
    assert_eq!(
        user,
        json!({"type": "user", "uuid": "m1",
               "timestamp": "2026-06-22T08:24:58.733Z",
               "message": {"role": "user", "content": "plan it"}})
    );
    let answer = r#"{"type":"user","session_id":"t","parent_tool_use_id":null,
        "message":{"role":"user","content":[]},"tool_use_result":{"answers":{}}}"#;
    let record = log_line("assistant", answer, "m2", "at").unwrap();
    assert!(
        record.get("session_id").is_none(),
        "the rebuilt file names the session"
    );
    assert_eq!(record["uuid"], "m2");
    assert_eq!(record["toolUseResult"], json!({"answers": {}}));
    assert!(record.get("tool_use_result").is_none());
    let subagent = r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{}}"#;
    assert_eq!(log_line("assistant", subagent, "m3", "at"), None);
    assert_eq!(
        log_line("assistant", r#"{"type":"result"}"#, "m4", "at"),
        None
    );
    assert_eq!(log_line("assistant", "not json", "m5", "at"), None);
}

#[test]
fn claude_log_chains_each_record_to_the_one_before() {
    let records = vec![
        json!({"type": "user", "uuid": "a", "message": {"content": "hi"}}),
        json!({"type": "assistant", "uuid": "b", "message": {"content": []}}),
    ];
    let lines: Vec<Value> = claude_log(records, "/w", "sid")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["parentUuid"], Value::Null);
    assert_eq!(lines[1]["parentUuid"], "a");
    assert_eq!(
        (
            &lines[1]["sessionId"],
            &lines[1]["cwd"],
            &lines[1]["isSidechain"]
        ),
        (&json!("sid"), &json!("/w"), &json!(false))
    );
}

#[test]
fn codex_log_maps_claude_blocks_to_codex_items() {
    let records = vec![
        json!({"type": "user", "timestamp": "t0", "message": {"content": "review it"}}),
        json!({"type": "assistant", "timestamp": "t1", "message": {"content": [
            {"type": "text", "text": "looking"},
            {"type": "tool_use", "id": "c1", "name": "Bash", "input": {"command": "git status"}},
        ]}}),
        json!({"type": "user", "timestamp": "t2", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "c1", "content": [{"type": "text", "text": "clean"}]},
        ]}}),
    ];
    let items: Vec<Value> = codex_log(records, "/w", "tid")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let kinds: Vec<_> = items
        .iter()
        .map(|i| {
            format!(
                "{}/{}",
                i["type"].as_str().unwrap(),
                i["payload"]["type"].as_str().unwrap_or("")
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "session_meta/",
            "response_item/message",
            "event_msg/user_message",
            "response_item/message",
            "event_msg/agent_message",
            "response_item/function_call",
            "response_item/function_call_output",
        ]
    );
    assert_eq!(
        (
            &items[0]["payload"]["id"],
            &items[0]["payload"]["cwd"],
            &items[0]["timestamp"]
        ),
        (&json!("tid"), &json!("/w"), &json!("t0"))
    );
    assert_eq!(items[5]["payload"]["name"], "exec_command");
    assert_eq!(items[5]["payload"]["arguments"], r#"{"cmd":"git status"}"#);
    assert_eq!(
        (
            &items[6]["payload"]["call_id"],
            &items[6]["payload"]["output"]
        ),
        (&json!("c1"), &json!("clean"))
    );
}
