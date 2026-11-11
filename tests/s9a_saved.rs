//! G1 freezes the serve caller API. This module is absent on c84c3919; that is not RED.
#![cfg(feature = "serve")]
#[path = "../src/serve/transcript_caller.rs"]
mod caller;
#[path = "s9a/frozen.rs"]
pub mod frozen;
use caller::{Fault, Source};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use swarm::view::transcript::worker_protocol::Binding;
use tokio::process::Command;
const WORK: Duration = Duration::from_millis(900);
// Cold process startup has a separate bound from protocol work.
const STARTUP: Duration = Duration::from_secs(10);

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("s9a-saved-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("OWNER.json"), "{\"owner\":\"orchestrator\"}\n").unwrap();
        std::fs::create_dir(p.join("home")).unwrap();
        Self(p)
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
        .current_dir(root);
    command.args(["_transcript-worker", "--protocol", "2"]);
    command
}
fn peer(mode: &str, root: &Path) -> Command {
    let mut command = Command::new("/usr/bin/python3");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/s9a/peer.py"))
        .arg(mode)
        .arg(root);
    command
}
fn peer_alive(root: &Path, mode: &str) -> bool {
    let pid = std::fs::read_to_string(root.join(format!("{mode}.pid"))).unwrap();
    std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}
fn binding(name: &str) -> Binding {
    Binding {
        source_id: name.into(),
        generation: 1,
    }
}
async fn open(command: Command, path: &Path, name: &str) -> Result<Source, Fault> {
    Source::open(command, binding(name), "claude", path, STARTUP).await
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

#[tokio::test]
async fn g1_saved_ranges_keep_old_rows_and_the_held_file() {
    let tree = Tree::new();
    let (mut actual, mut expected) = (serde_json::Map::new(), serde_json::Map::new());
    for case in ["claude-chair-run", "claude-edit"] {
        let path = tree.path(case);
        std::fs::write(&path, frozen::bytes(&format!("inputs/{case}.jsonl"))).unwrap();
        let source = open(real(&tree.0), &path, case).await.unwrap();
        let mut requests = vec![
            ("rows-full.txt".to_string(), 2000, None),
            ("rows-tail-1.txt".to_string(), 1, None),
            ("rows-tail-3.txt".to_string(), 3, None),
        ];
        let mut before = None;
        for n in 0..40 {
            let name = format!("rows-back-{n:02}.txt");
            let page = old(case, &name);
            requests.push((name, 2, before));
            let start = page["page"]["start_offset"].as_u64().unwrap();
            if start == 0 {
                break;
            }
            before = Some(start);
        }
        // A pathname replacement must not change the generation's saved descriptor.
        std::fs::rename(&path, tree.path(&format!("{case}-held"))).unwrap();
        std::fs::write(&path, b"replacement, not the open log\n").unwrap();
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
async fn g1_publication_cancel_and_saved_failure_guards() {
    let tree = Tree::new();
    let path = tree.path("claude.jsonl");
    std::fs::write(&path, frozen::bytes("inputs/claude-chair-run.jsonl")).unwrap();
    let incompatible = match open(peer("incompatible", &tree.0), &path, "bad-version").await {
        Err(Fault::UnsupportedProtocol) => "unsupported_protocol",
        _ => "published_or_wrong_fault",
    };
    let stale = open(peer("stale", &tree.0), &path, "stale").await.unwrap();
    let binding_guard = match stale.page(2000, None, WORK).await {
        Err(Fault::BindingMismatch) => "binding_mismatch",
        _ => "published_or_wrong_fault",
    };
    stale.close(WORK).await.unwrap();
    let staged = open(peer("cancel", &tree.0), &path, "cancel")
        .await
        .unwrap();
    // start_page returns after Begin. The peer withholds Commit until Cancel, then sends it late.
    let mut job = staged.start_page(2000, None, WORK).await.unwrap();
    job.cancel(WORK).await.unwrap();
    let cancel = match job.wait().await {
        Err(Fault::Cancelled) => "cancelled",
        _ => "late_commit_published_or_wrong_fault",
    };
    staged.close(WORK).await.unwrap();
    let payload = tree.path("old-page.json");
    std::fs::write(
        &payload,
        serde_json::to_vec(&old("claude-chair-run", "rows-full.txt")).unwrap(),
    )
    .unwrap();
    let mut failed_peer = peer("saved-failure", &tree.0);
    failed_peer.arg(&payload);
    let first = open(failed_peer, &path, "first").await.unwrap();
    let other = open(real(&tree.0), &path, "other").await.unwrap();
    let saved_failure = match first.page(2000, None, WORK).await {
        Err(Fault::Core) => "core",
        _ => "published_or_wrong_fault",
    };
    let after = first.page(2000, None, WORK).await.unwrap();
    let survives = other.page(2000, None, WORK).await.unwrap();
    first.close(WORK).await.unwrap();
    other.close(WORK).await.unwrap();
    let children_alive: Vec<_> = ["incompatible", "stale", "cancel", "saved-failure"]
        .map(|mode| peer_alive(&tree.0, mode))
        .into();
    assert_eq!(
        json!({"protocol":incompatible,"binding":binding_guard,"cancel":cancel,"saved_failure":saved_failure,"same_source":after,"other_source":survives,"children_alive":children_alive}),
        json!({"protocol":"unsupported_protocol","binding":"binding_mismatch","cancel":"cancelled","saved_failure":"core","same_source":old("claude-chair-run","rows-full.txt"),"other_source":old("claude-chair-run","rows-full.txt"),"children_alive":[false,false,false,false]})
    );
}
