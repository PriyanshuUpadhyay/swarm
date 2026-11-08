//! Public-API regression for plain-text URLs in the producer entry (Step 5(g) linkify slice).
//!
//! ORIGINAL (markdown-it with `linkify: true` and links limited to http and https) turns a plain-text
//! `http://` or `https://` URL into a link, in any letter case, and leaves `www.` text, a bare host name,
//! `ftp://`, `xhttps://` and a URL inside inline code as they are. Expected values come from the
//! independent ORIGINAL capture (`lk-original`, rows K1 to K18, `breaks` false) and the contract, never
//! from candidate output. Only public returned values of the first paragraph are compared, in order:
//! Text values, Link urls with their joined link text, and inline code values.
//! Every public outcome of the test is collected before its single final assertion.
//!
//! Escapes: none. No input has a LF or a final LF. Positions are not compared.

use markdown::mdast::Node;

/// One inline piece of the first paragraph, as ORIGINAL paints it.
#[derive(Debug, PartialEq)]
enum Piece {
    /// A run of Text (adjacent Text nodes are joined).
    Text(String),
    /// A link: its destination and the joined Text of its children.
    Link { url: String, label: String },
    /// An inline code value.
    Code(String),
    /// Any other node kind. It never equals an expected piece.
    Other,
}

/// What one input gives.
#[derive(Debug, PartialEq)]
enum Observed {
    /// The parse failed.
    ParseError,
    /// The tree has no Paragraph.
    NoParagraph,
    /// The pieces of the first Paragraph.
    Pieces(Vec<Piece>),
}

/// The joined Text values below one node.
fn label_of(children: &[Node]) -> String {
    children
        .iter()
        .map(|child| match child {
            Node::Text(inner) => inner.value.as_str(),
            _ => "\u{fffd}",
        })
        .collect()
}

/// The first Paragraph in document order.
fn first_paragraph(node: &Node) -> Option<&markdown::mdast::Paragraph> {
    if let Node::Paragraph(paragraph) = node {
        return Some(paragraph);
    }
    node.children()
        .into_iter()
        .flatten()
        .find_map(first_paragraph)
}

/// The pieces of the first Paragraph of one input through the trial entry.
fn observe(source: &str) -> Observed {
    let Ok(tree) = swarm::view::transcript::markdown::parse(source) else {
        return Observed::ParseError;
    };
    let Some(paragraph) = first_paragraph(&tree) else {
        return Observed::NoParagraph;
    };
    let mut pieces: Vec<Piece> = Vec::new();
    for child in &paragraph.children {
        match child {
            Node::Text(inner) => {
                if let Some(Piece::Text(previous)) = pieces.last_mut() {
                    previous.push_str(&inner.value);
                } else {
                    pieces.push(Piece::Text(inner.value.clone()));
                }
            }
            Node::Link(inner) => pieces.push(Piece::Link {
                url: inner.url.clone(),
                label: label_of(&inner.children),
            }),
            Node::InlineCode(inner) => pieces.push(Piece::Code(inner.value.clone())),
            _ => pieces.push(Piece::Other),
        }
    }
    Observed::Pieces(pieces)
}

fn text(value: &str) -> Piece {
    Piece::Text(value.to_owned())
}

fn link(url: &str, label: &str) -> Piece {
    Piece::Link {
        url: url.to_owned(),
        label: label.to_owned(),
    }
}

