//! Frozen synthetic AGY inputs and OLD-captured expectations (c8 Zig executable, captured twice,
//! see tmp/flow/transcript-caller-check-u06/capture). Nothing here is computed by Rust under test:
//! the event lines are the Zig output, the offsets are the byte lengths of the data files
//! (136 + 125 = 261 for the unfinished file, 136 + 125 + 16 = 277 once completed, which also
//! matches the Zig `--tail 2` page end_offset 277).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

pub const LINE1: &[u8] = include_bytes!("data/line1.jsonl");
pub const PREFIX: &[u8] = include_bytes!("data/line2_prefix.bin");
pub const SUFFIX: &[u8] = include_bytes!("data/line2_suffix.jsonl");
pub const REPLACEMENT: &[u8] = include_bytes!("data/replacement.jsonl");
pub const REPLACEMENT_APPEND: &[u8] = include_bytes!("data/replacement_append.jsonl");
const OLD_EVENT_1: &str = include_str!("data/old_event_line1.json");
const OLD_EVENT_2: &str = include_str!("data/old_event_line2.json");

pub const END1: u64 = 136;
pub const READ_END: u64 = 261;
pub const END2: u64 = 277;
/// The kernel cursor the trial sets once at Open.
pub const CURSOR: u64 = 7;
pub const SOURCE_ID: &str = "u06-seed-a-source";

pub fn binding() -> Value {
    json!({"source_id": SOURCE_ID, "generation": 1})
}

pub fn reader() -> Value {
    json!({"reader_id": 1, "generation": 1})
}

pub fn line1_entry() -> Value {
    json!({"start_byte": 0, "end_byte": END1, "terminated": true,
        "events": [serde_json::from_str::<Value>(OLD_EVENT_1).expect("OLD event 1")]})
}

pub fn line2_entry() -> Value {
    json!({"start_byte": END1, "end_byte": END2, "terminated": true,
        "events": [serde_json::from_str::<Value>(OLD_EVENT_2).expect("OLD event 2")]})
}

/// The native part of one reader update. Row projections are not part of this trial.
pub fn update(read_end: u64, complete_end: u64, lines: Vec<Value>) -> Value {
    json!({"reader": reader(), "origin_byte": 0, "read_end_byte": read_end,
        "complete_end_byte": complete_end, "next_complete_byte": complete_end, "lines": lines})
}

/// The native part of one saved read.
pub fn snapshot(end: u64, lines: Vec<Value>) -> Value {
    json!({"source_id": SOURCE_ID, "read_end_byte": end, "complete_end_byte": end, "lines": lines})
}

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const ATTEMPTS: u32 = 64;

/// One exclusive short subroot per test. Creation is exclusive, so parallel tests and repeated
/// attempts never share a name, and `Drop` removes only the directory this value created.
pub struct Tree {
    pub root: PathBuf,
}

impl Tree {
    pub fn create() -> Tree {
        for _ in 0..ATTEMPTS {
            let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("u{}-{n}", std::process::id()));
            match std::fs::create_dir(&root) {
                Ok(()) => {
                    let seed = root.join("brain/seed-a");
                    std::fs::create_dir_all(&seed).expect("seed dir");
                    std::fs::create_dir_all(root.join("home")).expect("home dir");
                    let mut original = LINE1.to_vec();
                    original.extend_from_slice(PREFIX);
                    std::fs::write(seed.join("log.jsonl"), original).expect("original");
                    return Tree { root };
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("subroot {}: {e}", root.display()),
            }
        }
        panic!("no free subroot name after {ATTEMPTS} attempts");
    }

    pub fn original(&self) -> PathBuf {
        self.root.join("brain/seed-a/log.jsonl")
    }

    fn renamed(&self) -> PathBuf {
        self.root.join("renamed-outside-brain.jsonl")
    }

    /// A socket path that must fit sun_path (104 bytes including the NUL).
    pub fn gate(&self, name: &str) -> PathBuf {
        let path = self.root.join(format!("{name}.sock"));
        assert!(
            path.as_os_str().len() < 104,
            "socket path too long: {}",
            path.display()
        );
        path
    }

    /// Moves the open file outside the seed directory and puts different valid content at the
    /// original path.
    pub fn rename_and_replace(&self) {
        std::fs::rename(self.original(), self.renamed()).expect("rename");
        std::fs::write(self.original(), REPLACEMENT).expect("replacement");
    }

    /// Completes the held partial line in the renamed file, and appends other text to the replacement.
    pub fn complete_partial(&self) {
        append(&self.renamed(), SUFFIX);
        append(&self.original(), REPLACEMENT_APPEND);
    }
}

fn append(path: &Path, bytes: &[u8]) {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("append open");
    file.write_all(bytes).expect("append");
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
