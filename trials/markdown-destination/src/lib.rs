//! Inert destination policy trial over the maintained pinned parser.

mod normalize_link;

/// Parse with CommonMark constructs and the two OLD href policy hooks.
///
/// # Errors
///
/// Returns the parser's original diagnostic if parsing fails.
pub fn parse(source: &str) -> Result<markdown::mdast::Node, markdown::message::Message> {
    markdown::to_mdast(
        source,
        &markdown::ParseOptions {
            normalize_link: Some(normalize_link::normalize),
            validate_link: Some(normalize_link::validate),
            ..markdown::ParseOptions::default()
        },
    )
}
