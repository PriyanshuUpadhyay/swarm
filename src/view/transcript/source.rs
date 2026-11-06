//! Pure, independent provider snapshots with physical source positions.

use super::source_follow::{PhysicalFramer, PhysicalLine};
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

#[derive(Clone, Debug, Serialize)]
pub struct LineEvents {
    pub start_byte: u64,
    pub end_byte: u64,
    pub terminated: bool,
    pub events: Vec<NormalizedEvent>,
}

/// A provider event whose copied input keeps OLD's exact numeric tokens.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct NormalizedEvent(Box<RawValue>);

impl NormalizedEvent {
    pub(crate) fn new(value: &impl Serialize) -> Self {
        // Callers construct only JSON fields and validated raw numeric tokens, with no I/O.
        Self(serde_json::value::to_raw_value(value).expect("normalized event is valid JSON"))
    }

    pub(crate) fn row_input(&self) -> Option<Value> {
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

#[derive(Debug, Serialize)]
pub struct NativeSnapshot {
    pub source_id: String,
    pub read_end_byte: u64,
    pub complete_end_byte: u64,
    pub lines: Vec<LineEvents>,
}

#[derive(Debug)]
pub enum SnapshotError {
    OffsetOverflow,
    Deadline,
    NativeTokenOverflow,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OffsetOverflow => "snapshot byte offset overflow",
            Self::Deadline => "snapshot work deadline reached",
            Self::NativeTokenOverflow => "native token count overflow",
        })
    }
}

impl std::error::Error for SnapshotError {}

pub trait Work {
    fn check(&mut self) -> Result<(), SnapshotError>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeFormat {
    #[default]
    Claude,
    Codex,
    Agy,
    Pi,
}

impl NativeFormat {
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Agy, Self::Pi];

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Agy => "agy",
            Self::Pi => "pi",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.name() == name)
    }

    pub(crate) fn session_id_from_path(self, path: &str) -> String {
        match self {
            Self::Agy => super::normalize::agy::session_id_from_path(path),
            Self::Pi => super::normalize::pi::session_id_from_path(path),
            Self::Claude | Self::Codex => String::new(),
        }
    }

    pub(crate) fn is_codex(self) -> bool {
        self == Self::Codex
    }

    pub(crate) fn parse_facts<'a>(
        self,
        line: &'a [u8],
        work: &mut dyn Work,
    ) -> Result<NativeFacts<'a>, SnapshotError> {
        match self {
            Self::Claude => super::normalize::claude_line(line, work).map(NativeFacts::Claude),
            Self::Codex => super::normalize::codex::parse_fact(line, work).map(NativeFacts::Codex),
            Self::Agy => super::normalize::agy::parse_fact(line, work).map(NativeFacts::Agy),
            Self::Pi => super::normalize::pi::parse_fact(line, work).map(NativeFacts::Pi),
        }
    }
}

#[derive(Debug)]
pub(crate) enum NativeFacts<'a> {
    Claude(Vec<NormalizedEvent>),
    Codex(super::normalize::codex::Fact<'a>),
    Agy(super::normalize::agy::Fact),
    Pi(super::normalize::pi::Fact),
}

#[derive(Debug)]
pub(crate) enum NativeContext {
    Claude,
    Codex(super::normalize::codex::ReaderContext),
    Agy(String),
    Pi(String),
}

impl NativeContext {
    pub(crate) fn new(format: NativeFormat, session_id: &str) -> Self {
        match format {
            NativeFormat::Claude => Self::Claude,
            NativeFormat::Codex => Self::Codex(super::normalize::codex::ReaderContext::new()),
            NativeFormat::Agy => Self::Agy(session_id.to_owned()),
            NativeFormat::Pi => Self::Pi(session_id.to_owned()),
        }
    }

    pub(crate) fn format(&self) -> NativeFormat {
        match self {
            Self::Claude => NativeFormat::Claude,
            Self::Codex(_) => NativeFormat::Codex,
            Self::Agy(_) => NativeFormat::Agy,
            Self::Pi(_) => NativeFormat::Pi,
        }
    }

