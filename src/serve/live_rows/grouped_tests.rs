use super::*;

#[tokio::test]
async fn synthetic_provider_logs_match_full_rebuild() {
    for codex in [false, true] {
        let fixture = Fixture::new();
        let lines=(0..12).flat_map(|turn| {
            let id=format!("tool-{turn}");
            if codex { vec![codex_line(turn).trim_end().to_owned(),
                json!({"type":"response_item","payload":{"type":"function_call","call_id":id,"name":"shell","arguments":"{\"command\":\"true\"}"}}).to_string(),
                json!({"type":"response_item","payload":{"type":"function_call_output","call_id":id,"output":"done"}}).to_string()]
            } else { vec![
                json!({"type":"user","uuid":format!("user-{turn}"),"message":{"content":"prompt"}}).to_string(),
                json!({"type":"assistant","uuid":format!("thought-{turn}"),"message":{"content":[{"type":"thinking","thinking":"think"}]}}).to_string(),
                json!({"type":"assistant","uuid":format!("call-{turn}"),"message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":"true"}}]}}).to_string(),
                json!({"type":"user","uuid":format!("result-{turn}"),"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"done"}]}}).to_string()]
            }
        }).collect::<Vec<_>>();
        std::fs::write(&fixture.log, lines.join("\n") + "\n").unwrap();
        let log = Log {
            path: fixture.log.clone(),
            format: if codex { "codex" } else { "claude" }.into(),
        };
        let mut state = LiveRows::empty(0, codex);
        let mut start = 0;
        for line in &lines {
            let end = start + line.len() as u64 + 1;
            let page = read_row_window(
                &fixture.app,
                &format!("{}:chair", fixture.session),
                &log,
                start,
                end,
            )
            .await
            .unwrap();
            state.ingest(
                start,
                end,
                &page
                    .records
                    .values()
                    .map(|record| record.event.clone())
                    .collect::<Vec<_>>(),
            );
            row_frame("chat", state.delta());
            start = end;
        }
        let mut full = read_row_window(
            &fixture.app,
            &format!("{}:chair", fixture.session),
            &log,
            0,
            start,
        )
        .await
        .unwrap();
        full.shown = state.shown.clone();
        assert_eq!(state.rows(), full.rows_full());
        assert_eq!(
            state.usage.value(),
            view::usage(&full.records.values().cloned().collect::<Vec<_>>())
        );
    }
}

#[test]
fn two_hundred_fifty_grouped_reads_make_three_stable_folds() {
    let records = (0..250)
        .map(|number| {
            let event = json!({
                "type":"tool_call", "tool_call_id":format!("read-{number}"),
                "name":"Read", "input":{"file_path":format!("file-{number}")},
                "status":"in_progress", "group":"read_search", "group_part":"read"
            });
            Record::new(event, format!("raw-{number}-0"), false)
        })
        .collect::<Vec<_>>();

    let rows = view::view_rows(&records, false, false, &HashSet::new());

    assert_eq!(
        rows.iter().map(|row| row.steps.len()).collect::<Vec<_>>(),
        [100, 100, 50]
    );
    let first = view::view_rows(&records[..100], false, false, &HashSet::new());
    assert_eq!(rows[0].id, first[0].id);
}
