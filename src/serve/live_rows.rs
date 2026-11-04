//! The server uses the shared row owner, without a separate reducer.

#[cfg(test)]
use super::*;

pub(super) use swarm::view::transcript::reader_rows::{ROW_RECORD_LIMIT, ReaderRows as LiveRows};

#[cfg(test)]
#[path = "live_rows/tests.rs"]
mod tests;
