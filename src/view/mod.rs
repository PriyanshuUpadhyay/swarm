mod builder;
mod fold;
mod preview;
mod tool;
pub mod transcript;

pub use fold::{FoldSummary, fold_expanded, fold_rows, summary as fold_summary};
pub use preview::{Diff, DiffHunk, DiffPreview, TextPreview, diff_patch, text_chunks};
pub use tool::{
    ToolActivity, ToolState, command, duration, duration_label, exec_commands, exit_code, path,
    reports_failure, title,
};

fn is_false(value: &bool) -> bool {
    !value
}
