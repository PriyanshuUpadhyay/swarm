//! Owned child processes for the Root client (observation-v4). One owner thread runs a small
//! Tokio runtime that owns the `tokio::process::Child`, its control writer and its two drains.
//! The owner `select!`s between the child exiting and a kill order, and kills only through
//! `Child::start_kill`, which does nothing once the child is reaped. No numeric pid is ever
//! signalled and there is no `unsafe`. Every wait on the test thread is a `recv_timeout` against
//! one `Obs` deadline. This cannot cancel an arbitrary blocked syscall; a child that outlives its
//! deadline is reported, not hidden.

use std::process::{Command as StdCommand, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const OBSERVATION: Duration = Duration::from_millis(900);

/// One logical observation: a single deadline for every frame, wait and check inside it.
pub struct Obs(Instant);

impl Default for Obs {
    fn default() -> Self {
        Obs::new()
    }
}

impl Obs {
    pub fn new() -> Obs {
        Obs::with_budget(OBSERVATION)
    }

    /// Checker-owned budget setter. `new` keeps the 900 ms default.
    pub fn with_budget(budget: Duration) -> Obs {
        Obs(Instant::now() + budget)
    }

    /// True once the ORIGINAL deadline has passed; ready buffered data does not renew it.
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.0
    }

    pub fn left(&self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    /// The budget still left for a finite Work grant. It is never renewed inside the observation.
    pub fn remaining_ms(&self) -> Result<u64, String> {
        match u64::try_from(self.left().as_millis()) {
            Ok(ms) if ms > 0 => Ok(ms),
            _ => Err("observation budget is spent before the request".into()),
        }
    }
}

type Chunks = Receiver<Option<Vec<u8>>>;

pub struct Proc {
    name: String,
    pub pid: u32,
    pub status: Option<ExitStatus>,
    write_tx: Option<UnboundedSender<Vec<u8>>>,
    kill_tx: UnboundedSender<()>,
    ack_rx: Receiver<Result<(), String>>,
    out_rx: Chunks,
    err_rx: Chunks,
    out_eof: bool,
    err_eof: bool,
    exit_rx: Receiver<Result<ExitStatus, String>>,
    done_rx: Receiver<()>,
    done: bool,
    owner: Option<JoinHandle<()>>,
}

async fn pump(mut pipe: impl AsyncRead + Unpin, tx: Sender<Option<Vec<u8>>>) {
    let mut buf = vec![0u8; 65536];
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tx.send(Some(buf[..n].to_vec())).is_err() {
                    break;
                }
            }
        }
    }
    let _ = tx.send(None);
}

struct Wiring {
    out_tx: Sender<Option<Vec<u8>>>,
    err_tx: Sender<Option<Vec<u8>>>,
    ack_tx: Sender<Result<(), String>>,
    exit_tx: Sender<Result<ExitStatus, String>>,
    write_rx: UnboundedReceiver<Vec<u8>>,
    kill_rx: UnboundedReceiver<()>,
}

/// The single owner of the Child: it decides between exit and cancellation.
async fn own(mut child: Child, w: Wiring) {
    let out = tokio::spawn(pump(child.stdout.take().expect("piped stdout"), w.out_tx));
    let err = tokio::spawn(pump(child.stderr.take().expect("piped stderr"), w.err_tx));
    let mut write_rx = w.write_rx;
    let ack_tx = w.ack_tx;
    let writer = child.stdin.take().map(|mut stdin| {
        tokio::spawn(async move {
            while let Some(frame) = write_rx.recv().await {
                let r = async {
                    stdin.write_all(&frame).await?;
                    stdin.flush().await
                }
                .await
                .map_err(|e| e.to_string());
                let failed = r.is_err();
                let _ = ack_tx.send(r);
                if failed {
                    break;
                }
            }
            // Dropping `stdin` here closes the real control pipe end.
        })
    });
    let mut kill_rx = w.kill_rx;
    let status = loop {
        tokio::select! {
            s = child.wait() => break s.map_err(|e| e.to_string()),
            k = kill_rx.recv() => match k {
                // start_kill is a no-op once the child is reaped, so no stale target is signalled.
                Some(()) => { let _ = child.start_kill(); }
                None => break child.wait().await.map_err(|e| e.to_string()),
            },
        }
    };
    let _ = w.exit_tx.send(status);
    if let Some(writer) = writer {
        writer.abort();
    }
    let _ = out.await;
    let _ = err.await;
}

