//! Private trial wire and one propagated work deadline.
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};
use swarm::view::transcript::{
    source::{EndPolicy, SnapshotError, Work},
    worker_protocol::{Binding, MAX_FRAME_BYTES, MAX_PART_BYTES, ReaderBinding},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::Instant,
};

pub const CLEANUP_BUDGET_MS: u64 = 900;
// TRIAL-V4/V5: only one eligible started frame gets this extra time from its original deadline.
const STARTED_FRAME_ALLOWANCE_MS: u64 = 900;
pub const UNIX_SOCKET_PATH_BYTES_WITH_NUL: usize = 104;
pub const MAX_JOB_JSON_BYTES: usize = 16384;
pub const KERNEL_CURSOR_WITNESS_BYTE: u64 = 7;

pub type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrialWork {
    Remaining { remaining_work_ms: u64 },
    UntilCancelled,
}

#[derive(Clone, Copy, Debug)]
pub struct Budget(pub Option<Instant>);
impl Budget {
    pub fn new(work: TrialWork, received: Instant) -> Result<Self> {
        match work {
            TrialWork::UntilCancelled => Ok(Self(None)),
            TrialWork::Remaining { remaining_work_ms } => {
                let end = received
                    .checked_add(Duration::from_millis(remaining_work_ms))
                    .ok_or("bad_request")?;
                Ok(Self(Some(end)))
            }
        }
    }
    pub fn cleanup() -> Self {
        Self(Some(
            Instant::now() + Duration::from_millis(CLEANUP_BUDGET_MS),
        ))
    }
    pub fn check_now(self) -> Result<()> {
        if self.0.is_some_and(|d| Instant::now() >= d) {
            Err("deadline")
        } else {
            Ok(())
        }
    }
    pub fn remaining(self) -> Result<TrialWork> {
        self.check_now()?;
        match self.0 {
            None => Ok(TrialWork::UntilCancelled),
            Some(d) => {
                let ms = u64::try_from(d.saturating_duration_since(Instant::now()).as_millis())
                    .map_err(|_| "bad_request")?;
                if ms == 0 {
                    Err("deadline")
                } else {
                    Ok(TrialWork::Remaining {
                        remaining_work_ms: ms,
                    })
                }
            }
        }
    }
    pub async fn run<T>(self, f: impl Future<Output = Result<T>>) -> Result<T> {
        self.check_now()?;
        let v = match self.0 {
            Some(d) => tokio::time::timeout_at(d, f)
                .await
                .map_err(|_| "deadline")?,
            None => f.await,
        }?;
        self.check_now()?;
        Ok(v)
    }
    pub async fn expired(self) {
        match self.0 {
            Some(d) => tokio::time::sleep_until(d).await,
            None => std::future::pending().await,
        }
    }
}
impl Work for Budget {
    fn check(&mut self) -> std::result::Result<(), SnapshotError> {
        self.check_now().map_err(|_| SnapshotError::Deadline)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub binding: Binding,
    pub request_id: u64,
    pub transport_id: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterCommit {
    Exit,
    WaitForKill,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Guard {
    None,
    HoldBeforeCommit {
        gate_path: String,
        after_commit: AfterCommit,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub tag: Tag,
    pub format: String,
    pub original_open_path: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub end_policy: EndPolicy,
    pub work: TrialWork,
    pub guard: Guard,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Hello {
        protocol: u16,
        work: TrialWork,
    },
    Open {
        request_id: u64,
        binding: Binding,
        format: String,
        path: String,
        work: TrialWork,
    },
    Bind {
        request_id: u64,
        binding: Binding,
        reader: ReaderBinding,
        from_byte: u64,
        work: TrialWork,
    },
    Advance {
        request_id: u64,
        binding: Binding,
        work: TrialWork,
    },
    Read {
        request_id: u64,
        binding: Binding,
        start_byte: u64,
        end_byte: u64,
        end_policy: EndPolicy,
        work: TrialWork,
        guard: Guard,
    },
    Cancel {
        request_id: u64,
        binding: Binding,
        target_request_id: u64,
        work: TrialWork,
    },
    Close {
        request_id: u64,
        binding: Binding,
        work: TrialWork,
    },
}
impl Control {
    pub fn context(&self) -> (Option<u64>, Option<&Binding>, TrialWork) {
        match self {
            Self::Hello { work, .. } => (None, None, *work),
            Self::Open {
                request_id,
                binding,
                work,
                ..
            }
            | Self::Bind {
                request_id,
                binding,
                work,
                ..
            }
            | Self::Advance {
                request_id,
                binding,
                work,
                ..
            }
            | Self::Read {
                request_id,
                binding,
                work,
                ..
            }
            | Self::Cancel {
                request_id,
                binding,
                work,
                ..
            }
            | Self::Close {
                request_id,
                binding,
                work,
                ..
            } => (Some(*request_id), Some(binding), *work),
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Exit {
    Code { code: i32 },
    Signal { signal: i32 },
}
impl Exit {
    pub fn from_status(s: std::process::ExitStatus) -> Self {
        use std::os::unix::process::ExitStatusExt;
        match s.code() {
            Some(code) => Self::Code { code },
            None => Self::Signal {
                signal: s.signal().unwrap_or(0),
            },
        }
    }
    pub fn success(&self) -> bool {
        matches!(self, Self::Code { code: 0 })
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Ready {
        protocol: u16,
    },
    Opened {
        request_id: u64,
        binding: Binding,
        device_id: String,
        inode_id: String,
        kernel_cursor_byte: u64,
    },
    Descriptor {
        tag: Tag,
        device_id: String,
        inode_id: String,
        pid: u32,
    },
    Begin {
        tag: Tag,
    },
    Part {
        tag: Tag,
        part_index: u64,
        bytes: Vec<u8>,
    },
    Commit {
        tag: Tag,
        part_count: u64,
        payload_bytes: u64,
    },
    Staged {
        tag: Tag,
    },
    Fenced {
        tag: Tag,
    },
    Position {
        tag: Tag,
        kernel_cursor_byte: u64,
    },
    Cancelled {
        request_id: u64,
        binding: Binding,
        target_request_id: u64,
        transport_id: u64,
        exit: Exit,
    },
    Closed {
        request_id: u64,
        binding: Binding,
    },
    Fault {
        request_id: Option<u64>,
        binding: Option<Binding>,
        code: String,
    },
}
impl Reply {
    pub fn tag(&self) -> Option<&Tag> {
        match self {
            Self::Descriptor { tag, .. }
            | Self::Begin { tag }
            | Self::Part { tag, .. }
            | Self::Commit { tag, .. }
            | Self::Staged { tag }
            | Self::Fenced { tag }
            | Self::Position { tag, .. } => Some(tag),
            _ => None,
        }
    }
}
pub fn fault(id: Option<u64>, binding: Option<Binding>, code: &str) -> Reply {
    Reply::Fault {
        request_id: id,
        binding,
        code: code.into(),
    }
}
pub fn payload(tag: &Tag, bytes: &[u8]) -> Result<Vec<Reply>> {
    let mut out = vec![Reply::Begin { tag: tag.clone() }];
    for (i, part) in bytes.chunks(MAX_PART_BYTES).enumerate() {
        out.push(Reply::Part {
            tag: tag.clone(),
            part_index: u64::try_from(i).map_err(|_| "encode")?,
            bytes: part.to_vec(),
        });
    }
    out.push(Reply::Commit {
        tag: tag.clone(),
        part_count: u64::try_from(out.len() - 1).map_err(|_| "encode")?,
        payload_bytes: u64::try_from(bytes.len()).map_err(|_| "encode")?,
    });
    Ok(out)
}
pub async fn read_frame(r: &mut (impl AsyncRead + Unpin)) -> Result<Option<Vec<u8>>> {
    let mut prefix = [0; 4];
    let n = r.read(&mut prefix[..1]).await.map_err(|_| "io")?;
    if n == 0 {
        return Ok(None);
    }
    r.read_exact(&mut prefix[1..])
        .await
        .map_err(|_| "bad_frame")?;
    let len = u32::from_be_bytes(prefix) as usize;
    if !(1..=MAX_FRAME_BYTES).contains(&len) {
        return Err("bad_frame");
    }
    let mut bytes = vec![0; len];
    r.read_exact(&mut bytes).await.map_err(|_| "bad_frame")?;
    Ok(Some(bytes))
}
#[derive(Debug)]
pub enum FrameError {
    Boundary(&'static str),
    Transport(&'static str),
}
impl FrameError {
    fn code(self) -> &'static str {
        match self {
            Self::Boundary(code) | Self::Transport(code) => code,
        }
    }
}

pub async fn write_frame_at_boundary(
    w: &mut (impl AsyncWrite + Unpin),
    reply: &Reply,
    budget: Budget,
) -> std::result::Result<(), FrameError> {
    let bytes = serde_json::to_vec(reply).map_err(|_| FrameError::Boundary("encode"))?;
    if !(1..=MAX_FRAME_BYTES).contains(&bytes.len()) {
        return Err(FrameError::Boundary("encode"));
    }
    // Encoding is charged before the first byte. Once writing starts, timeout can lose framing.
    budget.check_now().map_err(FrameError::Boundary)?;
    let write = async {
        w.write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(|_| "io")?;
        w.write_all(&bytes).await.map_err(|_| "io")?;
        w.flush().await.map_err(|_| "io")
    };
    let finish_allowed = match reply {
        Reply::Ready { .. }
        | Reply::Opened { .. }
        | Reply::Descriptor { .. }
        | Reply::Begin { .. }
        | Reply::Part { .. }
        | Reply::Commit { .. }
        | Reply::Staged { .. }
        | Reply::Position { .. } => true,
        Reply::Fault { .. }
        | Reply::Fenced { .. }
        | Reply::Cancelled { .. }
        | Reply::Closed { .. } => false,
    };
    match budget.0 {
        Some(deadline) if finish_allowed => {
            let finish_deadline = deadline
                .checked_add(Duration::from_millis(STARTED_FRAME_ALLOWANCE_MS))
                .ok_or(FrameError::Transport("deadline"))?;
            let mut write = std::pin::pin!(write);
            let result = match tokio::time::timeout_at(deadline, &mut write).await {
                Ok(result) => result,
                Err(_) => tokio::time::timeout_at(finish_deadline, &mut write)
                    .await
                    .map_err(|_| FrameError::Transport("deadline"))?,
            };
            result.map_err(FrameError::Transport)?;
            // Timeout polls a ready write first; reject success observed after the finish cap.
            if Instant::now() >= finish_deadline {
                return Err(FrameError::Transport("deadline"));
            }
            Ok(())
        }
        Some(deadline) => tokio::time::timeout_at(deadline, write)
            .await
            .map_err(|_| FrameError::Transport("deadline"))?
            .map_err(FrameError::Transport),
        None => write.await.map_err(FrameError::Transport),
    }
    // No post-write work check can undo a completely delivered frame or Commit.
}

pub async fn write_frame(
    w: &mut (impl AsyncWrite + Unpin),
    reply: &Reply,
    budget: Budget,
) -> Result<()> {
    write_frame_at_boundary(w, reply, budget)
        .await
        .map_err(FrameError::code)
}
