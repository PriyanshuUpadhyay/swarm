//! G1 uses the public saved caller with protocol 2; expected data is ORIGINAL or marked HAND.
#![cfg(feature = "serve")]
#[path = "../src/serve/transcript_caller.rs"]
mod caller;
#[path = "s9c/frozen.rs"]
pub mod frozen;
use caller::{Fault, Source};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use swarm::view::transcript::worker_protocol::{Binding, v2::Operation};
use tokio::process::Command;
const WORK: Duration = Duration::from_millis(900);
struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("s9c-saved-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("OWNER.json"), "{\"owner\":\"orchestrator\"}\n").unwrap();
        std::fs::create_dir(path.join("home")).unwrap();
        Self(path)
    }
    fn path(&self, case: &str, brain: bool) -> PathBuf {
        let path = if brain {
            self.0.join(case).join("brain/agy-s9c/steps.jsonl")
        } else {
            self.0.join(case).join("plain.jsonl")
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        path
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
async fn open(tree: &Tree, path: &Path, binding: &str) -> Source {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", tree.0.join("home"))
        .env("TMPDIR", &tree.0)
        .current_dir(&tree.0)
        .args(["_transcript-worker", "--protocol", "2"]);
    Source::open(
        command,
        Binding {
            source_id: binding.into(),
            generation: 1,
        },
        "agy",
        path,
        WORK,
    )
    .await
    .expect("AGY Open must succeed")
}
fn core(mut body: Value) -> Value {
    // These route-only maps are outside Source.page; G2/G3 keep all four in their whole objects.
    for key in ["display", "patch", "find", "images"] {
        body.as_object_mut().unwrap().remove(key);
    }
    body
}
fn page(old: &Value, case: &str, name: &str) -> Value {
    core(old[format!("{case}/{name}")]["body"].clone())
}
fn native(lines: &Value, case: &str, binding: &str, start: u64, end: u64) -> Value {
    let selected: Vec<_> = lines[case]
        .as_array()
        .unwrap()
        .iter()
        .filter(|line| {
            line["start_byte"].as_u64().unwrap() >= start
                && line["end_byte"].as_u64().unwrap() <= end
        })
        .cloned()
        .collect();
    // HAND envelope and physical byte offsets join literal OLD per-line events; no mapper runs here.
    json!({"source_id":binding,"read_end_byte":end,"complete_end_byte":end,"lines":selected})
}
fn raw(lines: &Value, case: &str, start: u64, end: u64) -> Value {
    let selected:Vec<_>=lines[case].as_array().unwrap().iter().filter(|line|line["start_byte"].as_u64().unwrap()>=start&&line["end_byte"].as_u64().unwrap()<=end).map(|line|json!({"start_byte":line["start_byte"],"end_byte":line["end_byte"],"events":line["events"]})).collect();
    // HAND protocol-2 saved payload shape joins literal OLD events and physical offsets.
    json!({"start_byte":start,"end_byte":end,"lines":selected})
}
#[tokio::test]
async fn g1_agy_saved_ranges_keep_old_values_and_the_original_held_path() {
    let tree = Tree::new();
    let sealed = frozen::expected(&["routes.json", "native-lines.json"]);
    let old = &sealed["routes.json"];
    let lines = &sealed["native-lines.json"];
    let (mut actual, mut expected) = (serde_json::Map::new(), serde_json::Map::new());
    for case in [
        "agy-chair-run",
        "agy-edit",
        "agy-main",
        "agy-prefix-1104",
        "agy-prefix-1358",
        "agy-guards",
        "agy-numbers",
    ] {
        let path = tree.path(case, true);
        std::fs::write(&path, frozen::bytes(&format!("inputs/{case}.jsonl"))).unwrap();
        let source = open(&tree, &path, "opaque-saved-binding").await;
        std::fs::rename(&path, path.with_extension("held")).unwrap();
        std::fs::write(&path, b"replacement, not the held log\n").unwrap();
        if old[format!("{case}/rows-full.txt")]["status"] != 200 {
            // HAND boundary translation of the captured OLD HTTP failure, not an empty Row page.
            let value = match source.page(2000, None, WORK).await {
                Err(Fault::Core) => json!({"fault":"core"}),
                other => json!({"unexpected":format!("{other:?}")}),
            };
            actual.insert(case.into(), value);
            expected.insert(case.into(), json!({"fault":"core"}));
        } else {
            let mut requests = vec![("full".to_owned(), 2000, None)];
            for tail in [1, 3, 4, 8, 11, 16] {
                requests.push((format!("tail-{tail}"), tail, None));
            }
            let mut before = None;
            for step in 0..40 {
                let label = format!("back-{step:02}");
                let row_page = page(old, case, &format!("rows-{label}.txt"));
                requests.push((label, 2, before));
                match row_page["page"]["start_offset"].as_u64().unwrap() {
                    0 => break,
                    offset => before = Some(offset),
                }
            }
            for (label, tail, before) in requests {
                let key = format!("{case}/{label}");
                let wanted = page(old, case, &format!("rows-{label}.txt"));
                let start = wanted["page"]["start_offset"].as_u64().unwrap();
                let end = wanted["page"]["end_offset"].as_u64().unwrap();
                actual.insert(key.clone(),json!({"rows":source.page(tail,before,WORK).await.unwrap(),"native":source.native(Operation::Raw{tail,before},WORK).await.unwrap()}));
                expected.insert(
                    key,
                    json!({"rows":wanted,"native":raw(lines,case,start,end)}),
                );
            }
        }
        source.close(WORK).await.unwrap();
    }
    let cold_path = tree.path("cold-result-only", true);
    std::fs::write(&cold_path, frozen::bytes("inputs/agy-main.jsonl")).unwrap();
    let cold = open(&tree, &cold_path, "opaque-cold-binding").await;
    actual.insert(
        "cold-result-only".into(),
        cold.page(2, Some(1358), WORK).await.unwrap(),
    );
    expected.insert(
        "cold-result-only".into(),
        page(old, "cold-result-only", "rows-page.txt"),
    );
    cold.close(WORK).await.unwrap();
    let path = tree.path("no-brain", false);
    std::fs::write(&path, frozen::bytes("inputs/agy-main.jsonl")).unwrap();
    let source = open(&tree, &path, "opaque-without-path-session").await;
    actual.insert(
        "no-brain".into(),
        source
            .native(
                Operation::Raw {
                    tail: 2000,
                    before: None,
                },
                WORK,
            )
            .await
            .unwrap(),
    );
    expected.insert("no-brain".into(), raw(lines, "agy-no-brain", 0, 3476));
    source.close(WORK).await.unwrap();
    assert_eq!(Value::Object(actual), Value::Object(expected));
}
#[tokio::test]
async fn g1_agy_saved_replay_preserves_two_live_readers_and_pending_calls() {
    let tree = Tree::new();
    let sealed = frozen::expected(&["routes.json", "native-lines.json"]);
    let old = &sealed["routes.json"];
    let lines = &sealed["native-lines.json"];
    let bytes = frozen::bytes("inputs/agy-main.jsonl");
    let path = tree.path("follow", true);
    std::fs::write(&path, &bytes[..1104]).unwrap();
    let a = open(&tree, &path, "opaque-reader-a").await;
    let b = open(&tree, &path, "opaque-reader-b").await;
    let initial = a
        .native(Operation::Follow { from: Some(0) }, WORK)
        .await
        .unwrap();
    let other_initial = b
        .native(Operation::Follow { from: Some(1104) }, WORK)
        .await
        .unwrap();
    let prefix = a.page(2000, None, WORK).await.unwrap();
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(&bytes[1104..1358]).unwrap();
    let after = a
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let other_after = b
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let after_page = a.page(2000, None, WORK).await.unwrap();
    let replay = a.page(2000, Some(1104), WORK).await.unwrap();
    let rejected = match a
        .native(
            Operation::Window {
                start: 1,
                end: 1104,
            },
            WORK,
        )
        .await
    {
        Err(Fault::Core) => "core",
        _ => "unexpected",
    };
    file.write_all(&bytes[1358..]).unwrap();
    let final_a = a
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let final_b = b
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let final_page = a.page(2000, None, WORK).await.unwrap();
    let quiet = a
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let mut job = a.start_page(2000, None, WORK).await.unwrap();
    job.cancel(WORK).await.unwrap();
    let cancelled = match job.wait().await {
        Err(Fault::Cancelled) => "cancelled",
        _ => "unexpected",
    };
    let after_cancel = a
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    a.close(WORK).await.unwrap();
    b.close(WORK).await.unwrap();
    assert_eq!(
        json!({"initial":initial,"other_initial":other_initial,"prefix":prefix,"after":after,"other_after":other_after,"after_page":after_page,"replay":replay,"rejected":rejected,"final_a":final_a,"final_b":final_b,"final_page":final_page,"quiet":quiet,"cancelled":cancelled,"after_cancel":after_cancel}),
        json!({"initial":native(lines,"agy-main","opaque-reader-a",0,1104),"other_initial":native(lines,"agy-main","opaque-reader-b",1104,1104),"prefix":page(old,"follow-main","rows-prefix.txt"),"after":native(lines,"agy-main","opaque-reader-a",1104,1358),"other_after":native(lines,"agy-main","opaque-reader-b",1104,1358),"after_page":page(old,"follow-main","rows-after-results.txt"),"replay":page(old,"follow-main","rows-replay.txt"),"rejected":"core","final_a":native(lines,"agy-main","opaque-reader-a",1358,3476),"final_b":native(lines,"agy-main","opaque-reader-b",1358,3476),"final_page":page(old,"follow-main","rows-final.txt"),"quiet":native(lines,"agy-main","opaque-reader-a",3476,3476),"cancelled":"cancelled","after_cancel":native(lines,"agy-main","opaque-reader-a",3476,3476)})
    );
}
