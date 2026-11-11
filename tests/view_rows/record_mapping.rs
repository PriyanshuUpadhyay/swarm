use super::*;

fn codex_rows(lines: Vec<Value>) -> Vec<Row> {
    let bytes = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let events = translated("codex", Path::new(""), bytes.as_bytes());
    transcript::rows(&records(events, true), false, true)
}

fn response(payload: Value) -> Value {
    json!({"type":"response_item", "payload":payload})
}

fn event(payload: Value) -> Value {
    json!({"type":"event_msg", "payload":payload})
}

fn completed(item: Value) -> Value {
    event(json!({"type":"item_completed", "item":item}))
}

fn apply_patch_call() -> Value {
    response(
        json!({"type":"custom_tool_call", "call_id":"patch", "name":"apply_patch", "input":"*** Begin Patch\n*** End Patch"}),
    )
}

#[test]
fn codex_file_changes_join_the_patch_call() {
    for source in ["patch_apply_end", "FileChange"] {
        let changes = json!({
            "new.txt":{"type":"add","content":"new\n"},
            "old.txt":{"type":"delete","content":"old\n"},
            "edit.txt":{"type":"update","unified_diff":"@@ -2,1 +2,1 @@\n-before\n+after\n","move_path":null}
        });
        let update = if source == "FileChange" {
            completed(
                json!({"type":source,"id":"patch","status":"completed","changes":changes,"stdout":"done"}),
            )
        } else {
            event(
                json!({"type":source,"call_id":"patch","success":true,"status":"completed","changes":changes,"stdout":"done"}),
            )
        };
        let rows = codex_rows(vec![apply_patch_call(), update]);
        assert_eq!(rows.len(), 1, "{source}");
        let tool = tool(&rows[0]);
        assert_eq!(tool.state, ToolState::Finished);
        assert_eq!(
            tool.diffs
                .iter()
                .map(|diff| (diff.path.as_str(), diff.added, diff.removed))
                .collect::<Vec<_>>(),
            [("new.txt", 1, 0), ("old.txt", 0, 1), ("edit.txt", 1, 1)]
        );
        assert!(tool.diffs[2].patch.contains("-before\n+after"));
    }
}
