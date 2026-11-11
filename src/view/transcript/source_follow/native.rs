//! Native complete-line follow reuses the domain framer and provider context.
use super::super::source::{NativeContext, NativeSnapshot, normalize_line};
use super::*;

#[derive(Debug)]
pub struct NativeStream {
    file: File,
    framer: PhysicalFramer,
    context: NativeContext,
}
impl NativeStream {
    pub fn new(file: File, from: u64, format: NativeFormat, path: &str) -> Self {
        Self {
            file,
            framer: PhysicalFramer::new(from),
            context: NativeContext::new(format, &format.session_id_from_path(path)),
        }
    }
    pub fn advance(
        &mut self,
        source_id: &str,
        work: &mut dyn Work,
    ) -> Result<NativeSnapshot, SourceError> {
        let end = file::length(&self.file, work)?;
        let start = self.framer.read_end_byte();
        let mut lines = Vec::new();
        if end > start {
            file::read_chunks(&self.file, start, end, work, |bytes, work| {
                self.framer.push(bytes, work, |line, work| {
                    lines.push(normalize_line(line, &mut self.context, work)?);
                    Ok(())
                })
            })?;
        }
        Ok(NativeSnapshot {
            source_id: source_id.into(),
            read_end_byte: self.framer.read_end_byte(),
            complete_end_byte: self.framer.complete_end_byte(),
            lines,
        })
    }
}
