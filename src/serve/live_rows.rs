//! The server holds a source lease beside the shared row owner.
use super::*;
use std::ops::{Deref, DerefMut};
pub(super) use swarm::view::transcript::reader_rows::ROW_RECORD_LIMIT;
use swarm::view::transcript::reader_rows::{ReaderRows, RowPage};

#[derive(Clone)]
pub(super) struct LiveRows {
    rows: ReaderRows,
    pub(super) source: Option<Arc<source_backend::Lease>>,
}
impl LiveRows {
    pub(super) fn empty(from: u64, is_codex: bool) -> Self {
        Self {
            rows: ReaderRows::empty(from, is_codex),
            source: None,
        }
    }
    pub(super) fn merge_page(&mut self, page: Self, before: bool) -> RowPage {
        if self.source.is_none() {
            self.source = page.source;
        }
        self.rows.merge_page(page.rows, before)
    }
    pub(super) fn recover(&mut self, page: Self) {
        self.rows.recover(page.rows);
    }
    pub(super) fn merge_incoming(&mut self, page: Self) {
        self.rows.merge_incoming(page.rows);
    }
}
impl Deref for LiveRows {
    type Target = ReaderRows;
    fn deref(&self) -> &ReaderRows {
        &self.rows
    }
}
impl DerefMut for LiveRows {
    fn deref_mut(&mut self) -> &mut ReaderRows {
        &mut self.rows
    }
}

#[cfg(test)]
#[path = "live_rows/tests.rs"]
mod tests;
