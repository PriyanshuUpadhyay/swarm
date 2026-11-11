//! Each saved child has one supervisor and independently owned stdout/stderr drains.
use super::protocol::*;
use std::{fs::File, path::PathBuf, process::Stdio};
use swarm::view::transcript::worker_protocol::MAX_PART_BYTES;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
    process::{Child, Command},
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

pub enum Event {
    Descriptor(Reply),
    Staged(Tag),
    Fenced(Tag),
    Done {
        tag: Tag,
        result: Result<(Exit, Vec<Reply>)>,
        cleanup: Result<()>,
    },
}
pub enum Order {
    Cancel(Budget),
    Close(Budget),
}
pub struct Running {
    pub tx: UnboundedSender<Order>,
    pub task: JoinHandle<()>,
}

pub fn spawn(
    file: File,
    mut job: Job,
    budget: Budget,
    events: UnboundedSender<Event>,
) -> Result<Running> {
    let listener = match &job.guard {
        Guard::None => None,
        Guard::HoldBeforeCommit { gate_path, .. } => {
            let path = std::path::Path::new(gate_path);
            let cwd = std::env::current_dir().map_err(|_| "io")?;
            let parent = path
                .parent()
                .ok_or("bad_request")?
                .canonicalize()
                .map_err(|_| "io")?;
            if !path.is_absolute()
                || parent != cwd
                || gate_path.len() >= UNIX_SOCKET_PATH_BYTES_WITH_NUL
            {
                return Err("bad_request");
            }
            Some(UnixListener::bind(path).map_err(|_| "io")?)
        }
    };
    let gate_path = match &job.guard {
        Guard::None => None,
        Guard::HoldBeforeCommit { gate_path, .. } => Some(PathBuf::from(gate_path)),
    };
    let result = (|| {
        job.work = budget.remaining()?;
        let text = serde_json::to_string(&job).map_err(|_| "encode")?;
        if text.len() > MAX_JOB_JSON_BYTES {
            return Err("bad_request");
        }
        let root = std::env::current_dir().map_err(|_| "io")?;
        let mut command = Command::new(std::env::current_exe().map_err(|_| "io")?);
        command
            .args(["executor", "--protocol", "1", "--job-json", &text])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", root.join("home"))
            .env("XDG_CONFIG_HOME", root.join("home/config"))
            .env("XDG_CACHE_HOME", root.join("home/cache"))
            .env("XDG_STATE_HOME", root.join("home/state"))
            .env("XDG_DATA_HOME", root.join("home/data"))
            .env("TMPDIR", &root)
            .env("TMP", &root)
            .env("TEMP", &root)
            .env("CARGO_HOME", root.join("home/cargo"))
            .env("CARGO_TARGET_DIR", root.join("home/target"))
            .current_dir(&root)
            .stdin(Stdio::from(file))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = command.spawn().map_err(|_| "io")?;
        drop(command);
        // A spawn cannot be preempted. The supervisor charges elapsed work before publication.
        Ok(child)
    })();
    let mut child = match result {
        Ok(child) => child,
        Err(e) => {
            drop(listener);
            if let Some(p) = gate_path {
                std::fs::remove_file(p).map_err(|_| "io")?;
            }
            return Err(e);
        }
    };
    let stdout = child.stdout.take().ok_or("io")?;
    let stderr = child.stderr.take().ok_or("io")?;
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let drain_out = tokio::spawn(async move {
        let mut stdout = stdout;
        loop {
            match read_frame(&mut stdout).await {
                Ok(Some(bytes)) => {
                    let frame = serde_json::from_slice::<Reply>(&bytes).map_err(|_| "bad_frame");
                    let _ = out_tx.send(frame);
                }
                Ok(None) => return Ok(()),
                Err(e) => {
                    let _ = out_tx.send(Err(e));
                    drain(stdout).await?;
                    return Err(e);
                }
            }
        }
    });
    let drain_err = tokio::spawn(drain(stderr));
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let tag = job.tag.clone();
        let mut completion_budget = budget;
        let result = supervise(
            &mut child,
            &job,
            listener,
            out_rx,
            rx,
            &events,
            &mut completion_budget,
        )
        .await;
        // Job outcome and resource cleanup are separate. A previously failed job cannot fail Close.
        let fallback = Budget::cleanup();
        let mut cleanup = Ok(());
        let mut result = match result {
            Ok(v) => Ok(v),
            Err(code) => {
                if child.start_kill().is_err() {
                    cleanup = Err("io");
                }
                if let Err(e) = fallback
                    .run(async { child.wait().await.map_err(|_| "io") })
                    .await
                {
                    cleanup = Err(e);
                    // Retain ownership even after reporting that the cleanup limit was missed.
                    if child.wait().await.is_err() {
                        cleanup = Err("io");
                    }
                }
                Err(code)
            }
        };
        let drains = async {
            let (out, err) = tokio::join!(drain_out, drain_err);
            let out = out.map_err(|_| "io")?;
            // A stderr I/O failure must survive an expected truncated stdout frame.
            err.map_err(|_| "io")?.and(out)
        };
        let mut drains = std::pin::pin!(drains);
        let drained = tokio::select! {
            r=drains.as_mut()=>Some(r),
            _=completion_budget.expired()=>None,
        };
        let drained = match drained {
            Some(r) => {
                if completion_budget.check_now().is_err() {
                    result = Err("deadline");
                }
                r
            }
            None => {
                result = Err("deadline");
                tokio::select! {
                    r=drains.as_mut()=>r,
                    _=fallback.expired()=> {
                        cleanup = Err("deadline");
                        drains.await
                    }
                }
            }
        };
        // Our kill can truncate a frame. Natural malformed output was already rejected by supervise.
        if let Err("io") = drained {
            result = Err("io");
            cleanup = Err("io");
        }
        if let Some(path) = gate_path
            && std::fs::remove_file(path).is_err()
        {
            cleanup = Err("io");
        }
        let _ = events.send(Event::Done {
            tag,
            result,
            cleanup,
        });
    });
    Ok(Running { tx, task })
}
async fn drain(mut pipe: impl AsyncRead + Unpin) -> Result<()> {
    let mut b = [0; 65536];
    loop {
        if pipe.read(&mut b).await.map_err(|_| "io")? == 0 {
            return Ok(());
        }
    }
}