#[test]
fn plain_urls_link_as_in_the_original() {
    // K1, 24 B: 7365652068747470733a2f2f652e746573742f61206e6f77
    // ORIGINAL: Text `see `, Link `https://e.test/a` (same text), Text ` now`.
    let in_text = "see https://e.test/a now";
    // K4, 21 B: 656e642068747470733a2f2f652e746573742f612e
    // ORIGINAL: Text `end `, Link `https://e.test/a`, Text `.` (the dot stays outside).
    let trailing_dot = "end https://e.test/a.";
    // K5, 18 B: 2868747470733a2f2f652e746573742f6129
    // ORIGINAL: Text `(`, Link `https://e.test/a`, Text `)` (the parentheses stay outside).
    let in_parentheses = "(https://e.test/a)";
    // K6, 21 B: 73656520687474703a2f2f652e74657374206e6f77
    // ORIGINAL: Text `see `, Link `http://e.test` (same text), Text ` now`.
    let http_scheme = "see http://e.test now";
    // K7, 19 B: 736565206578616d706c652e636f6d206e6f77
    // ORIGINAL: one Text, no link.
    let bare_host = "see example.com now";
    // K8, 20 B: 736565206674703a2f2f652e74657374206e6f77
    // ORIGINAL: one Text, no link.
    let ftp_scheme = "see ftp://e.test now";
    // K10, 23 B: 736565207868747470733a2f2f652e74657374206e6f77
    // ORIGINAL: one Text, no link.
    let prefixed_scheme = "see xhttps://e.test now";
    // K11, 42 B: 5b785d2868747470733a2f2f652e746573742f6c2920616e642068747470733a2f2f652e746573742f6d
    // ORIGINAL: Link `https://e.test/l` with text `x`, Text ` and `, Link `https://e.test/m` (same text).
    let link_then_url = "[x](https://e.test/l) and https://e.test/m";
    // K16, 24 B: 7365652048545450533a2f2f452e544553542f41206e6f77
    // ORIGINAL: Text `see `, Link `HTTPS://E.TEST/A` (same text, case kept), Text ` now`.
    let upper_case_scheme = "see HTTPS://E.TEST/A now";
    // K17, 18 B: 6068747470733a2f2f652e746573742f6160
    // ORIGINAL: one inline code `https://e.test/a`, no link.
    let inline_code = "`https://e.test/a`";

    let observed = vec![
        ("in_text", observe(in_text)),
        ("trailing_dot", observe(trailing_dot)),
        ("in_parentheses", observe(in_parentheses)),
        ("http_scheme", observe(http_scheme)),
        ("bare_host", observe(bare_host)),
        ("ftp_scheme", observe(ftp_scheme)),
        ("prefixed_scheme", observe(prefixed_scheme)),
        ("link_then_url", observe(link_then_url)),
        ("upper_case_scheme", observe(upper_case_scheme)),
        ("inline_code", observe(inline_code)),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "in_text",
                Observed::Pieces(vec![
                    text("see "),
                    link("https://e.test/a", "https://e.test/a"),
                    text(" now"),
                ]),
            ),
            (
                "trailing_dot",
                Observed::Pieces(vec![
                    text("end "),
                    link("https://e.test/a", "https://e.test/a"),
                    text("."),
                ]),
            ),
            (
                "in_parentheses",
                Observed::Pieces(vec![
                    text("("),
                    link("https://e.test/a", "https://e.test/a"),
                    text(")"),
                ]),
            ),
            (
                "http_scheme",
                Observed::Pieces(vec![
                    text("see "),
                    link("http://e.test", "http://e.test"),
                    text(" now"),
                ]),
            ),
            (
                "bare_host",
                Observed::Pieces(vec![text("see example.com now")]),
            ),
            (
                "ftp_scheme",
                Observed::Pieces(vec![text("see ftp://e.test now")]),
            ),
            (
                "prefixed_scheme",
                Observed::Pieces(vec![text("see xhttps://e.test now")]),
            ),
            (
                "link_then_url",
                Observed::Pieces(vec![
                    link("https://e.test/l", "x"),
                    text(" and "),
                    link("https://e.test/m", "https://e.test/m"),
                ]),
            ),
            (
                "upper_case_scheme",
                Observed::Pieces(vec![
                    text("see "),
                    link("HTTPS://E.TEST/A", "HTTPS://E.TEST/A"),
                    text(" now"),
                ]),
            ),
            (
                "inline_code",
                Observed::Pieces(vec![Piece::Code("https://e.test/a".to_owned())]),
            ),
        ],
    );
}
