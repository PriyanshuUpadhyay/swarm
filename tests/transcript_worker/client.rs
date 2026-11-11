//! The wire client of the worker tests: starts the real executable and speaks its frame protocol.
//! It is a system boundary (child process, pipes, frames), so it holds no domain knowledge.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixtures::{decode_deep, fixture_dir};

pub(super) const MAX_FRAME_BYTES: usize = 1_048_576;
pub(super) const MAX_PART_BYTES: usize = 65_536;
// A caller budget for one operation. It is a setting, not a measured time.
pub(super) const WORK_MS: u64 = 500;
// Only a hung worker waits this long, once. A normal run, and the red run, return at once.
// Each wait is under one second.
const HANG_CAP: Duration = Duration::from_millis(900);
const EXIT_POLLS: u32 = 180;
const EXIT_POLL_STEP: Duration = Duration::from_millis(5);

pub(super) struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: Receiver<Result<Option<Vec<u8>>, String>>,
    stderr: Receiver<String>,
    threads: Vec<JoinHandle<()>>,
    pub(super) scratch: PathBuf,
    // The caller budget sent with each Snapshot. It is a setting, never a measured time.
    pub(super) snapshot_work_ms: u64,
}

impl Worker {
    /// Starts the real executable with an empty environment and a private HOME, so no live
    /// state, store or app can be touched.
    pub(super) fn start() -> Worker {
        let scratch = std::env::temp_dir().join(format!(
            "swarm-transcript-worker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(scratch.join("home")).expect("scratch home");
        let home = scratch.join("home");
        let mut child = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .args(["_transcript-worker", "--protocol", "1"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("TMPDIR", &scratch)
            .current_dir(&scratch)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the swarm executable");
        let stdin = child.stdin.take();
        let mut stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let (frame_tx, frames) = mpsc::channel();
        let (err_tx, err_rx) = mpsc::channel();
        // Both pipes are drained for the whole life of the child, so a full pipe cannot stall it.
        let out_thread = std::thread::spawn(move || {
            loop {
                let mut prefix = [0u8; 4];
                match read_exact_or_eof(&mut stdout, &mut prefix) {
                    Ok(false) => {
                        let _ = frame_tx.send(Ok(None));
                        return;
                    }
                    Err(error) => {
                        let _ = frame_tx.send(Err(error));
                        return;
                    }
                    Ok(true) => {}
                }
                let len = u32::from_be_bytes(prefix) as usize;
                if len == 0 || len > MAX_FRAME_BYTES {
                    let _ =
                        frame_tx.send(Err(format!("stdout is not a frame: length prefix {len}")));
                    return;
                }
                let mut body = vec![0u8; len];
                if let Err(error) = stdout.read_exact(&mut body) {
                    let _ = frame_tx.send(Err(format!("truncated frame: {error}")));
                    return;
                }
                if frame_tx.send(Ok(Some(body))).is_err() {
                    return;
                }
            }
        });
        let err_thread = std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = stderr.read_to_end(&mut text);
            let _ = err_tx.send(String::from_utf8_lossy(&text).into_owned());
        });
        Worker {
            child,
            stdin,
            frames,
            stderr: err_rx,
            threads: vec![out_thread, err_thread],
            scratch,
            snapshot_work_ms: WORK_MS,
        }
    }

    /// Stops the child and ends the test with what the executable actually did.
    fn fail(&mut self, what: &str) -> ! {
        self.stdin.take();
        let _ = self.child.kill();
        let status = self.child.wait();
        let stderr = self
            .stderr
            .recv_timeout(Duration::from_millis(500))
            .unwrap_or_default();
        panic!("{what}: worker status {status:?}, stderr {stderr:?}");
    }

    fn send(&mut self, request: &Value) {
        let body = serde_json::to_vec(request).expect("request JSON");
        assert!(
            body.len() <= MAX_FRAME_BYTES,
            "the test request fits one frame"
        );
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        let sent = match self.stdin.as_mut() {
            Some(stdin) => stdin.write_all(&frame).and_then(|()| stdin.flush()),
            None => Err(std::io::Error::other("stdin already closed")),
        };
        if let Err(error) = sent {
            self.fail(&format!(
                "worker did not accept the request {}: {error}",
                request["op"]
            ));
        }
    }

    pub(super) fn frame(&mut self) -> Option<Value> {
        match self.frames.recv_timeout(HANG_CAP) {
            Ok(Ok(Some(body))) => Some(decode_deep(&body)),
            Ok(Ok(None)) => None,
            Ok(Err(error)) => self.fail(&error),
            Err(RecvTimeoutError::Timeout) => self.fail("worker sent no frame before the hang cap"),
            Err(RecvTimeoutError::Disconnected) => self.fail("frame reader ended"),
        }
    }

    fn expect_frame(&mut self, what: &str) -> Value {
        match self.frame() {
            Some(frame) => frame,
            None => self.fail(&format!("worker closed stdout before {what}")),
        }
    }

    pub(super) fn exit_code(&mut self) -> i32 {
        self.stdin.take();
        for _ in 0..EXIT_POLLS {
            if let Ok(Some(status)) = self.child.try_wait() {
                return status.code().unwrap_or(-1);
            }
            std::thread::sleep(EXIT_POLL_STEP);
        }
        self.fail("worker did not exit")
    }

    pub(super) fn hello(&mut self, protocol: u64, remaining_work_ms: u64) -> Value {
        self.send(
            &json!({"op": "hello", "protocol": protocol, "remaining_work_ms": remaining_work_ms}),
        );
        self.expect_frame("the ready or fault reply")
    }

    pub(super) fn open(
        &mut self,
        request_id: u64,
        binding: &Value,
        format: &str,
        file: &str,
    ) -> Value {
        let path = fixture_dir().join("inputs").join(file);
        self.send(
            &json!({"op": "open", "request_id": request_id, "binding": binding, "format": format,
            "path": path.to_str().expect("utf-8 path"), "remaining_work_ms": WORK_MS}),
        );
        self.expect_frame("the opened or fault reply")
    }

    /// Runs one snapshot. A fault reply comes back as `Err`; a success is the committed payload
    /// bytes, undecoded, so a caller can read them without a lossy number type.
    pub(super) fn snapshot_raw(
        &mut self,
        request_id: u64,
        binding: &Value,
        start: u64,
        end: &Value,
        policy: &str,
    ) -> Result<RawCommitted, Value> {
        self.send(&json!({"op": "snapshot", "request_id": request_id, "binding": binding, "start_byte": start,
            "end_byte": end, "end_policy": policy, "remaining_work_ms": self.snapshot_work_ms}));
        let begin = self.expect_frame("the begin or fault reply");
        if begin["op"] == "fault" {
            return Err(begin);
        }
        assert_eq!(
            begin,
            json!({"op": "begin", "request_id": request_id, "binding": binding})
        );
        let mut payload = Vec::new();
        let mut parts = 0u64;
        loop {
            let frame = self.expect_frame("the commit");
            match frame["op"].as_str() {
                Some("part") => {
                    assert_eq!(frame["request_id"], json!(request_id));
                    assert_eq!(frame["binding"], *binding);
                    assert_eq!(
                        frame["part_index"],
                        json!(parts),
                        "parts are contiguous from zero"
                    );
                    let bytes = frame["bytes"].as_array().expect("part bytes");
                    assert!(
                        bytes.len() <= MAX_PART_BYTES,
                        "a part carries at most 64 KiB of raw bytes"
                    );
                    payload.extend(
                        bytes
                            .iter()
                            .map(|b| u8::try_from(b.as_u64().expect("byte")).expect("byte")),
                    );
                    parts += 1;
                }
                Some("commit") => {
                    assert_eq!(
                        frame,
                        json!({"op": "commit", "request_id": request_id, "binding": binding,
                            "part_count": parts, "payload_bytes": payload.len()})
                    );
                    return Ok(RawCommitted { parts, payload });
                }
                other => self.fail(&format!(
                    "unexpected frame {other:?} inside a snapshot reply"
                )),
            }
        }
    }

    /// The same snapshot with the payload decoded as a JSON value (the decoder of the first 13 groups).
    pub(super) fn snapshot(
        &mut self,
        request_id: u64,
        binding: &Value,
        start: u64,
        end: &Value,
        policy: &str,
    ) -> Result<Committed, Value> {
        self.snapshot_raw(request_id, binding, start, end, policy)
            .map(|raw| Committed {
                parts: raw.parts,
                payload_bytes: raw.payload.len(),
                value: decode_deep(&raw.payload),
            })
    }

    pub(super) fn close(&mut self, request_id: u64, binding: &Value) {
        self.send(&json!({"op": "close", "request_id": request_id, "binding": binding, "remaining_work_ms": WORK_MS}));
        let closed = self.expect_frame("the closed reply");
        assert_eq!(
            closed,
            json!({"op": "closed", "request_id": request_id, "binding": binding})
        );
        assert_eq!(
            self.exit_code(),
            0,
            "Close releases the file and exits zero"
        );
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

pub(super) struct RawCommitted {
    pub(super) parts: u64,
    pub(super) payload: Vec<u8>,
}

pub(super) struct Committed {
    pub(super) parts: u64,
    pub(super) payload_bytes: usize,
    pub(super) value: Value,
}

fn read_exact_or_eof(input: &mut impl Read, buf: &mut [u8]) -> Result<bool, String> {
    let mut filled = 0;
    while filled < buf.len() {
        match input.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => return Err("stdout ended inside a length prefix".to_string()),
            Ok(n) => filled += n,
            Err(error) => return Err(format!("stdout read failed: {error}")),
        }
    }
    Ok(true)
}
