//! Tests for server row pages.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn older_page_sends_only_new_and_changed_rows_in_thousand_row_history() {
    let fixture = Fixture::new();
    codex_log(&fixture, 0);
    let call = json!({"type":"response_item","payload":{"type":"function_call","call_id":"boundary","name":"shell","arguments":"{\"command\":\"true\"}"}});
    let result = json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"boundary","output":"done"}});
    let history = (0..99).map(codex_line).collect::<String>()
        + &format!("{call}\n{result}\n")
        + &(100..999).map(codex_line).collect::<String>();
    assert_eq!(history.lines().count(), 1000);
    std::fs::write(&fixture.log, history).unwrap();
    let first = row_page(&fixture, 900, None).await.unwrap();
    let orphan = first["rows"][0]["id"].clone();
    let older = row_page(&fixture, 100, first["page"]["start_offset"].as_u64())
        .await
        .unwrap();
    assert_eq!(older["rows"].as_array().unwrap().len(), 100);
    assert_eq!(row_texts(&older).len(), 99);
    assert_eq!(older["rows"][99]["tool"]["state"], "finished");
    assert_eq!(older["remove"], json!([orphan]));
    assert!(
        older["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| !first["rows"].as_array().unwrap().contains(row))
    );
    let mut old_response = older.clone();
    old_response["rows"] = json!(fixture.app.hub.lock().unwrap().row_cache[0].1.last_rows);
    let new_bytes = older.to_string().len();
    let old_bytes = old_response.to_string().len();
    println!("older-page bytes {new_bytes}; old full-page bytes {old_bytes}");
    assert!(new_bytes * 5 < old_bytes);
}
#[tokio::test]
async fn older_page_replaces_changed_fold_and_omits_unchanged_prose() {
    let fixture = Fixture::new();
    let calls = (0..4).map(|index| json!({"type":"assistant","uuid":format!("call-{index}"),"message":{"content":[{"type":"tool_use","id":format!("tool-{index}"),"name":"Read","input":{"file_path":"readme"}}]}}).to_string()+"\n").collect::<String>();
    let prose = json!({"type":"assistant","uuid":"prose","message":{"content":[{"type":"text","text":"done"}]}});
    std::fs::write(&fixture.log, calls + &format!("{prose}\n")).unwrap();
    let first = row_page(&fixture, 3, None).await.unwrap();
    let older = row_page(&fixture, 2, first["page"]["start_offset"].as_u64())
        .await
        .unwrap();
    assert_eq!(older["rows"].as_array().unwrap().len(), 1);
    assert_eq!(older["rows"][0]["steps"].as_array().unwrap().len(), 4);
    assert!(
        older["remove"]
            .as_array()
            .unwrap()
            .contains(&first["rows"][0]["id"])
    );
}

