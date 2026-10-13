mod builder;
pub mod choices;
pub mod choices_store;
pub mod discovery;
pub mod fields;
mod fold;
pub mod github;
pub mod navigation;
mod preview;
mod sidebar;
pub mod tabs;
mod tool;
pub mod transcript;
pub mod tree;

pub use fold::{FoldSummary, fold_expanded, fold_rows, summary as fold_summary};
pub use preview::{Diff, DiffHunk, DiffPreview, TextPreview, diff_patch, text_chunks};
pub use tool::{
    ToolActivity, ToolState, command, duration, duration_label, exec_commands, exit_code, path,
    reports_failure, title,
};

fn is_false(value: &bool) -> bool {
    !value
}
