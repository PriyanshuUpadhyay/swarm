//! The existing server boundary resolves physical positions without a second Row reducer.

use super::super::reader_rows::ReaderRows;
use super::*;
use serde_json::Value;

struct Unbounded;
impl Work for Unbounded {
    fn check(&mut self) -> Result<(), SnapshotError> {
        Ok(())
    }
}

pub fn ingest_log_line(
    state: &mut ReaderRows,
    source: &File,
    start: u64,
    end: u64,
    events: &[Value],
) {
    if events.is_empty() {
        state.ingest(start, end, events);
        return;
    }
    let mut work = Unbounded;
    let line_start = (|| {
        if start > 0 && file::boundary(source, start, &mut work)? {
            return Ok(start);
        }
        if end == 0 || !file::boundary(source, end, &mut work)? {
            return Err(SourceError::InvalidRange);
        }
        file::physical_start(source, end - 1, &mut work)
    })();
    match line_start {
        Ok(line_start) if start > line_start => state.ingest(start, end, &[]),
        Ok(line_start) => state.ingest(line_start, end, events),
        Err(_) => state.ingest(start, end, events),
    }
}