#[tokio::test]
async fn pages() {
    let fixture = Fixture::new();
    codex_log(&fixture, 245);
    let first = row_page(&fixture, 100, None).await.unwrap();
    assert_eq!(
        row_texts(&first),
        (145..245)
            .map(|number| format!("message-{number}"))
            .collect::<Vec<_>>()
    );
    let second = row_page(&fixture, 100, first["page"]["start_offset"].as_u64())
        .await
        .unwrap();
    assert_eq!(
        row_texts(&second),
        (45..145)
            .map(|number| format!("message-{number}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(second["rows"].as_array().unwrap().len(), 100);
    let all = row_page(&fixture, 100, second["page"]["start_offset"].as_u64())
        .await
        .unwrap();
    assert_eq!(
        row_texts(&all),
        (0..45)
            .map(|number| format!("message-{number}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(all["page"]["start_offset"], 0);
    let again = row_page(&fixture, 100, Some(0)).await.unwrap();
    assert_eq!(again["rows"], json!([]));
}
#[tokio::test]
async fn append_while_paging() {
    let fixture = Fixture::new();
    codex_log(&fixture, 200);
    let first = row_page(&fixture, 100, None).await.unwrap();
    fixture
        .app
        .hub
        .lock()
        .unwrap()
        .streams
        .insert("live".into(), client(&[]));
    let key = format!("{}:chair", fixture.session);
    let _ = subscribe(
        State(fixture.app.clone()),
        Json(Subscription {
            stream: "live".into(),
            keys: vec![SubscriptionKey {
                key: key.clone(),
                from: first["page"]["end_offset"].as_u64().unwrap(),
                view: Some("rows".into()),
            }],
        }),
    )
    .await
    .unwrap();
    let older = row_page(&fixture, 100, first["page"]["start_offset"].as_u64());
    let append = async {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&fixture.log)
            .unwrap()
            .write_all(codex_line(200).as_bytes())
            .unwrap();
    };
    let (page, ()) = tokio::join!(older, append);
    page.unwrap();
    let size = std::fs::metadata(&fixture.log).unwrap().len();
    until_cursor(&fixture.app, "live", &key, size).await;
    {
        let hub = fixture.app.hub.lock().unwrap();
        let state = hub.followers[&key].rows.as_ref().unwrap();
        let texts: Vec<_> = state
            .rows()
            .iter()
            .filter(|row| row.kind == view::RowKind::User)
            .map(|row| row.text.clone())
            .collect();
        assert_eq!(
            texts,
            (0..=200)
                .map(|number| format!("message-{number}"))
                .collect::<Vec<_>>()
        );
    }
    // A fresh tail request must preserve history already held by a live row key.
    assert_eq!(
        row_texts(&row_page(&fixture, 1, None).await.unwrap()).len(),
        201
    );
    let mut done = fixture.app.hub.lock().unwrap().followers[&key].done.clone();
    drop(Events {
        app: fixture.app.clone(),
        id: "live".into(),
    });
    done.wait_for(|finished| *finished).await.unwrap();
}
#[tokio::test]
async fn retry_history() {
    let fixture = Fixture::new();
    codex_log(&fixture, 150);
    let data = std::fs::read(&fixture.log).unwrap();
    let first = row_page(&fixture, 100, None).await.unwrap();
    let cursor = first["page"]["start_offset"].as_u64().unwrap();
    std::fs::remove_file(&fixture.log).unwrap();
    assert!(row_page(&fixture, 100, Some(cursor)).await.is_err());
    assert_eq!(fixture.app.hub.lock().unwrap().row_cache[0].1.start, cursor);
    std::fs::write(&fixture.log, data).unwrap();
    let all = row_page(&fixture, 100, Some(cursor)).await.unwrap();
    assert_eq!(
        row_texts(&all),
        (0..50)
            .map(|number| format!("message-{number}"))
            .collect::<Vec<_>>()
    );
}
#[tokio::test]
async fn empty() {
    let fixture = Fixture::new();
    std::fs::write(&fixture.log, "").unwrap();
    let started = std::time::Instant::now();
    let page = row_page(&fixture, 100, None).await.unwrap();
    assert_eq!(page["rows"], json!([]));
    assert_eq!(page["page"]["start_offset"], 0);
    assert_eq!(page["page"]["end_offset"], 0);
    assert!(started.elapsed() < Duration::from_secs(2));
}
#[tokio::test]
async fn multiple_events() {
    let fixture = Fixture::new();
    let content: Vec<_> = (0..120).map(|index| json!({"type":"tool_use","id":format!("call-{index}"),"name":"Read","input":{}})).collect();
    std::fs::write(
        &fixture.log,
        json!({"type":"assistant","sessionId":"test","uuid":"many","message":{"content":content}})
            .to_string()
            + "\n",
    )
    .unwrap();
    let page = row_page(&fixture, 1, None).await.unwrap();
    assert_eq!(page["page"]["start_offset"], 0);
    assert_eq!(page["rows"].as_array().unwrap().len(), 2);
    assert_eq!(page["rows"][0]["steps"].as_array().unwrap().len(), 100);
    assert_eq!(page["rows"][1]["steps"].as_array().unwrap().len(), 20);
}
#[tokio::test]
async fn boundary_tool() {
    let fixture = Fixture::new();
    codex_log(&fixture, 0);
    let old = "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"call_id\":\"boundary\",\"name\":\"shell\",\"arguments\":\"{\\\"command\\\":\\\"true\\\"}\"}}\n";
    let result = "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"boundary\",\"output\":\"done\"}}\n";
    std::fs::write(&fixture.log, old.to_owned() + result).unwrap();
    let first = row_page(&fixture, 1, None).await.unwrap();
    assert_eq!(first["rows"][0]["kind"], "toolResult");
    let orphan_id = first["rows"][0]["id"].clone();
    let all = row_page(&fixture, 1, first["page"]["start_offset"].as_u64())
        .await
        .unwrap();
    assert_eq!(all["rows"].as_array().unwrap().len(), 1);
    assert_eq!(all["rows"][0]["tool"]["output"]["text"], "done");
    assert_eq!(all["rows"][0]["tool"]["state"], "finished");
    assert_eq!(all["remove"], json!([orphan_id]));
    let Json(output) = tool_output(
        State(fixture.app.clone()),
        Query(ToolOutputQuery {
            session: fixture.session.clone(),
            agent: "chair".into(),
            row: all["rows"][0]["id"].as_str().unwrap().into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(output["text"], "done");
}
#[tokio::test]
async fn giant_output_stays_small_and_full_output_is_exact() {
    let fixture = Fixture::new();
    let full = (0..100000)
        .map(|index| format!("line {index}\n"))
        .collect::<String>();
    let call = json!({"type":"assistant","uuid":"call","message":{"content":[{"type":"tool_use","id":"giant","name":"Bash","input":{"command":"print"}}]}});
    let result = json!({"type":"user","uuid":"result","message":{"content":[{"type":"tool_result","tool_use_id":"giant","content":full}]}});
    std::fs::write(&fixture.log, format!("{call}\n{result}\n")).unwrap();
    let page = row_page(&fixture, 200, None).await.unwrap();
    assert!(page.to_string().len() < 20000);
    assert_eq!(page["rows"][0]["tool"]["output"]["full_lines"], 100000);
    assert_eq!(page["rows"][0]["tool"]["output"]["full_bytes"], full.len());
    let Json(output) = tool_output(
        State(fixture.app.clone()),
        Query(ToolOutputQuery {
            session: fixture.session.clone(),
            agent: "chair".into(),
            row: page["rows"][0]["tool"]["output"]["id"]
                .as_str()
                .unwrap()
                .into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(output["text"], full);
    assert_eq!(
        tool_output(
            State(fixture.app.clone()),
            Query(ToolOutputQuery {
                session: fixture.session.clone(),
                agent: "chair".into(),
                row: "raw-1-0:output".into()
            })
        )
        .await
        .unwrap_err()
        .0,
        StatusCode::NOT_FOUND
    );
}