    pub(crate) fn project(
        &mut self,
        facts: &NativeFacts<'_>,
        work: &mut dyn Work,
    ) -> Result<Vec<NormalizedEvent>, SnapshotError> {
        work.check()?;
        match (self, facts) {
            (Self::Claude, NativeFacts::Claude(events)) => Ok(events.clone()),
            (Self::Codex(context), NativeFacts::Codex(fact)) => context.project(fact, work),
            (Self::Agy(session_id), NativeFacts::Agy(fact)) => fact.project(session_id, work),
            (Self::Pi(session_id), NativeFacts::Pi(fact)) => fact.project(session_id, work),
            _ => unreachable!("reader context and facts belong to the same bound format"),
        }
    }
}

#[derive(Debug)]
pub struct SnapshotReader {
    meta: SourceMeta,
    start_byte: u64,
    framer: PhysicalFramer,
    end: EndPolicy,
    lines: Vec<LineEvents>,
    context: NativeContext,
}

impl SnapshotReader {
    pub fn with_open_path(
        meta: SourceMeta,
        start_byte: u64,
        end: EndPolicy,
        format: NativeFormat,
        original_open_path: &str,
    ) -> Self {
        Self::with_session_id(
            meta,
            start_byte,
            end,
            format,
            &format.session_id_from_path(original_open_path),
        )
    }

    pub fn new(meta: SourceMeta, start_byte: u64, end: EndPolicy) -> Self {
        Self::with_format(meta, start_byte, end, NativeFormat::Claude)
    }

    pub(crate) fn with_format(
        meta: SourceMeta,
        start_byte: u64,
        end: EndPolicy,
        format: NativeFormat,
    ) -> Self {
        Self::with_session_id(meta, start_byte, end, format, "")
    }

    pub(crate) fn with_session_id(
        meta: SourceMeta,
        start_byte: u64,
        end: EndPolicy,
        format: NativeFormat,
        session_id: &str,
    ) -> Self {
        Self {
            meta,
            start_byte,
            framer: PhysicalFramer::new(start_byte),
            end,
            lines: Vec::new(),
            context: NativeContext::new(format, session_id),
        }
    }

    pub fn push(&mut self, bytes: &[u8], work: &mut dyn Work) -> Result<(), SnapshotError> {
        self.framer.push(bytes, work, |line, work| {
            self.lines
                .push(normalize_line(line, &mut self.context, work)?);
            Ok(())
        })
    }

    fn finish_eof(&mut self, work: &mut dyn Work) -> Result<(), SnapshotError> {
        work.check()?;
        if matches!(self.end, EndPolicy::SnapshotEof) {
            self.framer.eof(work, |line, work| {
                self.lines
                    .push(normalize_line(line, &mut self.context, work)?);
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn finish_native(mut self, work: &mut dyn Work) -> Result<NativeSnapshot, SnapshotError> {
        self.finish_eof(work)?;
        Ok(NativeSnapshot {
            source_id: self.meta.source_id,
            read_end_byte: self.framer.read_end_byte(),
            complete_end_byte: self.framer.complete_end_byte(),
            lines: self.lines,
        })
    }

    pub fn finish(mut self, work: &mut dyn Work) -> Result<Snapshot, SnapshotError> {
        self.finish_eof(work)?;
        let rows = self.project_rows(work)?;
        Ok(Snapshot {
            source_id: self.meta.source_id,
            read_end_byte: self.framer.read_end_byte(),
            complete_end_byte: self.framer.complete_end_byte(),
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
                    self.context.format().is_codex(),
                ));
            }
        }
        let rows = view_rows_with_context(
            &records,
            self.start_byte != 0,
            self.context.format().is_codex(),
            &HashSet::new(),
            None,
        );
        work.check()?;
        Ok(RowProjection::Rows(rows))
    }
}

pub(crate) fn normalize_line(
    line: PhysicalLine<'_>,
    context: &mut NativeContext,
    work: &mut dyn Work,
) -> Result<LineEvents, SnapshotError> {
    let facts = context
        .format()
        .parse_facts(line.content_from(line.start_byte), work)?;
    Ok(LineEvents {
        start_byte: line.start_byte,
        end_byte: line.end_byte,
        terminated: line.terminated,
        events: context.project(&facts, work)?,
    })
}
