//! The private reader operations of `swarm _transcript-worker --protocol 1` (second migration unit,
//! Claude only): two readers over one held file.
//!
//! Every expected value comes from the ORIGINAL c8ca837e translator and row layer, captured before
//! this unit existed (see `tests/fixtures/transcript-readers/PROVENANCE.json`). Requests are literal
//! JSON, so this test compiles without any new production API and fails until the operations exist.

use std::fs::OpenOptions;
use std::io::Write;

use serde_json::{Value, json};

#[path = "transcript_readers/client.rs"]
mod client;
#[path = "transcript_readers/fixtures.rs"]
mod fixtures;

use client::Worker;
use fixtures::Scenario;

fn reader(binding: &Value) -> Value {
    json!({"reader_id": binding[0], "generation": binding[1]})
}

#[test]
fn two_readers_share_one_held_file_and_match_the_old_translator_and_rows() {
    let scenario = Scenario::load();
    let mut worker = Worker::start();
    let work = worker.scratch.join("work");
    std::fs::create_dir_all(&work).expect("work dir");
    let path = work.join("chair.jsonl");
    std::fs::write(&path, &scenario.source[..792]).expect("initial bytes");
    let ready = worker.hello();
    assert_eq!(
        ready,
        json!({"op": "ready", "protocol": 1, "formats": ["claude"]})
    );
    let opened = worker.open(&scenario.binding, path.to_str().expect("utf-8 path"));
    assert_eq!(opened["op"], "opened", "open: {opened}");
    // The handle of the original file. After the decoy replaces the path, appends go here.
    let mut held = None;
    for step in &scenario.steps {
        let name = step["name"].as_str().expect("step name");
        if step["decoy_replace"] == true {
            held = Some(
                OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .expect("held handle"),
            );
            let decoy = work.join("decoy.jsonl");
            let mut bytes = scenario.source[..1050].to_vec();
            bytes.extend_from_slice(scenario.decoy_line.as_bytes());
            std::fs::write(&decoy, bytes).expect("decoy");
            std::fs::rename(&decoy, &path).expect("replace the path with the decoy");
        }
        if let Some(range) = step["append"].as_array() {
            let (from, to) = (
                range[0].as_u64().unwrap() as usize,
                range[1].as_u64().unwrap() as usize,
            );
            let bytes = &scenario.source[from..to];
            match held.as_mut() {
                Some(file) => file.write_all(bytes).expect("append to the held inode"),
                None => OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .expect("open for append")
                    .write_all(bytes)
                    .expect("append"),
            }
        }
        let op = step["op"].as_str().expect("op");
        let mut fields = json!({});
        if !step["reader"].is_null() {
            fields["reader"] = reader(&step["reader"]);
        }
        for key in ["from_byte", "start_byte", "end_byte"] {
            if !step[key].is_null() {
                fields[key] = step[key].clone();
            }
        }
        let updates = worker
            .reader_op(op, &scenario.binding, fields)
            .unwrap_or_else(|fault| {
                panic!("{name}: the worker answered {op} with a fault: {fault}")
            });
        assert_eq!(updates, scenario.expected_updates(step), "{name}: updates");
    }
    worker.close_source(&scenario.binding);
}

#[test]
fn a_reader_stale_after_a_higher_rebind_commits_nothing_and_ends_the_worker() {
    let scenario = Scenario::load();
    let mut worker = Worker::start();
    let work = worker.scratch.join("work");
    std::fs::create_dir_all(&work).expect("work dir");
    let path = work.join("chair.jsonl");
    std::fs::write(&path, &scenario.source[..1050]).expect("bytes");
    assert_eq!(worker.hello()["op"], "ready");
    let opened = worker.open(&scenario.binding, path.to_str().expect("utf-8 path"));
    assert_eq!(opened["op"], "opened", "open: {opened}");
    for (generation, from) in [(0, 0), (1, 485)] {
        worker
            .reader_op(
                "bind_reader",
                &scenario.binding,
                json!({"reader": {"reader_id": 1, "generation": generation}, "from_byte": from}),
            )
            .unwrap_or_else(|fault| panic!("bind generation {generation}: fault {fault}"));
    }
    // Generation 0 is stale now. The reply must be a fault with no begin frame before it.
    let stale = worker.reader_op(
        "replay_reader",
        &scenario.binding,
        json!({"reader": {"reader_id": 1, "generation": 0}, "start_byte": 0, "end_byte": 245}),
    );
    let fault = stale.expect_err("a stale reader generation is refused");
    assert_eq!(fault["op"], "fault");
    assert_eq!(fault["code"], "binding_mismatch");
    assert!(worker.frame().is_none(), "nothing follows a fault");
    assert_ne!(worker.exit_code(), 0, "a fault exits nonzero");
}
