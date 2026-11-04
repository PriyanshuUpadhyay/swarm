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
