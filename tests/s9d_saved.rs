//! G1 checks the real protocol 2 caller against ORIGINAL Pi ranges and events.
#![cfg(feature = "serve")]
#[path = "../src/serve/transcript_caller.rs"]
mod caller;
#[path = "s9d/frozen.rs"]
mod frozen;
use caller::{Fault, Source};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf, time::Duration};
use swarm::view::transcript::worker_protocol::{Binding, v2::Operation};
use tokio::process::Command;
const WORK: Duration = Duration::from_secs(10);
const MAIN: &str = "2026-11-05T00-00-00-000Z_pi-u04-main.jsonl";
const CAPTURE_SESSION: &str = "00000000-0000-4000-8000-000000000000";
fn capture_session(value: Value) -> Value {
    // ORIGINAL replaces its 36-byte session id after computing output byte counts.
    serde_json::from_str(&value.to_string().replace(CAPTURE_SESSION, "SESSION")).unwrap()
}
struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("s9d-saved-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("OWNER.json"), "{\"owner\":\"orchestrator\"}\n").unwrap();
        Self(p)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture(name: &str) -> Vec<u8> {
    frozen::bytes(&format!("inputs/{name}"))
}
async fn open(tree: &Tree, file: &str, bytes: &[u8], binding: &str) -> (Source, PathBuf) {
    let path = tree.0.join(file);
    std::fs::write(&path, bytes).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_swarm"));
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &tree.0)
        .env("TMPDIR", &tree.0)
        .args(["_transcript-worker", "--protocol", "2"]);
    let source = Source::open(
        cmd,
        Binding {
            source_id: binding.into(),
            generation: 1,
        },
        "pi",
        &path,
        WORK,
    )
    .await
    .unwrap_or_else(|fault| panic!("Pi Open must accept pi: {fault:?}"));
    (source, path)
}
fn old(case: &str, name: &str) -> Value {
    let bytes = frozen::bytes(&format!("out-1/native/{case}/{name}"));
    frozen::parse(name, &String::from_utf8(bytes).unwrap())
}
fn core(mut v: Value) -> Value {
    // These fields belong to the HTTP paint adapter, not the private worker page.
    for key in ["display", "patch", "find", "images"] {
        v.as_object_mut().unwrap().remove(key);
    }
    v
}
fn native(file: &str, start: u64, end: u64, follow: bool) -> Value {
    let data = frozen::bytes(&format!("native-1/{file}.lines.json"));
    let all: Value = serde_json::from_slice(&data).unwrap();
    let lines: Vec<Value> = all
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            v["start_byte"].as_u64().unwrap() >= start && v["end_byte"].as_u64().unwrap() <= end
        })
        .map(|v| {
            let mut v = v.clone();
            if follow {
                v["terminated"] = json!(true);
            }
            v
        })
        .collect();
    if follow {
        json!({"source_id":"pi-live-binding","read_end_byte":end,"complete_end_byte":end,"lines":lines})
    } else {
        json!({"start_byte":start,"end_byte":end,"lines":lines})
    }
}
async fn page(source: &Source, case: &str, name: &str, tail: u16, before: Option<u64>) {
    let expected = old(case, name);
    let actual = source.page(tail, before, WORK).await;
    if expected["status"] == 200 {
        assert_eq!(
            actual.unwrap(),
            core(expected["body"].clone()),
            "{case}/{name}"
        );
    } else {
        assert_eq!(actual.err(), Some(Fault::Core), "{case}/{name}: OLD failed");
    }
}
async fn raw(source: &Source, case: &str, name: &str, file: &str, tail: u16, before: Option<u64>) {
    let expected = old(case, name);
    let actual = source.native(Operation::Raw { tail, before }, WORK).await;
    if expected["status"] == 200 {
        let p = &expected["body"]["page"];
        assert_eq!(
            actual.unwrap(),
            native(
                file,
                p["start_offset"].as_u64().unwrap(),
                p["end_offset"].as_u64().unwrap(),
                false
            ),
            "{case}/{name}"
        );
    } else {
        assert_eq!(actual.err(), Some(Fault::Core), "{case}/{name}: OLD failed");
    }
}
#[tokio::test]
async fn g1_pi_saved_ranges_match_original() {
    let tree = Tree::new();
    let cases: Value = serde_json::from_str(include_str!("fixtures/s9d/cases.json")).unwrap();
    for c in cases.as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let file = c["file"].as_str().unwrap();
        let (source, _) = open(
            &tree,
            file,
            &fixture(c["input"].as_str().unwrap_or(file)),
            name,
        )
        .await;
        page(&source, name, "rows-full.txt", 2000, None).await;
        page(&source, name, "rows-cached.txt", 2000, None).await;
        raw(&source, name, "raw-full.txt", file, 2000, None).await;
        if c["deep"] == true {
            for tail in [1, 3, 8] {
                page(&source, name, &format!("rows-tail-{tail}.txt"), tail, None).await;
                raw(
                    &source,
                    name,
                    &format!("raw-tail-{tail}.txt"),
                    file,
                    tail,
                    None,
                )
                .await;
            }
            let mut before = None;
            for i in 0..40 {
                let name2 = format!("rows-back-{i:02}.txt");
                page(&source, name, &name2, 2, before).await;
                raw(
                    &source,
                    name,
                    &format!("raw-back-{i:02}.txt"),
                    file,
                    2,
                    before,
                )
                .await;
                match old(name, &name2)["body"]["page"]["start_offset"].as_u64() {
                    Some(0) | None => break,
                    Some(n) => before = Some(n),
                }
            }
            let full = old(name, "raw-full.txt");
            let end = full["body"]["page"]["end_offset"].as_u64().unwrap();
            assert_eq!(
                source
                    .native(Operation::Window { start: 0, end }, WORK)
                    .await
                    .unwrap(),
                native(file, 0, end, false)
            );
            let lines = native(file, 0, end, false);
            // Line is the public full-output lookup's native primitive.
            for line in lines["lines"].as_array().unwrap() {
                let offset = line["start_byte"].as_u64().unwrap();
                let end = line["end_byte"].as_u64().unwrap();
                assert_eq!(
                    source
                        .native(Operation::Line { offset }, WORK)
                        .await
                        .unwrap(),
                    native(file, offset, end, false)
                );
            }
        }
        source.close(WORK).await.unwrap();
    }
    for mode in ["native", "override", "direct"] {
        for agent in ["chair", "worker"] {
            for stored in [false, true] {
                let label = if stored { "stored" } else { "source" };
                let file = if stored {
                    MAIN
                } else if agent == "chair" {
                    "time_pi-fallback.jsonl"
                } else {
                    "time_00000000-0000-4000-8000-000000000000-worker.jsonl"
                };
                let original_file = if !stored && agent == "worker" {
                    "time_SESSION-worker.jsonl"
                } else {
                    file
                };
                let (source, _) = open(&tree, file, &fixture(MAIN), "source-choice-binding").await;
                let values = frozen::expected(&[&format!("{mode}/{label}-{agent}/")]);
                for (key, v) in values.as_object().unwrap() {
                    if key.ends_with("rows-full.txt") {
                        assert_eq!(
                            capture_session(source.page(2000, None, WORK).await.unwrap()),
                            core(v["body"].clone()),
                            "{key}"
                        );
                    } else {
                        assert_eq!(
                            capture_session(
                                source
                                    .native(
                                        Operation::Raw {
                                            tail: 2000,
                                            before: None
                                        },
                                        WORK
                                    )
                                    .await
                                    .unwrap()
                            ),
                            native(original_file, 0, 4179, false),
                            "{key}"
                        );
                    }
                }
                source.close(WORK).await.unwrap();
            }
        }
    }
    for (case, warm) in [("output-cold", false), ("output-retained", true)] {
        let (s, _) = open(&tree, MAIN, &fixture(MAIN)[..1687], case).await;
        if warm {
            page(&s, case, "rows-full.txt", 2000, None).await;
        }
        page(&s, case, "rows-window.txt", 2, Some(1687)).await;
        raw(&s, case, "raw-window.txt", MAIN, 2, Some(1687)).await;
        s.close(WORK).await.unwrap();
    }
}
#[tokio::test]
async fn g1_pi_follow_and_saved_replay_keep_reader_state() {
    let tree = Tree::new();
    let bytes = fixture(MAIN);
    let (s, path) = open(&tree, MAIN, &bytes[..1265], "pi-live-binding").await;
    let got = s
        .native(Operation::Follow { from: Some(0) }, WORK)
        .await
        .unwrap();
    assert_eq!(got, native(MAIN, 0, 1265, true));
    page(&s, "follow", "page-prefix.txt", 2000, None).await;
    page(&s, "follow", "page-older.txt", 2, Some(1265)).await;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&bytes[1265..1400])
        .unwrap();
    let mut partial = native(MAIN, 1265, 1265, true);
    partial["read_end_byte"] = json!(1400);
    assert_eq!(
        s.native(Operation::Follow { from: None }, WORK)
            .await
            .unwrap(),
        partial
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&bytes[1400..1687])
        .unwrap();
    assert_eq!(
        s.native(Operation::Follow { from: None }, WORK)
            .await
            .unwrap(),
        native(MAIN, 1265, 1687, true)
    );
    let (second, _) = open(&tree, MAIN, &bytes[..1687], "pi-live-binding").await;
    assert_eq!(
        second
            .native(Operation::Follow { from: Some(1265) }, WORK)
            .await
            .unwrap(),
        native(MAIN, 1265, 1687, true)
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&bytes[1687..])
        .unwrap();
    assert_eq!(
        s.native(Operation::Follow { from: None }, WORK)
            .await
            .unwrap(),
        native(MAIN, 1687, 4179, true)
    );
    assert_eq!(
        second
            .native(Operation::Follow { from: None }, WORK)
            .await
            .unwrap(),
        native(MAIN, 1687, 4179, true)
    );
    assert_eq!(
        s.native(Operation::Follow { from: None }, WORK)
            .await
            .unwrap(),
        native(MAIN, 4179, 4179, true)
    );
    let mut job = s.start_page(2000, None, WORK).await.unwrap();
    job.cancel(WORK).await.unwrap();
    assert_eq!(job.wait().await.err(), Some(Fault::Cancelled));
    assert_eq!(
        s.native(Operation::Follow { from: None }, WORK)
            .await
            .unwrap(),
        native(MAIN, 4179, 4179, true)
    );
    s.close(WORK).await.unwrap();
    second.close(WORK).await.unwrap();
}
