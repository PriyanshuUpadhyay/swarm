//! Pure, independent Claude snapshots with physical source positions.

use super::{Record, Row, view_rows_with_context};
use serde::{Deserialize, Serialize};
use serde_json::{Value, value::RawValue};
use std::collections::HashSet;

#[derive(Debug, Serialize)]
pub struct SourceMeta {
    pub source_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndPolicy {
    SnapshotEof,
    CompleteLines,
}

#[derive(Debug, Serialize)]
pub struct LineEvents {
    pub start_byte: u64,
    pub end_byte: u64,
    pub terminated: bool,
    pub events: Vec<NormalizedEvent>,
}

/// A provider event whose copied input keeps OLD's exact numeric tokens.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct NormalizedEvent(Box<RawValue>);

impl NormalizedEvent {
    pub(crate) fn new(value: &impl Serialize) -> Self {
        // Callers construct only JSON fields and validated raw numeric tokens, with no I/O.
        Self(serde_json::value::to_raw_value(value).expect("normalized event is valid JSON"))
    }

    fn row_input(&self) -> Option<Value> {
        let mut de = serde_json::Deserializer::from_str(self.0.get());
        // Canonical events contain only shallow fields and input already bounded to 255.
        // Keep that private depth policy while retaining the row consumer's number parser.
        de.disable_recursion_limit();
        Value::deserialize(&mut de).ok()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowProjectionError {
    UnrepresentableNumber,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RowProjection {
    Rows(Vec<Row>),
    Unavailable { error: RowProjectionError },
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub source_id: String,
    pub read_end_byte: u64,
    pub complete_end_byte: u64,
    pub lines: Vec<LineEvents>,
    pub rows: RowProjection,
}

#[derive(Debug)]
pub enum SnapshotError {
    OffsetOverflow,
    Deadline,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OffsetOverflow => "snapshot byte offset overflow",
            Self::Deadline => "snapshot work deadline reached",
        })
    }
}

impl std::error::Error for SnapshotError {}

pub trait Work {
    fn check(&mut self) -> Result<(), SnapshotError>;
}

#[derive(Debug)]
pub struct SnapshotReader {
    meta: SourceMeta,
    start_byte: u64,
    read_end_byte: u64,
    complete_end_byte: u64,
    end: EndPolicy,
    pending: Vec<u8>,
    lines: Vec<LineEvents>,
}

impl SnapshotReader {
    pub fn new(meta: SourceMeta, start_byte: u64, end: EndPolicy) -> Self {
        Self {
            meta,
            start_byte,
            read_end_byte: start_byte,
            complete_end_byte: start_byte,
            end,
            pending: Vec::new(),
            lines: Vec::new(),
        }
    }

    pub fn push(&mut self, bytes: &[u8], work: &mut dyn Work) -> Result<(), SnapshotError> {
        work.check()?;
        let count = u64::try_from(bytes.len()).map_err(|_| SnapshotError::OffsetOverflow)?;
        let read_end = self
            .read_end_byte
            .checked_add(count)
            .ok_or(SnapshotError::OffsetOverflow)?;
        let mut rest = bytes;
        while let Some(index) = rest.iter().position(|byte| *byte == b'\n') {
            work.check()?;
            self.pending.extend_from_slice(&rest[..index]);
            let count =
                u64::try_from(self.pending.len()).map_err(|_| SnapshotError::OffsetOverflow)?;
            let end = self
                .complete_end_byte
                .checked_add(count)
                .and_then(|end| end.checked_add(1))
                .ok_or(SnapshotError::OffsetOverflow)?;
            self.line(end, true, work)?;
            self.complete_end_byte = end;
            rest = &rest[index + 1..];
        }
        self.pending.extend_from_slice(rest);
        self.read_end_byte = read_end;
        Ok(())
    }

    fn line(
        &mut self,
        end_byte: u64,
        terminated: bool,
        work: &mut dyn Work,
    ) -> Result<(), SnapshotError> {
        let length = self
            .pending
            .iter()
            .rposition(|byte| *byte != b'\r')
            .map_or(0, |i| i + 1);
        let events = super::normalize::claude_line(&self.pending[..length], work)?;
        self.lines.push(LineEvents {
            start_byte: self.complete_end_byte,
            end_byte,
            terminated,
            events,
        });
        self.pending.clear();
        Ok(())
    }

    pub fn finish(mut self, work: &mut dyn Work) -> Result<Snapshot, SnapshotError> {
        work.check()?;
        if matches!(self.end, EndPolicy::SnapshotEof) && !self.pending.is_empty() {
            self.line(self.read_end_byte, false, work)?;
        }
        let rows = self.project_rows(work)?;
        Ok(Snapshot {
            source_id: self.meta.source_id,
            read_end_byte: self.read_end_byte,
            complete_end_byte: self.complete_end_byte,
            lines: self.lines,
            rows,
        })
    }

    fn project_rows(&self, work: &mut dyn Work) -> Result<RowProjection, SnapshotError> {
        let mut records = Vec::new();
        for line in &self.lines {
            work.check()?;
            for (ordinal, event) in line.events.iter().enumerate() {
                work.check()?;
                let input = event.row_input();
                work.check()?;
                let Some(input) = input else {
                    return Ok(RowProjection::Unavailable {
                        error: RowProjectionError::UnrepresentableNumber,
                    });
                };
                records.push(Record::new(
                    input,
                    format!("raw-{}-{ordinal}", line.start_byte),
                    false,
                ));
            }
        }
        let rows =
            view_rows_with_context(&records, self.start_byte != 0, false, &HashSet::new(), None);
        work.check()?;
        Ok(RowProjection::Rows(rows))
    }
}
