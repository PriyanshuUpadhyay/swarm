//! Inert destination policy trial over the maintained pinned parser.

mod normalize_link;

/// Parse with CommonMark constructs and the two OLD href policy hooks, replacing NUL with U+FFFD first.
///
/// # Errors
///
/// Returns the parser's original diagnostic if parsing fails.
pub fn parse(source: &str) -> Result<markdown::mdast::Node, markdown::message::Message> {
    // Each NUL adds 2 to later byte offsets; later columns on its line can move too (tabs snap to 4-column stops).
    let source = if source.contains('\0') {
        std::borrow::Cow::Owned(source.replace('\0', "\u{fffd}"))
    } else {
        std::borrow::Cow::Borrowed(source)
    };
    markdown::to_mdast(
        &source,
        &markdown::ParseOptions {
            normalize_link: Some(normalize_link::normalize),
            validate_link: Some(normalize_link::validate),
            ..markdown::ParseOptions::default()
        },
    )
}
