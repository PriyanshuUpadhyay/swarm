//! Reader epochs and retained Claude views contain no file handles or paths.

use super::Record;
use super::reader_rows::ReaderRows;
use super::source::{LineEvents, RowProjection, SnapshotError, Work, normalize_line};
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
}

#[derive(Debug, Default)]
pub(crate) struct ReaderSet {
    readers: BTreeMap<u64, ReaderState>,
    last_generation: BTreeMap<u64, u64>,
}

impl ReaderState {
    pub fn new(binding: ReaderBinding, origin_byte: u64) -> Self {
        Self {
            binding,
            origin_byte,
            next_complete_byte: origin_byte,
            observed_read_end_byte: origin_byte,
            rows: ReaderRows::empty(origin_byte, false),
        }
    }

    fn accept(
        &mut self,
        line: PhysicalLine<'_>,
        native: &LineEvents,
        facts: &[Option<Record>],
        whole: bool,
        work: &mut dyn Work,
    ) -> Result<Option<LineEvents>, SnapshotError> {
        work.check()?;
        if line.end_byte <= self.next_complete_byte {
            return Ok(None);
        }
        let start = line.start_byte.max(self.next_complete_byte);
        let events = if start == line.start_byte {
            if whole {
                self.rows.ingest_prepared(start, line.end_byte, facts);
            } else {
                self.rows.ingest(start, line.end_byte, &[]);
            }
            native.clone()
        } else {
            self.rows.ingest(start, line.end_byte, &[]);
            LineEvents {
                start_byte: start,
                end_byte: line.end_byte,
                terminated: true,
                events: super::normalize::claude_line(line.content_from(start), work)?,
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
        let native = normalize_line(line, work)?;
        let facts = prepare_records(&native, work)?;
        self.accept(line, &native, &facts, true, work)
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
        let native = normalize_line(line, work)?;
        let facts = prepare_records(&native, work)?;
        for (id, state) in &mut self.readers {
            if let Some(event) = state.accept(line, &native, &facts, whole, work)? {
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
    work: &mut dyn Work,
) -> Result<Vec<Option<Record>>, SnapshotError> {
    let mut facts = Vec::with_capacity(line.events.len());
    for (ordinal, event) in line.events.iter().enumerate() {
        work.check()?;
        facts.push(
            event.row_input().map(|input| {
                Record::new(input, format!("raw-{}-{ordinal}", line.start_byte), false)
            }),
        );
        work.check()?;
    }
    Ok(facts)
}
