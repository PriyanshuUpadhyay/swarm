//! Wire client for the reader tests: starts the real `swarm _transcript-worker --protocol 1`
//! executable and speaks its frame protocol with literal JSON. It is a system boundary (child,
//! pipes, frames) and holds no domain knowledge. Every logical wait is at most 900 ms in total.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const MAX_FRAME_BYTES: usize = 1_048_576;
// A caller budget sent with each request. It is a setting, never a measured time.
pub(super) const WORK_MS: u64 = 500;
const WAIT: Duration = Duration::from_millis(900);
const EXIT_POLLS: u32 = 180;
const EXIT_POLL_STEP: Duration = Duration::from_millis(5);

// Tests of one binary run in parallel inside one process, so the process id alone does not name a root.
static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);
// A fixed bound on name attempts: a leftover root of an older process can take a name, never the same
// name twice in one process.
const SCRATCH_ATTEMPTS: u32 = 64;

/// Creates a private scratch root that this worker alone owns: the name is the process id plus a
/// process-wide counter, and creation is exclusive, so two workers can never share a root and
/// `Drop` removes only the root its own worker created.
fn create_scratch() -> PathBuf {
    for _ in 0..SCRATCH_ATTEMPTS {
        let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let scratch = std::env::temp_dir().join(format!(
            "swarm-transcript-readers-{}-{sequence}",
            std::process::id()
        ));
        match std::fs::create_dir(&scratch) {
            Ok(()) => return scratch,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("scratch root {}: {error}", scratch.display()),
        }
    }
    panic!("no free scratch root name after {SCRATCH_ATTEMPTS} attempts");
}

type Frames = Receiver<Result<Option<Vec<u8>>, String>>;

pub(super) struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: Frames,
    stderr: Receiver<String>,
    threads: Vec<JoinHandle<()>>,
    pub(super) scratch: PathBuf,
    request_id: u64,
}

impl Worker {
    /// Starts the executable with an empty environment and a private HOME, so no live state,
    /// store or app can be touched. Both pipes are drained for the whole life of the child.
    pub(super) fn start() -> Worker {
        let scratch = create_scratch();
        let home = scratch.join("home");
        std::fs::create_dir_all(&home).expect("scratch home");
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
            .env("TMP", &scratch)
            .env("TEMP", &scratch)
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
                    let _ = frame_tx.send(Err(format!("stdout is not a frame: length {len}")));
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
            request_id: 0,
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
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&body);
        let sent = match self.stdin.as_mut() {
            Some(stdin) => stdin.write_all(&frame).and_then(|()| stdin.flush()),
            None => Err(std::io::Error::other("stdin already closed")),
        };
        if let Err(error) = sent {
            self.fail(&format!("worker did not accept {}: {error}", request["op"]));
        }
    }

    /// One frame before `deadline`; `None` means the worker closed stdout.
    fn frame_by(&mut self, deadline: Instant) -> Option<Value> {
        let left = deadline.saturating_duration_since(Instant::now());
        match self.frames.recv_timeout(left) {
            Ok(Ok(Some(body))) => Some(serde_json::from_slice(&body).expect("frame JSON")),
            Ok(Ok(None)) => None,
            Ok(Err(error)) => self.fail(&error),
            Err(RecvTimeoutError::Timeout) => self.fail("worker sent no frame within 900 ms"),
            Err(RecvTimeoutError::Disconnected) => self.fail("frame reader ended"),
        }
    }

    /// The next frame, or `None` once the worker has closed stdout.
    pub(super) fn frame(&mut self) -> Option<Value> {
        self.frame_by(Instant::now() + WAIT)
    }

    pub(super) fn hello(&mut self) -> Value {
        self.send(&json!({"op": "hello", "protocol": 1, "remaining_work_ms": WORK_MS}));
        self.frame()
            .unwrap_or_else(|| self.fail("closed before ready"))
    }

    pub(super) fn open(&mut self, binding: &Value, path: &str) -> Value {
        self.request_id += 1;
        self.send(
            &json!({"op": "open", "request_id": self.request_id, "binding": binding,
            "format": "claude", "path": path, "remaining_work_ms": WORK_MS}),
        );
        self.frame()
            .unwrap_or_else(|| self.fail("closed before opened"))
    }

    /// Sends a reader request (`fields` are the op-specific members) and returns the committed
    /// `updates`, or the fault frame. Begin, ordered parts and commit are checked here.
    pub(super) fn reader_op(
        &mut self,
        op: &str,
        binding: &Value,
        fields: Value,
    ) -> Result<Value, Value> {
        self.request_id += 1;
        let id = self.request_id;
        let mut request = json!({"op": op, "request_id": id, "binding": binding,
            "remaining_work_ms": WORK_MS});
        for (key, value) in fields.as_object().expect("fields object") {
            request[key] = value.clone();
        }
        self.send(&request);
        let deadline = Instant::now() + WAIT;
        let begin = self
            .frame_by(deadline)
            .unwrap_or_else(|| self.fail(&format!("{op}: closed before begin")));
        if begin["op"] == "fault" {
            return Err(begin);
        }
        assert_eq!(
            begin,
            json!({"op": "begin", "request_id": id, "binding": binding})
        );
        let mut payload = Vec::new();
        let mut parts = 0u64;
        loop {
            let frame = self
                .frame_by(deadline)
                .unwrap_or_else(|| self.fail(&format!("{op}: closed before commit")));
            match frame["op"].as_str() {
                Some("part") => {
                    assert_eq!(frame["request_id"], json!(id));
                    assert_eq!(frame["part_index"], json!(parts), "parts are contiguous");
                    payload.extend(
                        frame["bytes"]
                            .as_array()
                            .expect("part bytes")
                            .iter()
                            .map(|b| u8::try_from(b.as_u64().expect("byte")).expect("byte")),
                    );
                    parts += 1;
                }
                Some("commit") => {
                    assert_eq!(
                        frame,
                        json!({"op": "commit", "request_id": id, "binding": binding,
                            "part_count": parts, "payload_bytes": payload.len()})
                    );
                    let batch: Value = serde_json::from_slice(&payload).expect("payload JSON");
                    return Ok(batch["updates"].clone());
                }
                other => self.fail(&format!("{op}: unexpected frame {other:?}")),
            }
        }
    }

    /// Closes the source; the worker answers `closed` and exits zero.
    pub(super) fn close_source(&mut self, binding: &Value) {
        self.request_id += 1;
        let id = self.request_id;
        self.send(&json!({"op": "close", "request_id": id, "binding": binding,
            "remaining_work_ms": WORK_MS}));
        let closed = self
            .frame()
            .unwrap_or_else(|| self.fail("closed before closed"));
        assert_eq!(
            closed,
            json!({"op": "closed", "request_id": id, "binding": binding})
        );
        assert_eq!(
            self.exit_code(),
            0,
            "Close releases the file and exits zero"
        );
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
