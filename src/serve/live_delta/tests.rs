//! Tests for server live delta.

use super::*;
use crate::serve::test_metrics::*;
use crate::serve::test_support::*;
#[cfg(target_os = "macos")]
#[test]
#[ignore = "release performance check, 5 keys x 50 records/s x 600 simulated seconds"]
fn live_rows_closed_turn_soak() {
    live_soak(5, false);
}
#[cfg(target_os = "macos")]
#[test]
#[ignore = "release performance check, 1 open turn x 50 tools/s x 600 simulated seconds"]
fn live_rows_open_turn_soak() {
    live_soak(1, true);
}
#[test]
#[ignore = "release baseline for the previous full-history rebuild"]
fn live_rows_full_rebuild_baseline() {
    for count in [1000, 30000] {
        let mut state = LiveRows::empty(0, false);
        for index in 0..count {
            state.ingest(
                index as u64,
                index as u64 + 1,
                &[synthetic_event(index, false)],
            );
        }
        let mut samples = Vec::new();
        for _ in 0..20 {
            let timer = std::time::Instant::now();
            let frame = reference_delta(&mut state, "chat");
            std::hint::black_box(frame.to_string());
            samples.push(timer.elapsed().as_nanos());
        }
        println!(
            "full_rebuild records={count} p95_ns={}",
            percentile(&mut samples)
        );
    }
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "release cadence check during a 30000-tool turn-end rebuild"]
async fn large_turn_end_keeps_other_keys_running() {
    let fixture = Fixture::new();
    std::fs::write(&fixture.log, vec![b'\n'; 30_001]).unwrap();
    let source = std::fs::File::open(&fixture.log).unwrap();
    let heavy_source = source.try_clone().unwrap();
    let mut state = LiveRows::empty(0, false);
    state.ingest(
        0,
        30_000,
        &(0..30_000)
            .map(|number| synthetic_event(number, true))
            .collect::<Vec<_>>(),
    );
    state.delta("large");
    {
        let mut hub = fixture.app.hub.lock().unwrap();
        let mut stream = client(&[("large", 30_000), ("small", 0)]);
        stream.row_keys = HashSet::from(["large".into(), "small".into()]);
        hub.streams.insert("cadence".into(), stream);
        for (key, rows, from) in [
            ("large", state, 30_000),
            ("small", LiveRows::empty(0, false), 0),
        ] {
            let (stop, _) = watch::channel(false);
            let (_, done) = watch::channel(false);
            hub.followers.insert(
                key.into(),
                Follower {
                    generation: "cadence".into(),
                    from,
                    stop,
                    done,
                    rows: Some(rows),
                    rendering_rows: false,
                },
            );
        }
    }
    let weak = Arc::downgrade(&fixture.app);
    let heavy = tokio::spawn(async move {
        let mut batches = vec![(
            30_001,
            vec![json!({"type":"turn_ended","reason":"completed"})],
        )];
        flush_transcript(&weak, "large", "cadence", &mut batches, &heavy_source)
            .await
            .unwrap();
    });
    let mut ticks = Vec::new();
    let mut previous = std::time::Instant::now();
    for index in 0..15 {
        tokio::time::sleep(Duration::from_millis(16)).await;
        ticks.push(previous.elapsed().as_nanos());
        previous = std::time::Instant::now();
        let mut batches = vec![(index + 1, vec![synthetic_event(index as usize, false)])];
        flush_transcript(
            &Arc::downgrade(&fixture.app),
            "small",
            "cadence",
            &mut batches,
            &source,
        )
        .await
        .unwrap();
    }
    heavy.await.unwrap();
    println!(
        "other_key_cadence_p95_ms={} worst_ms={}",
        percentile(&mut ticks) as f64 / 1_000_000.,
        *ticks.iter().max().unwrap() as f64 / 1_000_000.
    );
    assert!(
        percentile(&mut ticks) < 40_000_000,
        "another key stalled during turn end"
    );
    assert_eq!(
        fixture.app.hub.lock().unwrap().streams["cadence"].keys["small"],
        15
    );
}
#[tokio::test]
#[ignore = "release HTTP-path costs on a 100000-line synthetic chat"]
async fn row_http_paths_on_large_synthetic_chat() {
    let fixture = Fixture::new();
    let full = (0..100_000)
        .map(|index| format!("line {index}\n"))
        .collect::<String>();
    let call = json!({"type":"assistant","uuid":"giant","message":{"content":[{"type":"tool_use","id":"giant","name":"Bash","input":{"command":"print"}}]}});
    let result = json!({"type":"user","uuid":"giant-result","message":{"content":[{"type":"tool_result","tool_use_id":"giant","content":full}]}});
    let beginning = format!("{call}\n{result}\n");
    let tail=(0..100_000).map(|index|json!({"type":"user","uuid":format!("u-{index}"),"message":{"content":format!("message-{index}")}}).to_string()+"\n").collect::<String>();
    std::fs::write(&fixture.log, &beginning).unwrap();
    let giant = row_page(&fixture, 2, None).await.unwrap();
    std::fs::write(&fixture.log, beginning + &tail).unwrap();
    let direct = giant["rows"][0]["tool"]["output"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let legacy = giant["rows"][0]["id"].as_str().unwrap().to_owned();
    let mut cold = Vec::new();
    for _ in 0..20 {
        fixture.app.hub.lock().unwrap().row_cache.clear();
        let timer = std::time::Instant::now();
        row_page(&fixture, 200, None).await.unwrap();
        cold.push(timer.elapsed().as_nanos());
    }
    let mut warm = Vec::new();
    let mut older = Vec::new();
    for _ in 0..20 {
        let timer = std::time::Instant::now();
        let page = row_page(&fixture, 200, None).await.unwrap();
        warm.push(timer.elapsed().as_nanos());
        let timer = std::time::Instant::now();
        row_page(&fixture, 200, page["page"]["start_offset"].as_u64())
            .await
            .unwrap();
        older.push(timer.elapsed().as_nanos());
    }
    let mut output = Vec::new();
    let mut legacy_output = Vec::new();
    for (id, times) in [(&direct, &mut output), (&legacy, &mut legacy_output)] {
        for _ in 0..20 {
            let timer = std::time::Instant::now();
            let Json(value) = tool_output(
                State(fixture.app.clone()),
                Query(ToolOutputQuery {
                    session: fixture.session.clone(),
                    agent: "chair".into(),
                    row: id.clone(),
                }),
            )
            .await
            .unwrap();
            assert_eq!(value["text"], full);
            times.push(timer.elapsed().as_nanos());
        }
    }
    println!(
        "http_100k cold_p95_ms={} warm_p95_ms={} older_p95_ms={} giant_direct_p95_ms={} giant_row_id_p95_ms={}",
        percentile(&mut cold) as f64 / 1_000_000.,
        percentile(&mut warm) as f64 / 1_000_000.,
        percentile(&mut older) as f64 / 1_000_000.,
        percentile(&mut output) as f64 / 1_000_000.,
        percentile(&mut legacy_output) as f64 / 1_000_000.
    );
    assert!(percentile(&mut cold) < 150_000_000);
    assert!(percentile(&mut warm) <= 100_000_000);
    assert!(percentile(&mut output) <= 150_000_000);
    assert!(percentile(&mut legacy_output) <= 150_000_000);
}
