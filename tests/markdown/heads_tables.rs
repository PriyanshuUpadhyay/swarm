//! Public-API regression for code heads and tables in the display tree (Step 5(g) slice S2c).
//!
//! ORIGINAL (`renderMarkdown`, `breaks` false) paints a fence with a head that holds the first word of the info
//! string (or `code`), paints indented code with no head, and paints a table with its source lines, its column
//! alignment, a head row and body rows. Expected values come from the independent ORIGINAL capture (`s2c-original`,
//! rows H1 to H7 and T1 to T7, T9; the HTML mapped to a tree by the S2a contract section 5 rules plus the S2c mapping)
//! and the contract, never from candidate output. Row H8 (a `diff` fence without hunks) is recorded only and is not
//! asserted here. Only the public `Display` values are compared. Every public outcome of a test is collected before
//! its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A), `\r` is CR (0x0D), `\"` is a quote, `\\` is one backslash.

use swarm::view::transcript::markdown::display::{Align, Block, Display, Inline, display};

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

fn quote(children: Vec<Block>) -> Block {
    Block::Blockquote(children)
}

fn code(label: Option<&str>, text: &str) -> Block {
    Block::Code {
        label: label.map(str::to_owned),
        text: text.to_owned(),
        diff: None,
    }
}

fn table(
    source: &str,
    align: Vec<Align>,
    head: Vec<Vec<Inline>>,
    rows: Vec<Vec<Vec<Inline>>>,
) -> Block {
    Block::Table {
        source: source.to_owned(),
        align,
        head,
        rows,
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

#[test]
fn fence_heads_match_the_original() {
    // H1, 13 B: 606060727573740a780a606060
    // ORIGINAL: one fence with label `rust` and text `x`, LF.
    let rust_label = "```rust\nx\n```";
    // H2, 19 B: 606060727573742065787472610a780a606060
    // ORIGINAL: label `rust` (the first word; `extra` is dropped), text `x`, LF.
    let label_is_first_word = "```rust extra\nx\n```";
    // H3, 9 B: 6060600a780a606060
    // ORIGINAL: label `code` when the info string is empty.
    let no_info_string_is_code = "```\nx\n```";
    // H4, 11 B: 7e7e7e70790a780a7e7e7e
    // ORIGINAL: label `py` from a tilde fence.
    let tilde_fence = "~~~py\nx\n~~~";
    // H5, 5 B: 2020202078
    // ORIGINAL: indented code has no label (None), text `x`, LF.
    let indented_code_has_no_label = "    x";
    // H6, 18 B: 606060266c743b622667743b0a780a606060
    // ORIGINAL: label is the raw text `&lt;b&gt;`, not decoded.
    let raw_label_not_decoded = "```&lt;b&gt;\nx\n```";
    // H7, 17 B: 3e206060606a730a3e20780a3e20606060
    // ORIGINAL: a quote holding one fence with label `js`.
    let fence_in_quote = "> ```js\n> x\n> ```";

    let observed = vec![
        ("rust_label", observe(rust_label)),
        ("label_is_first_word", observe(label_is_first_word)),
        ("no_info_string_is_code", observe(no_info_string_is_code)),
        ("tilde_fence", observe(tilde_fence)),
        (
            "indented_code_has_no_label",
            observe(indented_code_has_no_label),
        ),
        ("raw_label_not_decoded", observe(raw_label_not_decoded)),
        ("fence_in_quote", observe(fence_in_quote)),
    ];

    assert_eq!(
        observed,
        vec![
            ("rust_label", tree(vec![code(Some("rust"), "x\n")])),
            ("label_is_first_word", tree(vec![code(Some("rust"), "x\n")])),
            (
                "no_info_string_is_code",
                tree(vec![code(Some("code"), "x\n")])
            ),
            ("tilde_fence", tree(vec![code(Some("py"), "x\n")])),
            ("indented_code_has_no_label", tree(vec![code(None, "x\n")])),
            (
                "raw_label_not_decoded",
                tree(vec![code(Some("&lt;b&gt;"), "x\n")])
            ),
            (
                "fence_in_quote",
                tree(vec![quote(vec![code(Some("js"), "x\n")])])
            ),
        ],
    );
}

#[test]
fn tables_match_the_original() {
    // T1, 31 B: 7c2061207c2062207c0a7c2d2d2d7c3a2d3a7c0a7c2031207c202a322a207c
    // ORIGINAL: align None and Center; head `a`, `b`; one row `1` and emphasis `2`; source is the three lines.
    let aligned_table_with_emphasis = "| a | b |\n|---|:-:|\n| 1 | *2* |";
    // T2, 13 B: 61207c20620a2d2d7c2d2d0a31
    // ORIGINAL: the row `1` is padded with one empty cell; align None, None.
    let short_row_is_padded = "a | b\n--|--\n1";
    // T3, 21 B: 7c2061207c0a7c2d2d2d7c0a7c2031207c2032207c
    // ORIGINAL: the row is cut to the head width: only `1`.
    let long_row_is_cut = "| a |\n|---|\n| 1 | 2 |";
    // T4, 27 B: 7c2061205c7c2062207c0a7c2d2d2d7c0a7c2060635c7c6460207c
    // ORIGINAL: head text `a | b`; the cell holds inline code `c|d`; source keeps the backslashes.
    let escaped_pipe_and_code_pipe = "| a \\| b |\n|---|\n| `c\\|d` |";
    // T5, 23 B: 3e207c2061207c0a3e207c2d2d2d7c0a3e207c2031207c
    // ORIGINAL: a quote holding a table; source keeps the `> ` prefixes.
    let table_in_quote_keeps_prefix = "> | a |\n> |---|\n> | 1 |";
    // T6, 11 B: 7c2061207c0a7c3a2d2d7c
    // ORIGINAL: align Left, head `a`, no rows.
    let header_only_table = "| a |\n|:--|";
    // T7, 28 B: 7c2061207c2062207c0a7c2d2d3a7c2d2d2d7c0a7c2031207c20207c
    // ORIGINAL: align Right and None; the row `1` and one empty cell.
    let right_align_and_empty_cell = "| a | b |\n|--:|---|\n| 1 |  |";
    // T9, 19 B: 7c2061207c0d0a7c2d2d2d7c0d0a7c2031207c
    // ORIGINAL: source keeps each CR; head `a`, one row `1`.
    let crlf_source_keeps_cr = "| a |\r\n|---|\r\n| 1 |";

    let observed = vec![
        (
            "aligned_table_with_emphasis",
            observe(aligned_table_with_emphasis),
        ),
        ("short_row_is_padded", observe(short_row_is_padded)),
        ("long_row_is_cut", observe(long_row_is_cut)),
        (
            "escaped_pipe_and_code_pipe",
            observe(escaped_pipe_and_code_pipe),
        ),
        (
            "table_in_quote_keeps_prefix",
            observe(table_in_quote_keeps_prefix),
        ),
        ("header_only_table", observe(header_only_table)),
        (
            "right_align_and_empty_cell",
            observe(right_align_and_empty_cell),
        ),
        ("crlf_source_keeps_cr", observe(crlf_source_keeps_cr)),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "aligned_table_with_emphasis",
                tree(vec![table(
                    "| a | b |\n|---|:-:|\n| 1 | *2* |",
                    vec![Align::None, Align::Center],
                    vec![vec![text("a")], vec![text("b")]],
                    vec![vec![vec![text("1")], vec![emphasis(vec![text("2")])]]]
                )])
            ),
            (
                "short_row_is_padded",
                tree(vec![table(
                    "a | b\n--|--\n1",
                    vec![Align::None, Align::None],
                    vec![vec![text("a")], vec![text("b")]],
                    vec![vec![vec![text("1")], vec![]]]
                )])
            ),
            (
                "long_row_is_cut",
                tree(vec![table(
                    "| a |\n|---|\n| 1 | 2 |",
                    vec![Align::None],
                    vec![vec![text("a")]],
                    vec![vec![vec![text("1")]]]
                )])
            ),
            (
                "escaped_pipe_and_code_pipe",
                tree(vec![table(
                    "| a \\| b |\n|---|\n| `c\\|d` |",
                    vec![Align::None],
                    vec![vec![text("a | b")]],
                    vec![vec![vec![inline_code("c|d")]]]
                )])
            ),
            (
                "table_in_quote_keeps_prefix",
                tree(vec![quote(vec![table(
                    "> | a |\n> |---|\n> | 1 |",
                    vec![Align::None],
                    vec![vec![text("a")]],
                    vec![vec![vec![text("1")]]]
                )])])
            ),
            (
                "header_only_table",
                tree(vec![table(
                    "| a |\n|:--|",
                    vec![Align::Left],
                    vec![vec![text("a")]],
                    vec![]
                )])
            ),
            (
                "right_align_and_empty_cell",
                tree(vec![table(
                    "| a | b |\n|--:|---|\n| 1 |  |",
                    vec![Align::Right, Align::None],
                    vec![vec![text("a")], vec![text("b")]],
                    vec![vec![vec![text("1")], vec![]]]
                )])
            ),
            (
                "crlf_source_keeps_cr",
                tree(vec![table(
                    "| a |\r\n|---|\r\n| 1 |",
                    vec![Align::None],
                    vec![vec![text("a")]],
                    vec![vec![vec![text("1")]]]
                )])
            ),
        ],
    );
}
