mod builder;
pub mod changes;
pub mod choices;
pub mod choices_store;
mod claude_properties;
pub mod claude_text;
pub mod discovery;
pub mod fields;
mod fold;
pub mod github;
pub mod handoff;
pub mod lifecycle;
pub mod navigation;
mod preview;
pub mod queue;
mod sidebar;
pub mod tabs;
mod tool;
pub mod transcript;
pub mod tree;
pub mod workspace_panel;

pub use fold::{
    FOLD_STEP_LIMIT, FoldSummary, fold_expanded, fold_rows, is_step, summary as fold_summary,
};
pub use preview::{Diff, DiffHunk, DiffPreview, TextPreview, diff_patch, text_chunks};
pub use tool::{
    ToolActivity, ToolState, command, duration, duration_label, exec_commands, exit_code, path,
    reports_failure, title,
};

fn is_false(value: &bool) -> bool {
    !value
}
