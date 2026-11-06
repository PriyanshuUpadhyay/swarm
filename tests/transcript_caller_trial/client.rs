//! Root client: starts a real child CLI entry point and speaks four-byte-length JSON frames.
//! `SWARM_CALLER_ENTRY=old-worker` drives `swarm _transcript-worker --protocol 1` on its own
//! wire (checker-owned baseline adapter, no isolation). Unset drives the built example
//! `transcript_caller_trial authority --protocol 1`; a missing binary fails, never skips.
//! Each method is one logical observation (`Obs`, 900 ms for all its frames, waits and writes).
//! The client is the protocol boundary guard: a frame that breaks the frozen wire fails the check.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::fixtures::{CURSOR, binding};

#[path = "proc.rs"]
mod proc;
#[path = "trial.rs"]
mod trial;

pub use proc::Obs;
use proc::Proc;

const MAX_FRAME_BYTES: usize = 1_048_576;
const MAX_PART_BYTES: usize = 65_536;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Entry {
    OldWorker,
    Trial,
}

impl Entry {
    pub fn from_env() -> Entry {
        match std::env::var("SWARM_CALLER_ENTRY") {
            Err(std::env::VarError::NotPresent) => Entry::Trial,
            Ok(v) if v == "old-worker" => Entry::OldWorker,
            Ok(v) => panic!("unknown SWARM_CALLER_ENTRY {v:?}"),
            Err(e) => panic!("SWARM_CALLER_ENTRY unreadable: {e}"),
        }
    }

    fn command(self) -> Command {
        let swarm = env!("CARGO_BIN_EXE_swarm");
        match self {
            Entry::OldWorker => {
                let mut c = Command::new(swarm);
                c.args(["_transcript-worker", "--protocol", "1"]);
                c
            }
            Entry::Trial => {
                let path = std::env::var_os("SWARM_CALLER_EXAMPLE").map_or_else(
                    || {
                        Path::new(swarm)
                            .parent()
                            .expect("bin dir")
                            .join("examples/transcript_caller_trial")
                    },
                    PathBuf::from,
                );
                let mut c = Command::new(path);
                c.args(["authority", "--protocol", "1"]);
                c
            }
        }
    }
}

/// A staged guarded job: request id, saved transport id and the executor pid the authority reported.
pub struct Held {
    pub id: u64,
    pub transport_id: u64,
    pub pid: u64,
}

/// Live payloads (transport 0) carry no executor diagnostics; saved payloads (transport >= 1) do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Live,
    Saved,
}

pub struct Session {
    entry: Entry,
    p: Proc,
    buf: Vec<u8>,
    root: PathBuf,
    request_id: u64,
    opened: Option<(String, String)>,
}

fn is_decimal(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}

impl Session {
    /// Empty environment, private HOME/XDG/TMPDIR/cwd under the exclusive subroot. Startup is
    /// charged to `obs`, the same observation the following `hello` uses.
    pub fn start(entry: Entry, root: &Path, obs: &Obs) -> Result<Session, String> {
        Self::spawn(entry, entry.command(), root, obs)
    }

    /// The one spawn path: gives `cmd` the isolated environment, then starts it. `start` passes
    /// the real entry point; a stand-in child gets the same environment and lifecycle.
    pub fn spawn(
        entry: Entry,
        mut cmd: Command,
        root: &Path,
        obs: &Obs,
    ) -> Result<Session, String> {
        let home = root.join("home");
        cmd.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("TMPDIR", root)
            .env("TMP", root)
            .env("TEMP", root)
            .current_dir(root);
        let p = Proc::spawn("authority", cmd, true, obs)?;
        Ok(Session {
            entry,
            p,
            buf: Vec::new(),
            root: root.to_path_buf(),
            request_id: 0,
            opened: None,
        })
    }

    /// Waits for a short-lived stand-in child to exit with all its output queued for `next_out`.
    pub fn wait_for_stand_in(&mut self, obs: &Obs) -> Result<(), String> {
        self.p.finish(false, obs)
    }

    fn send(&mut self, request: &Value, obs: &Obs) -> Result<(), String> {
        let body = serde_json::to_vec(request).expect("request JSON");
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        self.p
            .send(frame, obs)
            .map_err(|e| format!("{}: {e}", request["op"]))
    }

