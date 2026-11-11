//! All range reads use the file opened by the worker's Open request.

use super::*;
use std::io;
use std::os::unix::fs::FileExt;

pub(super) fn length(file: &File, work: &mut dyn Work) -> Result<u64, SourceError> {
    work.check()?;
    let length = file.metadata().map_err(|_| SourceError::Io)?.len();
    work.check()?;
    Ok(length)
}

pub(super) fn read_chunks(
    file: &File,
    start: u64,
    end: u64,
    work: &mut dyn Work,
    mut consume: impl FnMut(&[u8], &mut dyn Work) -> Result<(), SourceError>,
) -> Result<(), SourceError> {
    work.check()?;
    let mut remaining = end.saturating_sub(start);
    let mut offset = start;
    let mut chunk = [0; super::super::worker_protocol::MAX_PART_BYTES];
    while remaining != 0 {
        work.check()?;
        let count = usize::try_from(remaining.min(chunk.len() as u64))
            .map_err(|_| SnapshotError::OffsetOverflow)?;
        file.read_exact_at(&mut chunk[..count], offset)
            .map_err(read_error)?;
        work.check()?;
        consume(&chunk[..count], work)?;
        remaining -= count as u64;
        offset = offset
            .checked_add(count as u64)
            .ok_or(SnapshotError::OffsetOverflow)?;
    }
    work.check()?;
    Ok(())
}

fn read_error(error: io::Error) -> SourceError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        SourceError::InputChanged
    } else {
        SourceError::Io
    }
}

pub(super) fn boundary(file: &File, offset: u64, work: &mut dyn Work) -> Result<bool, SourceError> {
    if offset == 0 {
        return Ok(true);
    }
    work.check()?;
    let mut byte = [0];
    file.read_exact_at(&mut byte, offset - 1)
        .map_err(read_error)?;
    work.check()?;
    Ok(byte[0] == b'\n')
}

pub(super) fn physical_start(
    file: &File,
    offset: u64,
    work: &mut dyn Work,
) -> Result<u64, SourceError> {
    if boundary(file, offset, work)? {
        return Ok(offset);
    }
    let mut cursor = offset;
    let mut buffer = [0; super::super::worker_protocol::MAX_PART_BYTES];
    while cursor != 0 {
        work.check()?;
        let count = usize::try_from(cursor.min(buffer.len() as u64))
            .map_err(|_| SnapshotError::OffsetOverflow)?;
        cursor -= count as u64;
        file.read_exact_at(&mut buffer[..count], cursor)
            .map_err(read_error)?;
        work.check()?;
        if let Some(index) = buffer[..count].iter().rposition(|byte| *byte == b'\n') {
            return cursor
                .checked_add(index as u64)
                .and_then(|start| start.checked_add(1))
                .ok_or(SnapshotError::OffsetOverflow.into());
        }
    }
    Ok(0)
}
