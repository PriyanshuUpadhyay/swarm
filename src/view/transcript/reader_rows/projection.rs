//! Prepared records retain unavailable inputs at their physical positions.

use super::super::source::{RowProjection, RowProjectionError};
use super::*;

impl ReaderRows {
    pub(crate) fn ingest_prepared(&mut self, start: u64, end: u64, facts: &[Option<Record>]) {
        self.start = self.start.min(start);
        self.end = self.end.max(end);
        for (ordinal, record) in facts.iter().enumerate() {
            let position = (start, ordinal);
            if self.records.contains_key(&position) {
                continue;
            }
            let record = match record {
                Some(record) => record.clone(),
                None => {
                    self.unavailable.insert(position);
                    // A positional sentinel participates in the same chunk/record retention.
                    // Its event is never exposed as a successful native or row projection.
                    Record::new(Value::Null, format!("raw-{start}-{ordinal}"), self.is_codex)
                }
            };
            self.insert_record(position, record);
        }
    }

    pub(crate) fn projection(&self) -> RowProjection {
        if self.unavailable.is_empty() {
            RowProjection::Rows(self.rows())
        } else {
            RowProjection::Unavailable {
                error: RowProjectionError::UnrepresentableNumber,
            }
        }
    }
}
