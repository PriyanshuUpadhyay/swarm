//! Inert destination policy trial over the maintained pinned parser.

mod normalize_link;

pub mod display;

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
