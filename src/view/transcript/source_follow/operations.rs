//! Explicit source operations share framing and do not reopen the source path.

use super::super::reader_rows::ReaderRows;
use super::super::source::{
    EndPolicy, NativeContext, Snapshot, SnapshotReader, SourceMeta, normalize_line,
};
use super::super::source_readers::{ReaderBatch, ReaderState, prepare_records};
use super::super::worker_protocol::ReaderBinding;
use super::file::{boundary, length, physical_start, read_chunks};
use super::*;
use std::collections::BTreeMap;

impl SourceFollow {
    pub fn snapshot(
        &mut self,
        meta: SourceMeta,
        start: u64,
        end: Option<u64>,
        policy: EndPolicy,
        work: &mut dyn Work,
    ) -> Result<Snapshot, SourceError> {
        let length = length(&self.file, work)?;
        let end = end.unwrap_or(length);
        if start > end || end > length || !boundary(&self.file, start, work)? {
            return Err(SourceError::InvalidRange);
        }
        let mut reader =
            SnapshotReader::with_session_id(meta, start, policy, self.format, &self.session_id);
        read_chunks(&self.file, start, end, work, |bytes, work| {
            reader.push(bytes, work).map_err(Into::into)
        })?;
        Ok(reader.finish(work)?)
    }

    pub fn bind_reader(
        &mut self,
        binding: ReaderBinding,
        from: u64,
        work: &mut dyn Work,
    ) -> Result<ReaderBatch, SourceError> {
        self.readers.check_bind(binding)?;
        let end = length(&self.file, work)?;
        let origin = from.min(end);
        let start = physical_start(&self.file, origin, work)?;
        let mut framer = PhysicalFramer::new(start);
        let mut state = ReaderState::new(binding, origin, self.format, &self.session_id);
        let mut lines = Vec::new();
        read_chunks(&self.file, start, end, work, |bytes, work| {
            framer.push(bytes, work, |line, work| {
                if let Some(native) = state.initial_line(line, work)? {
                    lines.push(native);
                }
                Ok(())
            })
        })?;
        Ok(self.readers.bind(state, end, lines, work)?)
    }

    pub fn replay_reader(
        &mut self,
        binding: ReaderBinding,
        start: u64,
        end: u64,
        work: &mut dyn Work,
    ) -> Result<ReaderBatch, SourceError> {
        self.readers.check_reader(binding)?;
        let length = length(&self.file, work)?;
        if start > end
            || end > length
            || !boundary(&self.file, start, work)?
            || !boundary(&self.file, end, work)?
        {
            return Err(SourceError::InvalidRange);
        }
        let mut framer = PhysicalFramer::new(start);
        let mut context = NativeContext::new(self.format, &self.session_id);
        let mut page = ReaderRows::empty(start, self.format.is_codex());
        let mut lines = Vec::new();
        read_chunks(&self.file, start, end, work, |bytes, work| {
            framer.push(bytes, work, |line, work| {
                let native = normalize_line(line, &mut context, work)?;
                let facts = prepare_records(&native, self.format, work)?;
                page.ingest_prepared(line.start_byte, line.end_byte, &facts);
                lines.push(native);
                Ok(())
            })
        })?;
        self.readers.replay(binding, page, end, lines, work)
    }

    pub fn advance(&mut self, work: &mut dyn Work) -> Result<ReaderBatch, SourceError> {
        let end = length(&self.file, work)?;
        let start = self.framer.read_end_byte();
        let mut lines = BTreeMap::new();
        let file = &self.file;
        let framer = &mut self.framer;
        let readers = &mut self.readers;
        // A shrink keeps the established old offset and pending slice, as OLD follow does.
        if end > start {
            read_chunks(&self.file, start, end, work, |bytes, work| {
                framer.push(bytes, work, |line, work| {
                    let whole = physical_start(file, line.start_byte, work)? == line.start_byte;
                    readers.live_line(line, whole, &mut lines, work)?;
                    Ok(())
                })
            })?;
        }
        Ok(readers.live_update(framer.read_end_byte(), lines, work)?)
    }

    pub fn close_reader(
        &mut self,
        binding: ReaderBinding,
        work: &mut dyn Work,
    ) -> Result<ReaderBatch, SourceError> {
        work.check()?;
        self.readers.close(binding)
    }
}
