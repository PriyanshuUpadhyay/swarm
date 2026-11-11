//! Private Rust worker caller for saved and live transcript reads.
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use swarm::view::transcript::worker_protocol::{
    Binding, MAX_PART_BYTES, MAX_SOURCE_ID_BYTES,
    v2::{self, Budget, Code, Operation, Reply, Request, Tag},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, ChildStdin, Command},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    UnsupportedProtocol,
    BindingMismatch,
    Cancelled,
    Core,
}
impl From<Code> for Fault {
    fn from(code: Code) -> Self {
        match code {
            Code::UnsupportedVersion => Self::UnsupportedProtocol,
            Code::BindingMismatch => Self::BindingMismatch,
            Code::Cancelled => Self::Cancelled,
            _ => Self::Core,
        }
    }
}
type Result<T> = std::result::Result<T, Fault>;
type Answer<T> = oneshot::Sender<Result<T>>;

enum Job {
    Page {
        tail: u16,
        before: Option<u64>,
    },
    #[allow(dead_code)] // The saved-only test mounts this module on its own.
    Native(Operation),
}
enum Order {
    Start {
        job: Job,
        budget: Budget,
        begun: Answer<u64>,
        done: Answer<Value>,
    },
    Cancel {
        target: u64,
        budget: Budget,
        done: Answer<()>,
    },
    Close {
        budget: Budget,
        done: Answer<()>,
    },
}

#[derive(Clone)]
pub struct Source {
    orders: mpsc::UnboundedSender<Order>,
}
pub struct PageJob {
    orders: mpsc::UnboundedSender<Order>,
    target: u64,
    done: Option<oneshot::Receiver<Result<Value>>>,
    cancelled: bool,
}
impl Source {
    #[allow(dead_code)] // The saved-only test mounts this module on its own.
    pub fn is_closed(&self) -> bool {
        self.orders.is_closed()
    }
    pub async fn open(
        mut command: Command,
        binding: Binding,
        format: &str,
        path: &Path,
        duration: Duration,
    ) -> Result<Self> {
        let budget = Budget::new(duration)?;
        if binding.source_id.is_empty()
            || binding.source_id.len() > MAX_SOURCE_ID_BYTES
            || !path.is_absolute()
        {
            return Err(Fault::Core);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| Fault::Core)?;
        let input = child.stdin.take().ok_or(Fault::Core)?;
        let mut output = child.stdout.take().ok_or(Fault::Core)?;
        let stderr = child.stderr.take().ok_or(Fault::Core)?;
        let (tx, frames) = mpsc::channel(8);
        let stdout = tokio::spawn(async move {
            let result = async {
                while let Some(bytes) = v2::read_frame(&mut output).await? {
                    let reply =
                        serde_json::from_slice::<Reply>(&bytes).map_err(|_| Code::BadFrame)?;
                    if tx.send(Ok(reply)).await.is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            }
            .await;
            if let Err(code) = result {
                let _ = tx.send(Err(code)).await;
            }
            drop(tx);
            drain(output).await
        });
        let stderr = tokio::spawn(drain(stderr));
        let mut connection = Connection {
            child,
            input: Some(input),
            frames,
            stdout,
            stderr,
            binding,
            next: 0,
            transport: 0,
        };
        let opened = connection.open(format, path, budget).await;
        if let Err(fault) = opened {
            let _ = connection.cleanup(true, budget).await;
            return Err(fault);
        }
        let (orders, rx) = mpsc::unbounded_channel();
        tokio::spawn(connection.run(rx));
        Ok(Self { orders })
    }
    pub async fn start_page(
        &self,
        tail: u16,
        before: Option<u64>,
        duration: Duration,
    ) -> Result<PageJob> {
        self.start(Job::Page { tail, before }, Budget::new(duration)?)
            .await
    }
    #[allow(dead_code)] // The saved-only test mounts this module on its own.
    pub async fn native(&self, operation: Operation, duration: Duration) -> Result<Value> {
        self.start(Job::Native(operation), Budget::new(duration)?)
            .await?
            .wait()
            .await
    }
    async fn start(&self, job: Job, budget: Budget) -> Result<PageJob> {
        let (begun, begin) = oneshot::channel();
        let (done, result) = oneshot::channel();
        self.orders
            .send(Order::Start {
                job,
                budget,
                begun,
                done,
            })
            .map_err(|_| Fault::Core)?;
        let target = begin.await.map_err(|_| Fault::Core)??;
        Ok(PageJob {
            orders: self.orders.clone(),
            target,
            done: Some(result),
            cancelled: false,
        })
    }
    pub async fn page(&self, tail: u16, before: Option<u64>, duration: Duration) -> Result<Value> {
        self.start_page(tail, before, duration).await?.wait().await
    }
    pub async fn close(self, duration: Duration) -> Result<()> {
        let budget = Budget::new(duration)?;
        let (done, result) = oneshot::channel();
        self.orders
            .send(Order::Close { budget, done })
            .map_err(|_| Fault::Core)?;
        result.await.map_err(|_| Fault::Core)?
    }
}
impl PageJob {
    pub async fn cancel(&mut self, duration: Duration) -> Result<()> {
        if self.cancelled {
            return Ok(());
        }
        let budget = Budget::new(duration)?;
        let (done, result) = oneshot::channel();
        self.orders
            .send(Order::Cancel {
                target: self.target,
                budget,
                done,
            })
            .map_err(|_| Fault::Core)?;
        result.await.map_err(|_| Fault::Core)??;
        self.cancelled = true;
        Ok(())
    }
    pub async fn wait(mut self) -> Result<Value> {
        let result = self
            .done
            .as_mut()
            .ok_or(Fault::Core)?
            .await
            .map_err(|_| Fault::Core)?;
        self.done.take();
        if self.cancelled {
            Err(Fault::Cancelled)
        } else {
            result
        }
    }
}
impl Drop for PageJob {
    fn drop(&mut self) {
        if self.done.is_some() && !self.cancelled {
            let (done, _) = oneshot::channel();
            if let Ok(budget) = Budget::new(Duration::from_secs(1)) {
                let _ = self.orders.send(Order::Cancel {
                    target: self.target,
                    budget,
                    done,
                });
            }
        }
    }
}

struct Connection {
    child: Child,
    input: Option<ChildStdin>,
    frames: mpsc::Receiver<v2::Result<Reply>>,
    stdout: JoinHandle<v2::Result<()>>,
    stderr: JoinHandle<v2::Result<()>>,
    binding: Binding,
    next: u64,
    transport: u64,
}
#[path = "transcript_caller/connection.rs"]
mod connection;

async fn drain(mut input: impl AsyncRead + Unpin) -> v2::Result<()> {
    let mut bytes = [0; 65536];
    while input.read(&mut bytes).await.map_err(|_| Code::Io)? != 0 {}
    Ok(())
}
