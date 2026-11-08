//! Public-API regression for the display tree of blocks and inlines (Step 5(g) slice S2a).
//!
//! ORIGINAL (`renderMarkdown`, `breaks` false) paints a paragraph, heading, thematic break, quote, list, code
//! block and the inlines emphasis, strong, delete, inline code, break and link. Expected values come from the
//! independent ORIGINAL capture (`s2a-original`, rows B1 to B10, L1 to L10, C1 to C11, I1 to I10, K1 to K5; the
//! HTML mapped to a tree by the contract section 5 rules) and the contract, never from candidate output. Rows K6
//! and K7 (the visible text of an angle autolink) are recorded only and are not asserted here. Only the public
//! `Display` values are compared. Every public outcome of a test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A), `\\` is one backslash, `\"` is a quote, `\u{a9}` is U+00A9. Positions are not compared.

use swarm::view::transcript::markdown::display::{Block, Display, Inline, display};

/// What one input gives.
#[derive(Debug, PartialEq)]
enum Observed {
    /// The call failed.
    Error,
    /// The blocks of the display tree.
    Tree(Vec<Block>),
}

fn observe(source: &str) -> Observed {
    match display(source) {
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

fn heading(depth: u8, children: Vec<Inline>) -> Block {
    Block::Heading { depth, children }
}

fn quote(children: Vec<Block>) -> Block {
    Block::Blockquote(children)
}

fn bullets(tight: bool, items: Vec<Vec<Block>>) -> Block {
    Block::List {
        ordered: false,
        start: None,
        tight,
        items,
    }
}

fn numbered(start: u32, tight: bool, items: Vec<Vec<Block>>) -> Block {
    Block::List {
        ordered: true,
        start: Some(start),
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

fn strong(children: Vec<Inline>) -> Inline {
    Inline::Strong(children)
}

fn delete(children: Vec<Inline>) -> Inline {
    Inline::Delete(children)
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
fn blocks_paint_as_the_original() {
    // B1, 4 B: 610a0a62
    // ORIGINAL: two paragraphs, `a` and `b`.
    let two_paragraphs = "a\n\nb";
    // B2, 5 B: 2320612023
    // ORIGINAL: one heading, depth 1, text `a` (the closing `#` is not text).
    let closed_atx = "# a #";
    // B3, 5 B: 610a3d3d3d
    // ORIGINAL: one heading, depth 1, text `a`.
    let setext_equals = "a\n===";
    // B4, 5 B: 610a2d2d2d
    // ORIGINAL: one heading, depth 2, text `a`.
    let setext_dashes = "a\n---";
    // B5, 8 B: 2323232323232061
    // ORIGINAL: one heading, depth 6.
    let depth_six = "###### a";
    // B6, 9 B: 232323232323232061
    // ORIGINAL: one paragraph with the text `####### a`.
    let depth_seven = "####### a";
    // B7, 3 B: 2a2a2a
    // ORIGINAL: one thematic break.
    let thematic_break = "***";
    // B8, 7 B: 3e20610a3e2062
    // ORIGINAL: one quote with one paragraph, text `a`, LF, `b`.
    let quote_soft_break = "> a\n> b";
    // B9, 9 B: 3e20610a3e0a3e2062
    // ORIGINAL: one quote with two paragraphs.
    let quote_two_paragraphs = "> a\n>\n> b";
    // B10, 5 B: 3e203e2061
    // ORIGINAL: a quote inside a quote, with one paragraph.
    let nested_quote = "> > a";

    let observed = vec![
        ("two_paragraphs", observe(two_paragraphs)),
        ("closed_atx", observe(closed_atx)),
        ("setext_equals", observe(setext_equals)),
        ("setext_dashes", observe(setext_dashes)),
        ("depth_six", observe(depth_six)),
        ("depth_seven", observe(depth_seven)),
        ("thematic_break", observe(thematic_break)),
        ("quote_soft_break", observe(quote_soft_break)),
        ("quote_two_paragraphs", observe(quote_two_paragraphs)),
        ("nested_quote", observe(nested_quote)),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "two_paragraphs",
                tree(vec![paragraph(vec![text("a")]), paragraph(vec![text("b")])])
            ),
            ("closed_atx", tree(vec![heading(1, vec![text("a")])])),
            ("setext_equals", tree(vec![heading(1, vec![text("a")])])),
            ("setext_dashes", tree(vec![heading(2, vec![text("a")])])),
            ("depth_six", tree(vec![heading(6, vec![text("a")])])),
            (
                "depth_seven",
                tree(vec![paragraph(vec![text("####### a")])])
            ),
            ("thematic_break", tree(vec![Block::ThematicBreak])),
            (
                "quote_soft_break",
                tree(vec![quote(vec![paragraph(vec![text("a\nb")])])])
            ),
            (
                "quote_two_paragraphs",
                tree(vec![quote(vec![
                    paragraph(vec![text("a")]),
                    paragraph(vec![text("b")])
                ])])
            ),
            (
                "nested_quote",
                tree(vec![quote(vec![quote(vec![paragraph(vec![text("a")])])])])
            ),
        ],
    );
}

#[test]
fn lists_keep_order_start_and_tightness() {
    // L1, 7 B: 2d20610a2d2062
    // ORIGINAL: one tight bullet list, items `a` and `b`.
    let tight_bullets = "- a\n- b";
    // L2, 8 B: 2d20610a0a2d2062
    // ORIGINAL: one loose bullet list.
    let loose_bullets = "- a\n\n- b";
    // L3, 9 B: 312e20610a322e2062
    // ORIGINAL: one tight ordered list, start 1.
    let ordered_from_one = "1. a\n2. b";
    // L4, 9 B: 332e20610a342e2062
    // ORIGINAL: one tight ordered list, start 3.
    let ordered_from_three = "3. a\n4. b";
    // L5, 9 B: 2d20610a20202d2062
    // ORIGINAL: one tight list; the item has paragraph `a` and a nested tight list with `b`.
    let nested_list = "- a\n  - b";
    // L6, 5 B: 2d0a2d2062
    // ORIGINAL: one tight list; the first item is empty, the second is `b`.
    let empty_first_item = "-\n- b";
    // L7, 8 B: 2d20610a0a202062
    // ORIGINAL: one loose list; one item with paragraphs `a` and `b`.
    let item_with_two_paragraphs = "- a\n\n  b";
    // L8, 12 B: 2d20610a2d20620a0a2d2063
    // ORIGINAL: one loose list of three items (one blank line makes the whole list loose).
    let blank_line_between_items = "- a\n- b\n\n- c";
    // L9, 7 B: 2a20610a2b2062
    // ORIGINAL: two bullet lists, each tight with one item.
    let marker_change = "* a\n+ b";
    // L10, 5 B: 2d203e2061
    // ORIGINAL: one tight list; the item is a quote with paragraph `a`.
    let item_with_quote = "- > a";

    let observed = vec![
        ("tight_bullets", observe(tight_bullets)),
        ("loose_bullets", observe(loose_bullets)),
        ("ordered_from_one", observe(ordered_from_one)),
        ("ordered_from_three", observe(ordered_from_three)),
        ("nested_list", observe(nested_list)),
        ("empty_first_item", observe(empty_first_item)),
        (
            "item_with_two_paragraphs",
            observe(item_with_two_paragraphs),
        ),
        (
            "blank_line_between_items",
            observe(blank_line_between_items),
        ),
        ("marker_change", observe(marker_change)),
        ("item_with_quote", observe(item_with_quote)),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "tight_bullets",
                tree(vec![bullets(
                    true,
                    vec![
                        vec![paragraph(vec![text("a")])],
                        vec![paragraph(vec![text("b")])]
                    ]
                )])
            ),
            (
                "loose_bullets",
                tree(vec![bullets(
                    false,
                    vec![
                        vec![paragraph(vec![text("a")])],
                        vec![paragraph(vec![text("b")])]
                    ]
                )])
            ),
            (
                "ordered_from_one",
                tree(vec![numbered(
                    1,
                    true,
                    vec![
                        vec![paragraph(vec![text("a")])],
                        vec![paragraph(vec![text("b")])]
                    ]
                )])
            ),
            (
                "ordered_from_three",
                tree(vec![numbered(
                    3,
                    true,
                    vec![
                        vec![paragraph(vec![text("a")])],
                        vec![paragraph(vec![text("b")])]
                    ]
                )])
            ),
            (
                "nested_list",
                tree(vec![bullets(
                    true,
                    vec![vec![
                        paragraph(vec![text("a")]),
                        bullets(true, vec![vec![paragraph(vec![text("b")])]])
                    ]]
                )])
            ),
            (
                "empty_first_item",
                tree(vec![bullets(
                    true,
                    vec![vec![], vec![paragraph(vec![text("b")])]]
                )])
            ),
            (
                "item_with_two_paragraphs",
                tree(vec![bullets(
                    false,
                    vec![vec![paragraph(vec![text("a")]), paragraph(vec![text("b")])]]
                )])
            ),
            (
                "blank_line_between_items",
                tree(vec![bullets(
                    false,
                    vec![
                        vec![paragraph(vec![text("a")])],
                        vec![paragraph(vec![text("b")])],
                        vec![paragraph(vec![text("c")])]
                    ]
                )])
            ),
            (
                "marker_change",
                tree(vec![
                    bullets(true, vec![vec![paragraph(vec![text("a")])]]),
                    bullets(true, vec![vec![paragraph(vec![text("b")])]])
                ])
            ),
            (
                "item_with_quote",
                tree(vec![bullets(
                    true,
                    vec![vec![quote(vec![paragraph(vec![text("a")])])]]
                )])
            ),
        ],
    );
}

#[test]
fn inlines_match_the_original() {
    // I1, 15 B: 2a612a202a2a622a2a207e7e637e7e
    // ORIGINAL: emphasis `a`, space, strong `b`, space, delete `c`.
    let emphasis_strong_delete = "*a* **b** ~~c~~";
    // I2, 11 B: 2a61202a2a622a2a20632a
    // ORIGINAL: emphasis with text `a `, strong `b`, text ` c`.
    let nested_inlines = "*a **b** c*";
    // I3, 5 B: 612a622a63
    // ORIGINAL: text `a`, emphasis `b`, text `c`.
    let intraword_star = "a*b*c";
    // I4, 5 B: 615f625f63
    // ORIGINAL: one text `a_b_c`.
    let intraword_underscore = "a_b_c";
    // I5, 4 B: 2a2a612a
    // ORIGINAL: text `*`, emphasis `a`.
    let unmatched_star = "**a*";
    // I6, 11 B: 6060206120602062206060
    // ORIGINAL: one inline code `a ` b`.
    let code_span_strip = "`` a ` b ``";
    // I7, 5 B: 6120200a62
    // ORIGINAL: text `a`, break, text `b`.
    let hard_break_spaces = "a  \nb";
    // I8, 4 B: 615c0a62
    // ORIGINAL: text `a`, break, text `b`.
    let hard_break_backslash = "a\\\nb";
    // I9, 4 B: 61200a62
    // ORIGINAL: one text `a`, LF, `b`.
    let soft_break = "a \nb";
    // I10, 15 B: 26616d703b2026636f70793b205c2a
    // ORIGINAL: one text `&`, space, U+00A9, space, `*`.
    let entities_and_escape = "&amp; &copy; \\*";

    let observed = vec![
        ("emphasis_strong_delete", observe(emphasis_strong_delete)),
        ("nested_inlines", observe(nested_inlines)),
        ("intraword_star", observe(intraword_star)),
        ("intraword_underscore", observe(intraword_underscore)),
        ("unmatched_star", observe(unmatched_star)),
        ("code_span_strip", observe(code_span_strip)),
        ("hard_break_spaces", observe(hard_break_spaces)),
        ("hard_break_backslash", observe(hard_break_backslash)),
        ("soft_break", observe(soft_break)),
        ("entities_and_escape", observe(entities_and_escape)),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "emphasis_strong_delete",
                tree(vec![paragraph(vec![
                    emphasis(vec![text("a")]),
                    text(" "),
                    strong(vec![text("b")]),
                    text(" "),
                    delete(vec![text("c")])
                ])])
            ),
            (
                "nested_inlines",
                tree(vec![paragraph(vec![emphasis(vec![
                    text("a "),
                    strong(vec![text("b")]),
                    text(" c")
                ])])])
            ),
            (
                "intraword_star",
                tree(vec![paragraph(vec![
                    text("a"),
                    emphasis(vec![text("b")]),
                    text("c")
                ])])
            ),
            (
                "intraword_underscore",
                tree(vec![paragraph(vec![text("a_b_c")])])
            ),
            (
                "unmatched_star",
                tree(vec![paragraph(vec![text("*"), emphasis(vec![text("a")])])])
            ),
            (
                "code_span_strip",
                tree(vec![paragraph(vec![inline_code("a ` b")])])
            ),
            (
                "hard_break_spaces",
                tree(vec![paragraph(vec![text("a"), Inline::Break, text("b")])])
            ),
            (
                "hard_break_backslash",
                tree(vec![paragraph(vec![text("a"), Inline::Break, text("b")])])
            ),
            ("soft_break", tree(vec![paragraph(vec![text("a\nb")])])),
            (
                "entities_and_escape",
                tree(vec![paragraph(vec![text("& \u{a9} *")])])
            ),
        ],
    );
}

