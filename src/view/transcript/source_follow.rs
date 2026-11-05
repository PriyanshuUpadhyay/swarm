//! Physical framing and the single held-file boundary for transcript readers.

use super::source::{NativeFormat, SnapshotError, Work};
use super::source_readers::ReaderSet;
use std::fs::File;

mod file;
mod legacy;
mod operations;
pub use legacy::ingest_log_line;

#[derive(Debug)]
pub enum SourceError {
    InvalidRange,
    BindingMismatch,
    Io,
    InputChanged,
    Work(SnapshotError),
}

impl From<SnapshotError> for SourceError {
    fn from(error: SnapshotError) -> Self {
        Self::Work(error)
    }
}

#[derive(Debug)]
pub struct SourceFollow {
    format: NativeFormat,
    session_id: String,
    file: File,
    framer: PhysicalFramer,
    readers: ReaderSet,
}

impl SourceFollow {
    pub fn new(file: File) -> Self {
        Self::with_format(file, NativeFormat::Claude)
    }

    pub fn with_format(file: File, format: NativeFormat) -> Self {
        Self::with_open_path(file, format, "")
    }

    pub fn with_open_path(file: File, format: NativeFormat, open_path: &str) -> Self {
        Self {
            format,
            session_id: format.session_id_from_path(open_path),
            file,
            framer: PhysicalFramer::new(0),
            readers: ReaderSet::new(format),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PhysicalLine<'a> {
    pub start_byte: u64,
    pub end_byte: u64,
    pub terminated: bool,
    bytes: &'a [u8],
}

impl PhysicalLine<'_> {
    pub fn content_from(&self, start_byte: u64) -> &[u8] {
        let skip = usize::try_from(start_byte - self.start_byte).unwrap_or(usize::MAX);
        let bytes = self.bytes.get(skip..).unwrap_or_default();
        let length = bytes
            .iter()
            .rposition(|byte| *byte != b'\r')
            .map_or(0, |i| i + 1);
        &bytes[..length]
    }
}

#[derive(Debug)]
pub(crate) struct PhysicalFramer {
    read_end_byte: u64,
    complete_end_byte: u64,
    pending: Vec<u8>,
}

impl PhysicalFramer {
    pub fn new(start_byte: u64) -> Self {
        Self {
            read_end_byte: start_byte,
            complete_end_byte: start_byte,
            pending: Vec::new(),
        }
    }

    pub fn read_end_byte(&self) -> u64 {
        self.read_end_byte
    }
    pub fn complete_end_byte(&self) -> u64 {
        self.complete_end_byte
    }

    pub fn push<E: From<SnapshotError>>(
        &mut self,
        bytes: &[u8],
        work: &mut dyn Work,
        mut emit: impl FnMut(PhysicalLine<'_>, &mut dyn Work) -> Result<(), E>,
    ) -> Result<(), E> {
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
            emit(
                PhysicalLine {
                    start_byte: self.complete_end_byte,
                    end_byte: end,
                    terminated: true,
                    bytes: &self.pending,
                },
                work,
            )?;
            self.pending.clear();
            self.complete_end_byte = end;
            rest = &rest[index + 1..];
        }
        self.pending.extend_from_slice(rest);
        self.read_end_byte = read_end;
        Ok(())
    }

    pub fn eof(
        &self,
        work: &mut dyn Work,
        emit: impl FnOnce(PhysicalLine<'_>, &mut dyn Work) -> Result<(), SnapshotError>,
    ) -> Result<(), SnapshotError> {
        work.check()?;
        if !self.pending.is_empty() {
            emit(
                PhysicalLine {
                    start_byte: self.complete_end_byte,
                    end_byte: self.read_end_byte,
                    terminated: false,
                    bytes: &self.pending,
                },
                work,
            )?;
        }
        Ok(())
    }
}
