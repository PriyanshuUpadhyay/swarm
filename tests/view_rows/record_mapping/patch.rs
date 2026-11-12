use super::*;

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
            "src/a.rs":{"type":"update","unified_diff":"@@ -2,1 +2,1 @@\n-before\n+after\n","move_path":"src/b.rs"}
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
            [("new.txt", 1, 0), ("old.txt", 0, 1), ("src/b.rs", 1, 1)]
        );

        assert_eq!(tool.diffs[2].old_path.as_deref(), Some("src/a.rs"));
        assert!(tool.diffs[2].patch.contains("-before\n+after"));
        assert!(
            tool.diffs[2]
                .patch
                .starts_with("diff --git \"a/src/a.rs\" \"b/src/b.rs\"\n")
        );
    }
}

#[test]
fn codex_interrupted_patches_show_interrupted() {
    let rows = codex_rows(vec![
        apply_patch_call(),
        event(
            json!({"type":"patch_apply_end","call_id":"patch","status":"interrupted","success":false}),
        ),
    ]);

    assert_eq!(kinds(&rows), [RowKind::ToolUse]);
    assert_eq!(tool(&rows[0]).state, ToolState::Interrupted);
}
