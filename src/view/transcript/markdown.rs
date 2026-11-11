//! App Markdown producer over the maintained pinned parser.

mod diff_fence;
mod normalize_link;

pub mod display;

/// Build display entries for Markdown rows and their steps in painter order.
/// Later entries replace earlier entries with the same row id.
pub fn row_displays(rows: &[super::Row]) -> std::collections::BTreeMap<String, display::Display> {
    use super::RowKind;

    let mut displays = std::collections::BTreeMap::new();
    let mut pending: Vec<_> = rows.iter().rev().collect();
    while let Some(row) = pending.pop() {
        pending.extend(row.steps.iter().rev());
        let system_kind = row.system_kind.as_deref();
        let tree = if row.kind == RowKind::Fold
            || row.tool.is_some()
            || row.kind == RowKind::Divider
            || matches!(system_kind, Some("working" | "swarm_ring"))
        {
            None
        } else if matches!(system_kind, Some("compaction" | "compact_summary")) {
            row.text.contains('\n').then(|| display::display(&row.text))
        } else if row.shell.is_some() || row.command.is_some() {
            None
        } else if matches!(row.kind, RowKind::User | RowKind::Queued) {
            Some(display::display_with_breaks(&row.text))
        } else if row.ends_turn {
            None
        } else if matches!(row.kind, RowKind::Assistant | RowKind::ProposedPlan) {
            Some(display::display(&row.text))
        } else {
            None
        };
        if let Some(Ok(tree)) = tree {
            displays.insert(row.id.clone(), tree);
        }
    }
    displays
}

/// Pair each row and step with the painter for its already parsed Display.
pub fn row_bodies(
    rows: &[super::Row],
    displays: &std::collections::BTreeMap<String, display::Display>,
) -> std::collections::BTreeMap<String, display::BodyMode> {
    let mut bodies = std::collections::BTreeMap::new();
    let mut pending: Vec<_> = rows.iter().rev().collect();
    while let Some(row) = pending.pop() {
        pending.extend(row.steps.iter().rev());
        bodies.insert(row.id.clone(), display::body_mode(displays.get(&row.id)));
    }
    bodies
}

fn prelude(source: &str) -> std::borrow::Cow<'_, str> {
    // Each CRLF makes later byte offsets 1 smaller; each NUL adds 2.
    // Later columns on a NUL's line can move too (tabs snap to 4-column stops).
    if source.contains('\r') || source.contains('\0') {
        std::borrow::Cow::Owned(
            source
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace('\0', "\u{fffd}"),
        )
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

/// Parse CommonMark with tables, strikethrough and autolinks, raw HTML and images off, href hooks and NUL as U+FFFD.
///
/// # Errors
///
/// Returns the parser's original diagnostic if parsing fails.
pub fn parse(source: &str) -> Result<markdown::mdast::Node, markdown::message::Message> {
    let source = prelude(source);
    markdown::to_mdast(
        &source,
        &markdown::ParseOptions {
            constructs: markdown::Constructs {
                gfm_autolink_literal: true,
                gfm_footnote_definition: false,
                gfm_label_start_footnote: false,
                gfm_strikethrough: true,
                gfm_table: true,
                html_flow: false,
                html_text: false,
                label_start_image: false,
                ..markdown::Constructs::default()
            },
            gfm_strikethrough_single_tilde: false,
            normalize_link: Some(normalize_link::normalize),
            validate_link: Some(normalize_link::validate),
            ..markdown::ParseOptions::default()
        },
    )
}
