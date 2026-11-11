//! Wire types for the private, same-binary transcript worker.

use super::source::EndPolicy;
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 1_048_576;
pub const MAX_PART_BYTES: usize = 65_536;
/// Keeps escaped binding metadata and a full byte-array part within one frame.
pub const MAX_SOURCE_ID_BYTES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub source_id: String,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderBinding {
    pub reader_id: u64,
    pub generation: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Hello {
        protocol: u16,
        remaining_work_ms: u64,
    },
    Open {
        request_id: u64,
        binding: Binding,
        format: String,
        path: String,
        remaining_work_ms: u64,
    },
    Snapshot {
        request_id: u64,
        binding: Binding,
        start_byte: u64,
        end_byte: Option<u64>,
        end_policy: EndPolicy,
        remaining_work_ms: u64,
    },
    BindReader {
        request_id: u64,
        binding: Binding,
        reader: ReaderBinding,
        from_byte: u64,
        remaining_work_ms: u64,
    },
    ReplayReader {
        request_id: u64,
        binding: Binding,
        reader: ReaderBinding,
        start_byte: u64,
        end_byte: u64,
        remaining_work_ms: u64,
    },
    Advance {
        request_id: u64,
        binding: Binding,
        remaining_work_ms: u64,
    },
    CloseReader {
        request_id: u64,
        binding: Binding,
        reader: ReaderBinding,
        remaining_work_ms: u64,
    },
    Close {
        request_id: u64,
        binding: Binding,
        remaining_work_ms: u64,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Ready {
        protocol: u16,
        formats: Vec<String>,
    },
    Opened {
        request_id: u64,
        binding: Binding,
    },
    Begin {
        request_id: u64,
        binding: Binding,
    },
    Part {
        request_id: u64,
        binding: Binding,
        part_index: u64,
        bytes: Vec<u8>,
    },
    Commit {
        request_id: u64,
        binding: Binding,
        part_count: u64,
        payload_bytes: u64,
    },
    Closed {
        request_id: u64,
        binding: Binding,
    },
    Fault {
        request_id: Option<u64>,
        binding: Option<Binding>,
        code: FaultCode,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultCode {
    BadFrame,
    BadRequest,
    UnsupportedVersion,
    UnsupportedFormat,
    BindingMismatch,
    InvalidRange,
    Io,
    InputChanged,
    Deadline,
    Core,
    Encode,
}

/// The saved-page supervisor has its own envelope; v1 remains unchanged.
#[cfg(feature = "serve")]
pub mod v2 {
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Operation {
        Window { start: u64, end: u64 },
        Raw { tail: u16, before: Option<u64> },
        Line { offset: u64 },
        Follow { from: Option<u64> },
    }

    use super::{Binding, MAX_FRAME_BYTES};
    use serde::{Deserialize, Serialize};
    use std::{future::Future, time::Duration};
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
    use tokio::time::Instant;

    pub const PROTOCOL: u16 = 2;
    pub type Result<T> = std::result::Result<T, Code>;

    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Code {
        UnsupportedVersion,
        BindingMismatch,
        Cancelled,
        Core,
        BadRequest,
        BadFrame,
        Deadline,
        Io,
    }

    #[derive(Clone, Copy)]
    pub struct Budget(pub Instant);
    impl Budget {
        pub fn new(duration: Duration) -> Result<Self> {
            Instant::now()
                .checked_add(duration)
                .map(Self)
                .ok_or(Code::BadRequest)
        }
        pub fn check(self) -> Result<()> {
            if Instant::now() >= self.0 {
                Err(Code::Deadline)
            } else {
                Ok(())
            }
        }
        pub fn remaining_ms(self) -> Result<u64> {
            self.check()?;
            let ms = u64::try_from(self.0.saturating_duration_since(Instant::now()).as_millis())
                .map_err(|_| Code::BadRequest)?;
            if ms == 0 { Err(Code::Deadline) } else { Ok(ms) }
        }
        pub async fn run<T>(self, future: impl Future<Output = Result<T>>) -> Result<T> {
            self.check()?;
            let value = tokio::time::timeout_at(self.0, future)
                .await
                .map_err(|_| Code::Deadline)??;
            self.check()?;
            Ok(value)
        }
    }
    impl super::super::source::Work for Budget {
        fn check(&mut self) -> std::result::Result<(), super::super::source::SnapshotError> {
            Budget::check(*self).map_err(|_| super::super::source::SnapshotError::Deadline)
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Tag {
        pub request_id: u64,
        pub binding: Binding,
        pub transport_id: u64,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Request {
        Hello {
            protocol: u16,
            remaining_work_ms: u64,
        },
        Open {
            request_id: u64,
            binding: Binding,
            format: String,
            path: String,
            remaining_work_ms: u64,
        },
        Page {
            request_id: u64,
            binding: Binding,
            tail: u16,
            before: Option<u64>,
            remaining_work_ms: u64,
        },
        Native {
            request_id: u64,
            binding: Binding,
            operation: Operation,
            remaining_work_ms: u64,
        },
        Cancel {
            request_id: u64,
            binding: Binding,
            target_request_id: u64,
            remaining_work_ms: u64,
        },
        Close {
            request_id: u64,
            binding: Binding,
            remaining_work_ms: u64,
        },
    }
    impl Request {
        pub fn context(&self) -> (Option<u64>, Option<&Binding>, u64) {
            match self {
                Self::Hello {
                    remaining_work_ms, ..
                } => (None, None, *remaining_work_ms),
                Self::Open {
                    request_id,
                    binding,
                    remaining_work_ms,
                    ..
                }
                | Self::Page {
                    request_id,
                    binding,
                    remaining_work_ms,
                    ..
                }
                | Self::Native {
                    request_id,
                    binding,
                    remaining_work_ms,
                    ..
                }
                | Self::Cancel {
                    request_id,
                    binding,
                    remaining_work_ms,
                    ..
                }
                | Self::Close {
                    request_id,
                    binding,
                    remaining_work_ms,
                } => (Some(*request_id), Some(binding), *remaining_work_ms),
            }
        }
    }
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Exit {
        Code { code: i32 },
        Signal { signal: i32 },
    }
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
    pub enum Reply {
        Ready {
            protocol: u16,
            formats: Vec<String>,
        },
        Opened {
            request_id: u64,
            binding: Binding,
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
        Fenced {
            tag: Tag,
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
            code: Code,
        },
    }

    pub async fn read_frame(input: &mut (impl AsyncRead + Unpin)) -> Result<Option<Vec<u8>>> {
        let mut prefix = [0; 4];
        if input.read(&mut prefix[..1]).await.map_err(|_| Code::Io)? == 0 {
            return Ok(None);
        }
        input
            .read_exact(&mut prefix[1..])
            .await
            .map_err(|_| Code::BadFrame)?;
        let len = u32::from_be_bytes(prefix) as usize;
        if !(1..=MAX_FRAME_BYTES).contains(&len) {
            return Err(Code::BadFrame);
        }
        let mut bytes = vec![0; len];
        input
            .read_exact(&mut bytes)
            .await
            .map_err(|_| Code::BadFrame)?;
        Ok(Some(bytes))
    }
    pub async fn write_frame(
        output: &mut (impl AsyncWrite + Unpin),
        value: &impl Serialize,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(value).map_err(|_| Code::Core)?;
        if !(1..=MAX_FRAME_BYTES).contains(&bytes.len()) {
            return Err(Code::BadFrame);
        }
        output
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(|_| Code::Io)?;
        output.write_all(&bytes).await.map_err(|_| Code::Io)?;
        output.flush().await.map_err(|_| Code::Io)
    }
}
