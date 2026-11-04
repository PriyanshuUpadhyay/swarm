//! Claude provider normalization; file and view policy stay with their owners.

mod claude;
mod claude_input;
mod claude_json;
mod claude_meta;
mod claude_tools;

use super::source::{NormalizedEvent, SnapshotError, Work};

pub(crate) fn claude_line(
    line: &[u8],
    work: &mut dyn Work,
) -> Result<Vec<NormalizedEvent>, SnapshotError> {
    claude::line(line, work)
}