#[test]
fn links_keep_url_and_title() {
    // K1, 31 B: 5b615d2868747470733a2f2f652e746573742f78202274205c22715c222229
    // ORIGINAL: link, title `t "q"`, text `a`.
    let title_with_quotes = "[a](https://e.test/x \"t \\\"q\\\"\")";
    // K2, 25 B: 5b615d2868747470733a2f2f652e746573742f782027742729
    // ORIGINAL: link, title `t`.
    let single_quote_title = "[a](https://e.test/x 't')";
    // K3, 25 B: 5b615d2868747470733a2f2f652e746573742f782028742929
    // ORIGINAL: link, title `t`.
    let paren_title = "[a](https://e.test/x (t))";
    // K4, 24 B: 5b615d2868747470733a2f2f652e746573742f7820222229
    // ORIGINAL: link, no title.
    let empty_title = "[a](https://e.test/x \"\")";
    // K5, 27 B: 5b2a612a206062605d2868747470733a2f2f652e746573742f7829
    // ORIGINAL: link, no title, children emphasis `a`, space, inline code `b`.
    let inline_children = "[*a* `b`](https://e.test/x)";

    let observed = vec![
        ("title_with_quotes", observe(title_with_quotes)),
        ("single_quote_title", observe(single_quote_title)),
        ("paren_title", observe(paren_title)),
        ("empty_title", observe(empty_title)),
        ("inline_children", observe(inline_children)),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "title_with_quotes",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    Some("t \"q\""),
                    vec![text("a")]
                )])])
            ),
            (
                "single_quote_title",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    Some("t"),
                    vec![text("a")]
                )])])
            ),
            (
                "paren_title",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    Some("t"),
                    vec![text("a")]
                )])])
            ),
            (
                "empty_title",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![text("a")]
                )])])
            ),
            (
                "inline_children",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![emphasis(vec![text("a")]), text(" "), inline_code("b")]
                )])])
            ),
        ],
    );
}