struct Transaction {
    descriptor: bool,
    begun: bool,
    parts: u64,
    bytes: u64,
    committed: bool,
    frames: Vec<Reply>,
}
impl Transaction {
    fn new() -> Self {
        Self {
            descriptor: false,
            begun: false,
            parts: 0,
            bytes: 0,
            committed: false,
            frames: Vec::new(),
        }
    }
    fn accept(
        &mut self,
        frame: Reply,
        tag: &Tag,
        pid: u32,
        events: &UnboundedSender<Event>,
    ) -> Result<()> {
        if frame.tag() != Some(tag) {
            return Err("binding_mismatch");
        }
        match &frame {
            Reply::Descriptor {
                pid: actual,
                device_id,
                inode_id,
                ..
            } if !self.descriptor
                && !self.begun
                && *actual == pid
                && decimal(device_id)
                && decimal(inode_id) =>
            {
                self.descriptor = true;
                events.send(Event::Descriptor(frame)).map_err(|_| "io")?;
                return Ok(());
            }
            Reply::Begin { .. } if self.descriptor && !self.begun => self.begun = true,
            Reply::Part {
                part_index, bytes, ..
            } if self.begun
                && !self.committed
                && *part_index == self.parts
                && !bytes.is_empty()
                && bytes.len() <= MAX_PART_BYTES =>
            {
                self.parts = self.parts.checked_add(1).ok_or("encode")?;
                self.bytes = self.bytes.checked_add(bytes.len() as u64).ok_or("encode")?;
            }
            Reply::Commit {
                part_count,
                payload_bytes,
                ..
            } if self.begun
                && !self.committed
                && *part_count == self.parts
                && *payload_bytes == self.bytes =>
            {
                self.committed = true
            }
            _ => return Err("bad_frame"),
        }
        self.frames.push(frame);
        Ok(())
    }
}
fn decimal(v: &str) -> bool {
    !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())
}

async fn supervise(
    child: &mut Child,
    job: &Job,
    listener: Option<UnixListener>,
    mut out: UnboundedReceiver<Result<Reply>>,
    mut orders: UnboundedReceiver<Order>,
    events: &UnboundedSender<Event>,
    completion_budget: &mut Budget,
) -> Result<(Exit, Vec<Reply>)> {
    let work = *completion_budget;
    let pid = child.id().ok_or("io")?;
    let mut transaction = Transaction::new();
    let (mut staged, mut eof) = (false, false);
    let mut status = None;
    let mut gate = None;
    loop {
        work.check_now()?;
        if eof && status.is_some() {
            let exit = Exit::from_status(status.ok_or("io")?);
            return if transaction.committed && exit.success() {
                Ok((exit, transaction.frames))
            } else {
                Err("executor_exit")
            };
        }
        tokio::select! {
            biased;
            order=orders.recv()=> {
                let (cancel,budget)=match order {Some(Order::Cancel(b))=>(true,b),Some(Order::Close(b))=>(false,b),None=>return Err("io")};
                *completion_budget=budget;
                if cancel && listener.is_some() {
                    let listener=listener.as_ref().ok_or("io")?;
                    let (mut stream,_)=budget.run(async{listener.accept().await.map_err(|_|"io")}).await?;
                    budget.run(async{stream.write_all(&[1]).await.map_err(|_|"io")}).await?;
                    while !transaction.committed {
                        let frame=budget.run(async{out.recv().await.ok_or("bad_frame")?}).await?;
                        transaction.accept(frame,&job.tag,pid,events)?;
                    }
                    // This receipt follows a real late Commit, not merely a fence assignment.
                    events.send(Event::Fenced(job.tag.clone())).map_err(|_|"io")?;
                    gate=Some(stream);
                }
                if !cancel || matches!(job.guard,Guard::None|Guard::HoldBeforeCommit{after_commit:AfterCommit::WaitForKill,..}) {
                    child.start_kill().map_err(|_|"io")?;
                }
                let s=budget.run(async{child.wait().await.map_err(|_|"io")}).await?;
                // Keep the gate alive until the owned child has actually exited.
                drop(gate);
                return Ok((Exit::from_status(s),transaction.frames));
            }
            _=work.expired()=>return Err("deadline"),
            frame=out.recv(), if !eof=>match frame {
                Some(frame)=>{
                    transaction.accept(frame?,&job.tag,pid,events)?;
                    if !staged && transaction.parts>0 && listener.is_some() {
                        staged=true;events.send(Event::Staged(job.tag.clone())).map_err(|_|"io")?;
                    }
                }
                None=>eof=true,
            },
            s=child.wait(), if status.is_none()=>status=Some(s.map_err(|_|"io")?),
        }
    }
}
