//! A physical Codex line is parsed once and projected with each reader's context.

pub(crate) use super::codex_context::ReaderContext;
pub(crate) use super::codex_facts::Fact;
use super::{SnapshotError, Work};

pub(crate) fn parse_fact<'a>(
    line: &'a [u8],
    work: &mut dyn Work,
) -> Result<Fact<'a>, SnapshotError> {
    super::codex_facts::parse(line, work)
}
