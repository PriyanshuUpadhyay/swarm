//! Reader epochs and retained provider views contain no file handles or paths.

use super::Record;
use super::reader_rows::ReaderRows;
use super::source::{
    LineEvents, NativeContext, NativeFacts, NativeFormat, RowProjection, SnapshotError, Work,
};
use super::source_follow::{PhysicalLine, SourceError};
use super::worker_protocol::ReaderBinding;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct ReaderUpdate {
    pub reader: ReaderBinding,
    pub origin_byte: u64,
    pub start_byte: u64,
    pub read_end_byte: u64,
    pub complete_end_byte: u64,
    pub next_complete_byte: u64,
    pub lines: Vec<LineEvents>,
    pub rows: RowProjection,
}

#[derive(Debug, Serialize)]
pub struct ReaderBatch {
    pub updates: Vec<ReaderUpdate>,
}

#[derive(Debug)]
pub(crate) struct ReaderState {
    binding: ReaderBinding,
    origin_byte: u64,
    next_complete_byte: u64,
    observed_read_end_byte: u64,
    rows: ReaderRows,
    context: NativeContext,
}

#[derive(Debug, Default)]
pub(crate) struct ReaderSet {
    format: NativeFormat,
    readers: BTreeMap<u64, ReaderState>,
    last_generation: BTreeMap<u64, u64>,
}

impl ReaderState {
    pub fn new(
        binding: ReaderBinding,
        origin_byte: u64,
        format: NativeFormat,
        session_id: &str,
    ) -> Self {
        Self {
            binding,
            origin_byte,
            next_complete_byte: origin_byte,
            observed_read_end_byte: origin_byte,
            rows: ReaderRows::empty(origin_byte, format.is_codex()),
            context: NativeContext::new(format, session_id),
        }
    }

    fn accept(
        &mut self,
        line: PhysicalLine<'_>,
        facts: &NativeFacts<'_>,
        whole: bool,
        work: &mut dyn Work,
    ) -> Result<Option<LineEvents>, SnapshotError> {
        work.check()?;
        if line.end_byte <= self.next_complete_byte {
            return Ok(None);
        }
        let start = line.start_byte.max(self.next_complete_byte);
        let events = if start == line.start_byte {
            let native = LineEvents {
                start_byte: start,
                end_byte: line.end_byte,
                terminated: line.terminated,
                events: self.context.project(facts, work)?,
            };
            if whole {
                let records = prepare_records(&native, self.context.format(), work)?;
                self.rows.ingest_prepared(start, line.end_byte, &records);
            } else {
                self.rows.ingest(start, line.end_byte, &[]);
            }
            native
        } else {
            self.rows.ingest(start, line.end_byte, &[]);
            let suffix = self
                .context
                .format()
                .parse_facts(line.content_from(start), work)?;
            LineEvents {
                start_byte: start,
                end_byte: line.end_byte,
                terminated: true,
                events: self.context.project(&suffix, work)?,
            }
        };
        self.next_complete_byte = line.end_byte;
        work.check()?;
        Ok(Some(events))
    }

    pub fn initial_line(
        &mut self,
        line: PhysicalLine<'_>,
        work: &mut dyn Work,
    ) -> Result<Option<LineEvents>, SnapshotError> {
        if line.end_byte <= self.next_complete_byte {
            return Ok(None);
        }
        let facts = self
            .context
            .format()
            .parse_facts(line.content_from(line.start_byte), work)?;
        self.accept(line, &facts, true, work)
    }

    fn update(
        &mut self,
        read_end_byte: u64,
        complete_end_byte: u64,
        lines: Vec<LineEvents>,
        work: &mut dyn Work,
    ) -> Result<ReaderUpdate, SnapshotError> {
        work.check()?;
        self.rows.delta();
        work.check()?;
        let rows = self.rows.projection();
        work.check()?;
        Ok(ReaderUpdate {
            reader: self.binding,
            origin_byte: self.origin_byte,
            start_byte: self.rows.start,
            read_end_byte,
            complete_end_byte,
            next_complete_byte: self.next_complete_byte,
            lines,
            rows,
        })
    }
}

