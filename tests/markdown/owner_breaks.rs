//! Public-API regression for owner line breaks (Step 5(g) slice S2b).
//!
//! ORIGINAL (`renderMarkdown`, `breaks` true) gives a `br` for a soft break and for a hard break. Expected values come
//! from the independent ORIGINAL captures (`s2b-original` rows W1 to W7, `s2b-original-v2` rows W9 and W10; the HTML
//! mapped to a tree by the S2a contract section 5 rules, plus the S2b rule that a `br` is a `Break` and the layout LF
//! after it is dropped) and the contract, never from candidate output. Row W8 (an entity LF stays text in ORIGINAL) is
//! recorded only and is not asserted here. Only the public `Display` values are compared. Every public outcome of a
//! test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A).

use swarm::view::transcript::markdown::display::{Block, Display, Inline, display_with_breaks};

/// What one input gives.
#[derive(Debug, PartialEq)]
enum Observed {
    /// The call failed.
    Error,
    /// The blocks of the display tree.
    Tree(Vec<Block>),
}

fn observe_breaks(source: &str) -> Observed {
    match display_with_breaks(source) {
        Ok(Display { blocks }) => Observed::Tree(blocks),
        Err(_) => Observed::Error,
    }
}

fn tree(blocks: Vec<Block>) -> Observed {
    Observed::Tree(blocks)
}

fn paragraph(children: Vec<Inline>) -> Block {
    Block::Paragraph(children)
}

fn quote(children: Vec<Block>) -> Block {
    Block::Blockquote(children)
}

fn heading(depth: u8, children: Vec<Inline>) -> Block {
    Block::Heading { depth, children }
}

fn bullets(tight: bool, items: Vec<Vec<Block>>) -> Block {
    Block::List {
        ordered: false,
        start: None,
        tight,
        items,
    }
}

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}

fn emphasis(children: Vec<Inline>) -> Inline {
    Inline::Emphasis(children)
}

fn inline_code(value: &str) -> Inline {
    Inline::Code(value.to_owned())
}

fn link(url: &str, title: Option<&str>, children: Vec<Inline>) -> Inline {
    Inline::Link {
        url: url.to_owned(),
        title: title.map(str::to_owned),
        children,
    }
}

#[test]
fn owner_breaks_match_the_original() {
    // W1, 3 B: 610a62
    // ORIGINAL: text `a`, break, text `b`.
    let soft_break = "a\nb";
    // W2, 5 B: 6120200a62
    // ORIGINAL: text `a`, one break, text `b` (the same HTML as the soft break).
    let hard_break_spaces = "a  \nb";
    // W3, 5 B: 2a610a622a
    // ORIGINAL: emphasis with text `a`, break, text `b`.
    let break_in_emphasis = "*a\nb*";
    // W4, 23 B: 5b610a625d2868747470733a2f2f652e746573742f7829
    // ORIGINAL: link to the url with text `a`, break, text `b`.
    let break_in_link_text = "[a\nb](https://e.test/x)";
    // W5, 5 B: 60610a6260
    // ORIGINAL: one inline code `a b` (the line end is a space, no break).
    let code_span_line_end = "`a\nb`";
    // W6, 7 B: 2d20610a202062
    // ORIGINAL: tight bullet list, one item, text `a`, break, text `b`.
    let break_in_list_item = "- a\n  b";
    // W7, 7 B: 3e20610a3e2062
    // ORIGINAL: quote with a paragraph of text `a`, break, text `b`.
    let break_in_quote = "> a\n> b";
    // W9, 5 B: 2a612a0a62
    // ORIGINAL: emphasis with text `a`, break, text `b` (no empty piece).
    let break_after_emphasis = "*a*\nb";
    // W10, 7 B: 610a620a3d3d3d
    // ORIGINAL: heading of depth 1 with text `a`, break, text `b`.
    let break_in_setext_heading = "a\nb\n===";

    let observed = vec![
        ("soft_break", observe_breaks(soft_break)),
        ("hard_break_spaces", observe_breaks(hard_break_spaces)),
        ("break_in_emphasis", observe_breaks(break_in_emphasis)),
        ("break_in_link_text", observe_breaks(break_in_link_text)),
        ("code_span_line_end", observe_breaks(code_span_line_end)),
        ("break_in_list_item", observe_breaks(break_in_list_item)),
        ("break_in_quote", observe_breaks(break_in_quote)),
        ("break_after_emphasis", observe_breaks(break_after_emphasis)),
        (
            "break_in_setext_heading",
            observe_breaks(break_in_setext_heading),
        ),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "soft_break",
                tree(vec![paragraph(vec![text("a"), Inline::Break, text("b")])])
            ),
            (
                "hard_break_spaces",
                tree(vec![paragraph(vec![text("a"), Inline::Break, text("b")])])
            ),
            (
                "break_in_emphasis",
                tree(vec![paragraph(vec![emphasis(vec![
                    text("a"),
                    Inline::Break,
                    text("b")
                ])])])
            ),
            (
                "break_in_link_text",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![text("a"), Inline::Break, text("b")]
                )])])
            ),
            (
                "code_span_line_end",
                tree(vec![paragraph(vec![inline_code("a b")])])
            ),
            (
                "break_in_list_item",
                tree(vec![bullets(
                    true,
                    vec![vec![paragraph(vec![text("a"), Inline::Break, text("b")])]]
                )])
            ),
            (
                "break_in_quote",
                tree(vec![quote(vec![paragraph(vec![
                    text("a"),
                    Inline::Break,
                    text("b")
                ])])])
            ),
            (
                "break_after_emphasis",
                tree(vec![paragraph(vec![
                    emphasis(vec![text("a")]),
                    Inline::Break,
                    text("b")
                ])])
            ),
            (
                "break_in_setext_heading",
                tree(vec![heading(1, vec![text("a"), Inline::Break, text("b")])])
            ),
        ],
    );
}
