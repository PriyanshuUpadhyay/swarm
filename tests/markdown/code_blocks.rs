//! Public-API regression for code blocks in the display tree (Step 5(g) slices S2a and S2c).
//!
//! Moved without change from `display_tree.rs` (S2c revision) so each test file stays under 20000 bytes. The code
//! text rows C1 to C11 come from the independent S2a ORIGINAL capture (`s2a-original`); each fenced row label is
//! the sealed S2a `md-head` span text and each indented row has none. Expected values never come from candidate
//! output. Every public outcome of the test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A). Positions are not compared.

use swarm::view::transcript::markdown::display::{Block, Display, display};

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
    }
}

#[test]
fn code_text_matches_the_original() {
    // C1, 9 B: 6060600a780a606060
    // ORIGINAL: text `x`, LF.
    let fence_with_line = "```\nx\n```";
    // C2, 7 B: 6060600a606060
    // ORIGINAL: empty text.
    let fence_without_line = "```\n```";
    // C3, 8 B: 6060600a0a606060
    // ORIGINAL: text LF.
    let fence_blank_line = "```\n\n```";
    // C4, 5 B: 6060600a78
    // ORIGINAL: text `x` (no LF).
    let unclosed_no_final_lf = "```\nx";
    // C5, 6 B: 6060600a780a
    // ORIGINAL: text `x`, LF.
    let unclosed_final_lf = "```\nx\n";
    // C6, 7 B: 20202020780a0a
    // ORIGINAL: text `x`, LF.
    let indented_code = "    x\n\n";
    // C7, 15 B: 3e206060600a3e20780a3e20606060
    // ORIGINAL: a quote with one code block, text `x`, LF.
    let fence_in_quote = "> ```\n> x\n> ```";
    // C8, 16 B: 20206060600a202020780a2020606060
    // ORIGINAL: text space, `x`, LF.
    let indented_fence_content = "  ```\n   x\n  ```";
    // C9, 11 B: 20202020780a2020202020
    // ORIGINAL: text `x`, LF (the last line of spaces is dropped).
    let trailing_spaces_line = "    x\n     ";
    // C10, 17 B: 20202020780a20202020200a2020202079
    // ORIGINAL: text `x`, LF, one space, LF, `y`, LF (the inner line keeps the spaces past the indent).
    let inner_spaces_line = "    x\n     \n    y";
    // C11, 13 B: 20202020780a2020202020200a
    // ORIGINAL: text `x`, LF (the last line of spaces is dropped).
    let trailing_spaces_line_with_lf = "    x\n      \n";

    let observed = vec![
        ("fence_with_line", observe(fence_with_line)),
        ("fence_without_line", observe(fence_without_line)),
        ("fence_blank_line", observe(fence_blank_line)),
        ("unclosed_no_final_lf", observe(unclosed_no_final_lf)),
        ("unclosed_final_lf", observe(unclosed_final_lf)),
        ("indented_code", observe(indented_code)),
        ("fence_in_quote", observe(fence_in_quote)),
        ("indented_fence_content", observe(indented_fence_content)),
        ("trailing_spaces_line", observe(trailing_spaces_line)),
        ("inner_spaces_line", observe(inner_spaces_line)),
        (
            "trailing_spaces_line_with_lf",
            observe(trailing_spaces_line_with_lf),
        ),
    ];

    assert_eq!(
        observed,
        vec![
            ("fence_with_line", tree(vec![code(Some("code"), "x\n")])),
            ("fence_without_line", tree(vec![code(Some("code"), "")])),
            ("fence_blank_line", tree(vec![code(Some("code"), "\n")])),
            ("unclosed_no_final_lf", tree(vec![code(Some("code"), "x")])),
            ("unclosed_final_lf", tree(vec![code(Some("code"), "x\n")])),
            ("indented_code", tree(vec![code(None, "x\n")])),
            (
                "fence_in_quote",
                tree(vec![quote(vec![code(Some("code"), "x\n")])])
            ),
            (
                "indented_fence_content",
                tree(vec![code(Some("code"), " x\n")])
            ),
            ("trailing_spaces_line", tree(vec![code(None, "x\n")])),
            ("inner_spaces_line", tree(vec![code(None, "x\n \ny\n")])),
            (
                "trailing_spaces_line_with_lf",
                tree(vec![code(None, "x\n")])
            ),
        ],
    );
}
