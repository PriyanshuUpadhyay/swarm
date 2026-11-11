//! Public-API regression for link references and inline code line ends (Step 5(g) slice S2b).
//!
//! ORIGINAL (`renderMarkdown`, `breaks` false) turns a full, collapsed or shortcut reference into a link from the first
//! matching definition, and turns a line end in a code span into a space. Expected values come from the independent
//! ORIGINAL captures (`s2b-original` rows R1 to R9, `s2b-original-v2` rows R10, R11 and C12, `s2b-original-v3` row R12; the HTML mapped to a tree by
//! the S2a contract section 5 rules) and the contract, never from candidate output. Row C13 (the padding of a code span
//! that starts with a line end) is recorded only and is not asserted here. Only the public `Display` values are compared.
//! Every public outcome of a test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A), `\"` is a quote, `\u{a0}` is U+00A0, `\u{1e9e}` is U+1E9E.

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

fn quote(children: Vec<Block>) -> Block {
    Block::Blockquote(children)
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
fn references_link_as_in_the_original() {
    // R1, 29 B: 5b615d5b625d0a0a5b625d3a2068747470733a2f2f652e746573742f78
    // ORIGINAL: link to the definition url with text `a`.
    let full_reference = "[a][b]\n\n[b]: https://e.test/x";
    // R2, 28 B: 5b625d5b5d0a0a5b625d3a2068747470733a2f2f652e746573742f78
    // ORIGINAL: link to the definition url with text `b`.
    let collapsed_reference = "[b][]\n\n[b]: https://e.test/x";
    // R3, 26 B: 5b625d0a0a5b625d3a2068747470733a2f2f652e746573742f78
    // ORIGINAL: link to the definition url with text `b`.
    let shortcut_reference = "[b]\n\n[b]: https://e.test/x";
    // R4, 35 B: 5b422020635d0a0a5b6220635d3a2068747470733a2f2f652e746573742f7820227422
    // ORIGINAL: link with title `t`, text `B  c` (two spaces kept); the label match folds case and spaces.
    let label_fold_and_title = "[B  c]\n\n[b c]: https://e.test/x \"t\"";
    // R5, 48 B: 5b625d0a0a5b625d3a2068747470733a2f2f652e746573742f310a5b625d3a2068747470733a2f2f652e746573742f32
    // ORIGINAL: link to the first definition url `.../1`, text `b`.
    let first_definition_wins = "[b]\n\n[b]: https://e.test/1\n[b]: https://e.test/2";
    // R6, 28 B: 3e205b625d3a2068747470733a2f2f652e746573742f780a0a5b625d
    // ORIGINAL: an empty quote, then a link to the url with text `b`.
    let definition_in_quote = "> [b]: https://e.test/x\n\n[b]";
    // R7, 39 B: 5b2a612a206063605d5b625d0a0a5b625d3a203c68747470733a2f2f652e746573742f6120623e
    // ORIGINAL: link to the url with the space encoded as `%20`; children emphasis `a`, text space, inline code `c`.
    let inline_children_and_encoded_url = "[*a* `c`][b]\n\n[b]: <https://e.test/a b>";
    // R8, 22 B: 5b625d0a0a5b625d3a206a6176617363726970743a78
    // ORIGINAL: two paragraphs of text: `[b]` and `[b]: javascript:x`.
    let invalid_destination_is_text = "[b]\n\n[b]: javascript:x";
    // R9, 29 B: 5b615d5b635d0a0a5b625d3a2068747470733a2f2f652e746573742f78
    // ORIGINAL: one paragraph of text `[a][c]` (no definition is used).
    let undefined_label_is_text = "[a][c]\n\n[b]: https://e.test/x";
    // R10, 31 B: 5b61c2a0625d0a0a5b6120625d3a2068747470733a2f2f652e746573742f78
    // ORIGINAL: link to the definition url; the label matches through the space fold and the link text keeps U+00A0.
    let no_break_space_label = "[a\u{a0}b]\n\n[a b]: https://e.test/x";
    // R11, 29 B: 5be1ba9e5d0a0a5b73735d3a2068747470733a2f2f652e746573742f78
    // ORIGINAL: link to the definition url with text U+1E9E (it matches the label `ss`).
    let sharp_s_label = "[\u{1e9e}]\n\n[ss]: https://e.test/x";
    // R12, 48 B: 5b625d0a0a5b615d3a2068747470733a2f2f652e746573742f310a5b625d3a2068747470733a2f2f652e746573742f32
    // ORIGINAL: link to the url `.../2` of its own label `b`, not the first definition `a` (`.../1`).
    let own_definition_among_labels = "[b]\n\n[a]: https://e.test/1\n[b]: https://e.test/2";

    let observed = vec![
        ("full_reference", observe(full_reference)),
        ("collapsed_reference", observe(collapsed_reference)),
        ("shortcut_reference", observe(shortcut_reference)),
        ("label_fold_and_title", observe(label_fold_and_title)),
        ("first_definition_wins", observe(first_definition_wins)),
        ("definition_in_quote", observe(definition_in_quote)),
        (
            "inline_children_and_encoded_url",
            observe(inline_children_and_encoded_url),
        ),
        (
            "invalid_destination_is_text",
            observe(invalid_destination_is_text),
        ),
        ("undefined_label_is_text", observe(undefined_label_is_text)),
        ("no_break_space_label", observe(no_break_space_label)),
        ("sharp_s_label", observe(sharp_s_label)),
        (
            "own_definition_among_labels",
            observe(own_definition_among_labels),
        ),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "full_reference",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![text("a")]
                )])])
            ),
            (
                "collapsed_reference",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![text("b")]
                )])])
            ),
            (
                "shortcut_reference",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![text("b")]
                )])])
            ),
            (
                "label_fold_and_title",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    Some("t"),
                    vec![text("B  c")]
                )])])
            ),
            (
                "first_definition_wins",
                tree(vec![paragraph(vec![link(
                    "https://e.test/1",
                    None,
                    vec![text("b")]
                )])])
            ),
            (
                "definition_in_quote",
                tree(vec![
                    quote(vec![]),
                    paragraph(vec![link("https://e.test/x", None, vec![text("b")])])
                ])
            ),
            (
                "inline_children_and_encoded_url",
                tree(vec![paragraph(vec![link(
                    "https://e.test/a%20b",
                    None,
                    vec![emphasis(vec![text("a")]), text(" "), inline_code("c")]
                )])])
            ),
            (
                "invalid_destination_is_text",
                tree(vec![
                    paragraph(vec![text("[b]")]),
                    paragraph(vec![text("[b]: javascript:x")])
                ])
            ),
            (
                "undefined_label_is_text",
                tree(vec![paragraph(vec![text("[a][c]")])])
            ),
            (
                "no_break_space_label",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![text("a\u{a0}b")]
                )])])
            ),
            (
                "sharp_s_label",
                tree(vec![paragraph(vec![link(
                    "https://e.test/x",
                    None,
                    vec![text("\u{1e9e}")]
                )])])
            ),
            (
                "own_definition_among_labels",
                tree(vec![paragraph(vec![link(
                    "https://e.test/2",
                    None,
                    vec![text("b")]
                )])])
            ),
        ],
    );
}

#[test]
fn inline_code_line_ends_are_spaces() {
    // C12, 5 B: 60610a6260
    // ORIGINAL: one inline code `a b` (the line end is a space).
    let code_span_line_end = "`a\nb`";

    let observed = vec![("code_span_line_end", observe(code_span_line_end))];

    assert_eq!(
        observed,
        vec![(
            "code_span_line_end",
            tree(vec![paragraph(vec![inline_code("a b")])])
        ),],
    );
}
