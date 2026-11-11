//! G1 uses the S9a2 candidate-01 caller and protocol 2. Expected values are OLD or HAND.
#![cfg(feature = "serve")]
#[path = "../src/serve/transcript_caller.rs"]
mod caller;
#[path = "s9b/frozen.rs"]
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
// Cold process startup has a separate bound from protocol work.
const STARTUP: Duration = Duration::from_secs(10);
struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("s9b-saved-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("OWNER.json"), "{\"owner\":\"orchestrator\"}\n").unwrap();
        std::fs::create_dir(path.join("home")).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn real(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("TMPDIR", root)
        .current_dir(root)
        .args(["_transcript-worker", "--protocol", "2"]);
    command
}
async fn open(root: &Path, path: &Path, name: &str) -> Source {
    Source::open(
        real(root),
        Binding {
            source_id: name.into(),
            generation: 1,
        },
        "codex",
        path,
        STARTUP,
    )
    .await
    .unwrap()
}
fn core(mut page: Value) -> Value {
    for key in ["display", "patch", "find"] {
        page.as_object_mut().unwrap().remove(key);
    }
    page
}
fn old(case: &str, name: &str) -> Value {
    let text = String::from_utf8(frozen::bytes(&format!("out-1/{case}/{name}"))).unwrap();
    core(frozen::parse(name, &text)["body"].clone())
}
// These line maps are the immutable c8ca837e ORIGINAL captures, named in their PROVENANCE.json.
fn native(source_id: &str, start: u64, end: u64) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/transcript_codex/fixtures");
    let mut lines = Vec::new();
    for name in ["lines-kernel-o0-a.json", "lines-kernel-o0-b.json"] {
        let value: Value =
            serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
        lines.extend(
            value
                .as_object()
                .unwrap()
                .values()
                .filter(|line| {
                    line["start_byte"].as_u64().unwrap() >= start
                        && line["end_byte"].as_u64().unwrap() <= end
                })
                .cloned(),
        );
    }
    lines.sort_by_key(|line| line["start_byte"].as_u64().unwrap());
    json!({"source_id":source_id,"read_end_byte":end,"complete_end_byte":end,"lines":lines})
}
#[tokio::test]
async fn g1_codex_saved_ranges_keep_the_old_values_and_the_held_file() {
    let tree = Tree::new();
    let (mut actual, mut expected) = (serde_json::Map::new(), serde_json::Map::new());
    for case in [
        "codex-chair-run",
        "codex-exec-edit",
        "codex-kernel",
        "codex-yield",
    ] {
        let path = tree.path(case);
        std::fs::write(&path, frozen::bytes(&format!("inputs/{case}.jsonl"))).unwrap();
        let source = open(&tree.0, &path, case).await;
        std::fs::rename(&path, tree.path(&format!("{case}-held"))).unwrap();
        std::fs::write(&path, b"replacement, not the open log\n").unwrap();
        let mut requests = vec![("rows-full.txt".to_owned(), 2000, None)];
        for tail in [1, 3, 4, 8, 11] {
            requests.push((format!("rows-tail-{tail}.txt"), tail, None));
        }
        let mut before = None;
        for step in 0..40 {
            let name = format!("rows-back-{step:02}.txt");
            let page = old(case, &name);
            requests.push((name, 2, before));
            match page["page"]["start_offset"].as_u64().unwrap() {
                0 => break,
                offset => before = Some(offset),
            }
        }
        for (name, tail, before) in requests {
            let key = format!("{case}/{name}");
            actual.insert(key.clone(), source.page(tail, before, WORK).await.unwrap());
            expected.insert(key, old(case, &name));
        }
        source.close(WORK).await.unwrap();
    }
    assert_eq!(Value::Object(actual), Value::Object(expected));
}
#[tokio::test]
async fn g1_saved_replay_and_rejected_work_keep_pending_live_questions() {
    let tree = Tree::new();
    let bytes = frozen::bytes("inputs/codex-kernel.jsonl");
    let path = tree.path("codex.jsonl");
    std::fs::write(&path, &bytes[..1518]).unwrap();
    let source = open(&tree.0, &path, "pending").await;
    let other = open(&tree.0, &path, "other").await;
    let initial = source
        .native(Operation::Follow { from: Some(0) }, WORK)
        .await
        .unwrap();
    let other_initial = other
        .native(Operation::Follow { from: Some(0) }, WORK)
        .await
        .unwrap();
    let page = source.page(2000, None, WORK).await.unwrap();
    let replay = source.page(2000, Some(1252), WORK).await.unwrap();
    // A saved range that starts inside a line fails without resetting either live reader.
    let rejected = match source
        .native(
            Operation::Window {
                start: 1,
                end: 1252,
            },
            WORK,
        )
        .await
    {
        Err(Fault::Core) => "core",
        _ => "published_or_wrong_fault",
    };
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&bytes[1518..])
        .unwrap();
    let after = source
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let other_after = other
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let quiet = source
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    let final_page = source.page(2000, None, WORK).await.unwrap();
    let mut job = source.start_page(2000, None, WORK).await.unwrap();
    job.cancel(WORK).await.unwrap();
    let cancelled = match job.wait().await {
        Err(Fault::Cancelled) => "cancelled",
        _ => "published_or_wrong_fault",
    };
    let after_cancel = source
        .native(Operation::Follow { from: None }, WORK)
        .await
        .unwrap();
    source.close(WORK).await.unwrap();
    other.close(WORK).await.unwrap();
    assert_eq!(
        json!({"initial":initial,"other_initial":other_initial,"page":page,"replay":replay,"rejected":rejected,"after":after,"other_after":other_after,"quiet":quiet,"final_page":final_page,"cancelled":cancelled,"after_cancel":after_cancel}),
        json!({"initial":native("pending",0,1518),"other_initial":native("other",0,1518),"page":old("follow-codex-kernel","page-00-pending.txt"),"replay":old("follow-codex-kernel","page-01-replay-without-question.txt"),"rejected":"core","after":native("pending",1518,3836),"other_after":native("other",1518,3836),"quiet":native("pending",3836,3836),"final_page":old("follow-codex-kernel","page-02-final.txt"),"cancelled":"cancelled","after_cancel":native("pending",3836,3836)})
    );
}
