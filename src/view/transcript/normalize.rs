//! Provider normalization; file and view policy stay with their owners.

pub(super) mod agy;
mod claude;
mod claude_input;
mod claude_json;
mod claude_meta;
mod claude_tools;
pub(super) mod codex;
mod codex_changes;
mod codex_context;
mod codex_facts;
mod codex_questions;
mod codex_tools;
pub(super) mod pi;
mod pi_text;

use super::source::{NormalizedEvent, SnapshotError, Work};

pub(crate) fn claude_line(
    line: &[u8],
    work: &mut dyn Work,
) -> Result<Vec<NormalizedEvent>, SnapshotError> {
    claude::line(line, work)
}