impl ReaderSet {
    pub fn new(format: NativeFormat) -> Self {
        Self {
            format,
            ..Self::default()
        }
    }

    pub fn check_bind(&self, binding: ReaderBinding) -> Result<(), SourceError> {
        if self
            .last_generation
            .get(&binding.reader_id)
            .is_some_and(|generation| binding.generation <= *generation)
        {
            return Err(SourceError::BindingMismatch);
        }
        Ok(())
    }

    pub fn check_reader(&self, binding: ReaderBinding) -> Result<(), SourceError> {
        self.get(binding).map(|_| ())
    }

    fn get(&self, binding: ReaderBinding) -> Result<&ReaderState, SourceError> {
        self.readers
            .get(&binding.reader_id)
            .filter(|state| state.binding == binding)
            .ok_or(SourceError::BindingMismatch)
    }

    pub fn bind(
        &mut self,
        mut state: ReaderState,
        end: u64,
        lines: Vec<LineEvents>,
        work: &mut dyn Work,
    ) -> Result<ReaderBatch, SnapshotError> {
        state.observed_read_end_byte = end;
        let update = state.update(end, state.next_complete_byte, lines, work)?;
        self.last_generation
            .insert(state.binding.reader_id, state.binding.generation);
        self.readers.insert(state.binding.reader_id, state);
        Ok(ReaderBatch {
            updates: vec![update],
        })
    }

    pub fn live_line(
        &mut self,
        line: PhysicalLine<'_>,
        whole: bool,
        lines: &mut BTreeMap<u64, Vec<LineEvents>>,
        work: &mut dyn Work,
    ) -> Result<(), SnapshotError> {
        if !self
            .readers
            .values()
            .any(|state| line.end_byte > state.next_complete_byte)
        {
            return Ok(());
        }
        let facts = self
            .format
            .parse_facts(line.content_from(line.start_byte), work)?;
        for (id, state) in &mut self.readers {
            if let Some(event) = state.accept(line, &facts, whole, work)? {
                lines.entry(*id).or_default().push(event);
            }
        }
        Ok(())
    }

    pub fn live_update(
        &mut self,
        end: u64,
        mut lines: BTreeMap<u64, Vec<LineEvents>>,
        work: &mut dyn Work,
    ) -> Result<ReaderBatch, SnapshotError> {
        let mut updates = Vec::new();
        for (id, state) in &mut self.readers {
            state.observed_read_end_byte = state.observed_read_end_byte.max(end);
            updates.push(state.update(
                state.observed_read_end_byte,
                state.next_complete_byte,
                lines.remove(id).unwrap_or_default(),
                work,
            )?);
        }
        Ok(ReaderBatch { updates })
    }

    pub fn replay(
        &mut self,
        binding: ReaderBinding,
        page: ReaderRows,
        end: u64,
        lines: Vec<LineEvents>,
        work: &mut dyn Work,
    ) -> Result<ReaderBatch, SourceError> {
        self.check_reader(binding)?;
        let state = self
            .readers
            .get_mut(&binding.reader_id)
            .ok_or(SourceError::BindingMismatch)?;
        work.check()?;
        state.rows.merge_page(page, true);
        let update = state.update(end, end, lines, work)?;
        Ok(ReaderBatch {
            updates: vec![update],
        })
    }

    pub fn close(&mut self, binding: ReaderBinding) -> Result<ReaderBatch, SourceError> {
        self.check_reader(binding)?;
        self.readers.remove(&binding.reader_id);
        Ok(ReaderBatch {
            updates: Vec::new(),
        })
    }
}

pub(crate) fn prepare_records(
    line: &LineEvents,
    format: NativeFormat,
    work: &mut dyn Work,
) -> Result<Vec<Option<Record>>, SnapshotError> {
    let mut facts = Vec::with_capacity(line.events.len());
    for (ordinal, event) in line.events.iter().enumerate() {
        work.check()?;
        facts.push(event.row_input().map(|input| {
            Record::new(
                input,
                format!("raw-{}-{ordinal}", line.start_byte),
                format.is_codex(),
            )
        }));
        work.check()?;
    }
    Ok(facts)
}