impl Proc {
    /// Spawns on the calling thread, so there is no startup channel to wait on. The `spawn` system
    /// call itself cannot be preempted; its cost is charged to `obs` right after it returns. If the
    /// observation is already spent, the child is killed and reaped in a separate cleanup
    /// observation and the check fails. The runtime and the Child then move into the owner thread.
    pub fn spawn(
        name: &str,
        cmd: StdCommand,
        piped_stdin: bool,
        obs: &Obs,
    ) -> Result<Proc, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("{name}: runtime: {e}"))?;
        let mut cmd = Command::from(cmd);
        cmd.stdin(if piped_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
        let child = {
            let _context = rt.enter();
            cmd.spawn().map_err(|e| format!("{name}: spawn: {e}"))?
        };
        let pid = child.id().unwrap_or(0);
        let (out_tx, out_rx) = mpsc::channel();
        let (err_tx, err_rx) = mpsc::channel();
        let (ack_tx, ack_rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let (write_tx, write_rx) = unbounded_channel();
        let (kill_tx, kill_rx) = unbounded_channel();
        let wiring = Wiring {
            out_tx,
            err_tx,
            ack_tx,
            exit_tx,
            write_rx,
            kill_rx,
        };
        let owner = std::thread::spawn(move || {
            rt.block_on(own(child, wiring));
            let _ = done_tx.send(());
        });
        let mut proc = Proc {
            name: name.to_owned(),
            pid,
            status: None,
            write_tx: piped_stdin.then_some(write_tx),
            kill_tx,
            ack_rx,
            out_rx,
            err_rx,
            out_eof: false,
            err_eof: false,
            exit_rx,
            done_rx,
            done: false,
            owner: Some(owner),
        };
        if obs.left().is_zero() {
            let cleanup = proc.finish(true, &Obs::new());
            return Err(format!(
                "{name}: the spawn call spent the observation; cleanup {cleanup:?}"
            ));
        }
        Ok(proc)
    }

    /// One control write, handed to the writer task and acknowledged before the observation ends.
    pub fn send(&mut self, bytes: Vec<u8>, obs: &Obs) -> Result<(), String> {
        let tx = self
            .write_tx
            .as_ref()
            .ok_or("control stdin is already closed")?;
        tx.send(bytes)
            .map_err(|_| "control writer ended".to_string())?;
        match self.ack_rx.recv_timeout(obs.left()) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "{}: control write not done within the observation",
                self.name
            )),
            Err(RecvTimeoutError::Disconnected) => Err("control writer ended".into()),
        }
    }

    /// Drops control stdin: the writer task drops the real pipe end.
    pub fn close_stdin(&mut self) {
        self.write_tx.take();
    }

    fn next(rx: &Chunks, eof: &mut bool, obs: &Obs, what: &str) -> Result<Option<Vec<u8>>, String> {
        if *eof {
            return Ok(None);
        }
        match rx.recv_timeout(obs.left()) {
            Ok(Some(chunk)) => Ok(Some(chunk)),
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                *eof = true;
                Ok(None)
            }
            Err(RecvTimeoutError::Timeout) => {
                Err(format!("{what} produced nothing within the observation"))
            }
        }
    }

    pub fn next_out(&mut self, obs: &Obs) -> Result<Option<Vec<u8>>, String> {
        let what = format!("{} pid {} stdout", self.name, self.pid);
        Self::next(&self.out_rx, &mut self.out_eof, obs, &what)
    }

    /// Reads stderr to EOF within the observation.
    pub fn drain_err(&mut self, obs: &Obs) -> Result<Vec<u8>, String> {
        let what = format!("{} pid {} stderr", self.name, self.pid);
        let mut all = Vec::new();
        while let Some(chunk) = Self::next(&self.err_rx, &mut self.err_eof, obs, &what)? {
            all.extend(chunk);
        }
        Ok(all)
    }

    pub fn wait_exit(&mut self, obs: &Obs) -> Result<ExitStatus, String> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        match self.exit_rx.recv_timeout(obs.left()) {
            Ok(Ok(status)) => {
                self.status = Some(status);
                Ok(status)
            }
            Ok(Err(e)) => Err(format!("{} pid {}: wait failed: {e}", self.name, self.pid)),
            Err(_) => Err(format!(
                "{} pid {} was not reaped within the observation",
                self.name, self.pid
            )),
        }
    }

    /// Ends the child (optionally by a kill order to its owner), reaps it and joins the owner
    /// within `obs`. An Err names the handle that did not finish; it is never a successful cleanup.
    pub fn finish(&mut self, kill: bool, obs: &Obs) -> Result<(), String> {
        self.close_stdin();
        if kill {
            let _ = self.kill_tx.send(());
        }
        self.wait_exit(obs)?;
        if !self.done {
            if self.done_rx.recv_timeout(obs.left()).is_err() {
                return Err(format!(
                    "{} pid {}: owner and I/O tasks did not finish",
                    self.name, self.pid
                ));
            }
            self.done = true;
        }
        if let Some(owner) = self.owner.take() {
            let _ = owner.join();
        }
        Ok(())
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if self.owner.is_none() {
            return;
        }
        if let Err(e) = self.finish(true, &Obs::new()) {
            eprintln!("CLEANUP INCOMPLETE: {e}");
            if !std::thread::panicking() {
                panic!("CLEANUP INCOMPLETE: {e}");
            }
        }
    }
}