    /// One whole frame, or `None` at a clean EOF between frames.
    fn frame_by(&mut self, obs: &Obs) -> Result<Option<Value>, String> {
        loop {
            if self.buf.len() >= 4 {
                let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]])
                    as usize;
                if len == 0 || len > MAX_FRAME_BYTES {
                    return Err(format!("stdout is not a frame: length {len}"));
                }
                if self.buf.len() >= 4 + len {
                    let body: Vec<u8> = self.buf.drain(..4 + len).skip(4).collect();
                    return serde_json::from_slice(&body)
                        .map(Some)
                        .map_err(|e| format!("frame JSON: {e}"));
                }
            }
            match self.p.next_out(obs)? {
                Some(chunk) => self.buf.extend(chunk),
                None if self.buf.is_empty() => return Ok(None),
                None => return Err("stdout ended inside a frame".into()),
            }
        }
    }

    fn frame_now(&mut self, obs: &Obs, what: &str) -> Result<Value, String> {
        self.frame_by(obs)?
            .ok_or_else(|| format!("stdout closed before {what}"))
    }

    /// Builds one request. The finite Work grant is the budget left in this observation.
    fn build(&mut self, op: &str, fields: Value, obs: &Obs) -> Result<(u64, Value), String> {
        self.request_id += 1;
        let id = self.request_id;
        let mut r = json!({"op": op, "request_id": id, "binding": binding()});
        for (k, v) in fields.as_object().expect("fields") {
            r[k] = v.clone();
        }
        let ms = obs.remaining_ms()?;
        match self.entry {
            Entry::OldWorker => r["remaining_work_ms"] = json!(ms),
            Entry::Trial => r["work"] = json!({"kind": "remaining", "remaining_work_ms": ms}),
        }
        Ok((id, r))
    }

    pub fn hello(&mut self, obs: &Obs) -> Result<(), String> {
        let ms = obs.remaining_ms()?;
        let mut r = json!({"op": "hello", "protocol": 1});
        match self.entry {
            Entry::OldWorker => r["remaining_work_ms"] = json!(ms),
            Entry::Trial => r["work"] = json!({"kind": "remaining", "remaining_work_ms": ms}),
        }
        self.send(&r, obs)?;
        let f = self.frame_now(obs, "ready")?;
        if f["op"] != "ready" || f["protocol"] != 1 {
            return Err(format!("not ready: {f}"));
        }
        Ok(())
    }

    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let obs = Obs::new();
        let (id, r) = self.build("open", json!({"format": "agy", "path": path}), &obs)?;
        self.send(&r, &obs)?;
        let f = self.frame_now(&obs, "opened")?;
        if f["op"] != "opened" || f["request_id"] != id || f["binding"] != binding() {
            return Err(format!("bad opened: {f}"));
        }
        if self.entry == Entry::Trial {
            // Only the trial reports held-handle identity and sets the shared kernel cursor once.
            if f["kernel_cursor_byte"] != CURSOR
                || !is_decimal(&f["device_id"])
                || !is_decimal(&f["inode_id"])
            {
                return Err(format!("opened identity or cursor {CURSOR} wrong: {f}"));
            }
            self.opened = Some((
                f["device_id"].as_str().unwrap().into(),
                f["inode_id"].as_str().unwrap().into(),
            ));
        }
        Ok(())
    }

    /// Checks the exact frame context. OLD: top-level request and binding. Trial: a full tag with
    /// this binding and request, transport 0 for live and one constant transport >= 1 for saved.
    fn tag_check(
        &self,
        f: &Value,
        id: u64,
        kind: Kind,
        transport: &mut Option<u64>,
    ) -> Result<(), String> {
        if self.entry == Entry::OldWorker {
            return if f["request_id"] == id && f["binding"] == binding() {
                Ok(())
            } else {
                Err(format!("wrong request or binding: {f}"))
            };
        }
        let tag = f
            .get("tag")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("frame without a tag: {f}"))?;
        let t = tag.get("transport_id").and_then(Value::as_u64);
        let shape = tag.len() == 3
            && tag.get("binding") == Some(&binding())
            && tag.get("request_id") == Some(&json!(id));
        let live_ok = kind == Kind::Live && t == Some(0);
        let saved_ok = kind == Kind::Saved
            && t.is_some_and(|t| t >= 1 && transport.is_none_or(|prev| prev == t));
        if !shape || !(live_ok || saved_ok) || f.get("request_id").is_some() {
            return Err(format!(
                "frame tag does not match request {id} / {}: {f}",
                if kind == Kind::Live { "live" } else { "saved" }
            ));
        }
        *transport = t;
        Ok(())
    }

    /// Collects one payload as a single transaction: [Descriptor] Begin, ordered Parts, Commit
    /// [Position]. Duplicate or misordered Begin/Commit, oversize or non-byte parts fail.
    pub fn collect(&mut self, id: u64, kind: Kind, obs: &Obs) -> Result<Value, String> {
        let diagnostics = kind == Kind::Saved && self.entry == Entry::Trial;
        let (mut begun, mut committed, mut descriptor, mut position) = (false, false, false, false);
        let (mut bytes, mut parts, mut transport) = (Vec::<u8>::new(), 0u64, None);
        while !(committed && (!diagnostics || (descriptor && position))) {
            let f = self.frame_now(obs, "the payload finished")?;
            if f["op"] == "fault" {
                return Err(format!("fault frame {f}"));
            }
            self.tag_check(&f, id, kind, &mut transport)?;
            match f["op"].as_str() {
                Some("descriptor") if diagnostics && !descriptor && !begun => {
                    let same = self
                        .opened
                        .as_ref()
                        .is_some_and(|(d, i)| f["device_id"] == *d && f["inode_id"] == *i);
                    if !same || f["pid"].as_u64().is_none_or(|p| p == 0) {
                        return Err(format!("descriptor identity or pid wrong: {f}"));
                    }
                    descriptor = true;
                }
                Some("position")
                    if diagnostics
                        && descriptor
                        && !position
                        && f["kernel_cursor_byte"] == CURSOR =>
                {
                    position = true
                }
                Some("begin") if !begun && !committed && (!diagnostics || descriptor) => {
                    begun = true
                }
                Some("part") if begun && !committed && f["part_index"] == parts => {
                    let list = f["bytes"]
                        .as_array()
                        .filter(|l| l.len() <= MAX_PART_BYTES)
                        .ok_or("part bytes missing or oversize")?;
                    for b in list {
                        bytes.push(
                            b.as_u64()
                                .and_then(|n| u8::try_from(n).ok())
                                .ok_or_else(|| format!("part byte out of range: {b}"))?,
                        );
                    }
                    parts += 1;
                }
                Some("commit")
                    if begun
                        && !committed
                        && f["part_count"] == parts
                        && f["payload_bytes"] == bytes.len() =>
                {
                    committed = true
                }
                _ => return Err(format!("unexpected or misordered frame: {f}")),
            }
        }
        let value = serde_json::from_slice(&bytes).map_err(|e| format!("payload JSON: {e}"))?;
        // Publication fence, after decode: a Commit that is already buffered is still late once
        // the original observation has passed, because `recv_timeout(0)` hands back ready data,
        // and decoding can spend the last of the deadline.
        if obs.is_expired() {
            return Err("late result rejected: the original observation expired before the result was accepted".into());
        }
        Ok(value)
    }

    fn payload(&mut self, op: &str, fields: Value, kind: Kind) -> Result<Value, String> {
        let obs = Obs::new();
        let (id, r) = self.build(op, fields, &obs)?;
        self.send(&r, &obs)?;
        self.collect(id, kind, &obs)
    }

    fn first_update(&mut self, op: &str, fields: Value) -> Result<Value, String> {
        let batch = self.payload(op, fields, Kind::Live)?;
        let u = &batch["updates"][0];
        Ok(
            json!({"reader": u["reader"], "origin_byte": u["origin_byte"], "read_end_byte": u["read_end_byte"],
            "complete_end_byte": u["complete_end_byte"], "next_complete_byte": u["next_complete_byte"], "lines": u["lines"]}),
        )
    }

    pub fn bind(&mut self, reader: Value) -> Result<Value, String> {
        let op = if self.entry == Entry::OldWorker {
            "bind_reader"
        } else {
            "bind"
        };
        self.first_update(op, json!({"reader": reader, "from_byte": 0}))
    }

    pub fn advance(&mut self) -> Result<Value, String> {
        self.first_update("advance", json!({}))
    }

    /// A saved read from byte 0 to `end` with complete lines only; native part only.
    pub fn read(&mut self, end: u64) -> Result<Value, String> {
        let p = match self.entry {
            Entry::OldWorker => self.payload("snapshot", json!({"start_byte": 0, "end_byte": end, "end_policy": "complete_lines"}), Kind::Saved)?,
            Entry::Trial => self.payload(
                "read",
                json!({"start_byte": 0, "end_byte": end, "end_policy": "complete_lines", "guard": {"kind": "none"}}),
                Kind::Saved,
            )?,
        };
        Ok(
            json!({"source_id": p["source_id"], "read_end_byte": p["read_end_byte"],
            "complete_end_byte": p["complete_end_byte"], "lines": p["lines"]}),
        )
    }

    /// OLD's only cancellation: kill the one worker, reap it and join its owned threads, all within
    /// one observation. Its retained state is lost. A timeout is a failed check.
    pub fn kill_and_reap(&mut self) -> Result<(), String> {
        let obs = Obs::new();
        self.p.finish(true, &obs)?;
        println!(
            "reached: old worker killed and reaped, status {:?}",
            self.p.status.expect("reaped")
        );
        Ok(())
    }
}
