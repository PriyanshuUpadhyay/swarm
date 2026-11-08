//! Public-API regression for the code heads and table copy sources of the display entry (Step 5(g) display slice D-1).
//!
//! ORIGINAL (`markdown.ts`) gives every fenced code block a head whose label is the fence word, or `code`
//! when the word is empty. An indented code block gets no head, also when its text looks like a fence. It
//! gives every table the head `Table` and a copy source: the source lines of the table, joined with LF.
//! Expected values come from the independent ORIGINAL captures (`d1-original` rows H1 to H10 and T1 to T6,
//! `d1-h11` row H11, `breaks` false) and the contract, never from candidate output. Only the public
//! `Head` values are compared. Every public outcome of a test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A). Positions are not compared.

use markdown_destination::display::{Head, heads};

/// What one input gives.
#[derive(Debug, PartialEq)]
enum Observed {
    /// The call failed.
    Error,
    /// The heads in document order.
    Heads(Vec<Head>),
}

fn observe(source: &str) -> Observed {
    match heads(source) {
        Ok(found) => Observed::Heads(found),
        Err(_) => Observed::Error,
    }
}

fn code(label: &str) -> Head {
    Head::Code {
        label: label.to_owned(),
    }
}

fn table(source: &str) -> Head {
    Head::Table {
        source: source.to_owned(),
    }
}

#[test]
fn code_heads_use_the_fence_word() {
    // H1, 9 B: 6060600a780a606060
    // ORIGINAL: one head, label `code` (the word is empty).
    let no_word = "```\nx\n```";
    // H2, 13 B: 606060727573740a780a606060
    // ORIGINAL: one head, label `rust`.
    let word_rust = "```rust\nx\n```";
    // H3, 11 B: 7e7e7e70790a780a7e7e7e
    // ORIGINAL: one head, label `py` (tilde fence).
    let tilde_fence = "~~~py\nx\n~~~";
    // H4, 16 B: 6060606126616d703b620a780a606060
    // ORIGINAL: one head, label `a&amp;b` (the entity is not decoded).
    let entity_word = "```a&amp;b\nx\n```";
    // H5, 6 B: 20202020780a
    // ORIGINAL: no head (indented code block).
    let indented_code = "    x\n";
    // H6, 17 B: 2d2060606073680a2020780a2020606060
    // ORIGINAL: one head, label `sh` (fence inside a list item).
    let in_list_item = "- ```sh\n  x\n  ```";
    // H7, 17 B: 3e20606060676f0a3e20780a3e20606060
    // ORIGINAL: one head, label `go` (fence inside a quote).
    let in_quote = "> ```go\n> x\n> ```";
    // H8, 14 B: 606060646966660a2b610a606060
    // ORIGINAL: one head, label `diff` (a diff fence without a hunk keeps the head).
    let diff_word = "```diff\n+a\n```";
    // H9, 9 B: 610a0a6060600a780a
    // ORIGINAL: one head, label `code` (the fence is not closed).
    let unclosed = "a\n\n```\nx\n";
    // H10, 17 B: 20206060606a730a2020780a2020606060
    // ORIGINAL: one head, label `js` (a fence indented by two spaces).
    let indented_two = "  ```js\n  x\n  ```";
    // H11, 14 B: 202020206060600a20202020780a
    // ORIGINAL: no head (an indented code block whose text looks like a fence).
    let indented_fence_text = "    ```\n    x\n";

    let observed = vec![
        ("no_word", observe(no_word)),
        ("word_rust", observe(word_rust)),
        ("tilde_fence", observe(tilde_fence)),
        ("entity_word", observe(entity_word)),
        ("indented_code", observe(indented_code)),
        ("in_list_item", observe(in_list_item)),
        ("in_quote", observe(in_quote)),
        ("diff_word", observe(diff_word)),
        ("unclosed", observe(unclosed)),
        ("indented_two", observe(indented_two)),
        ("indented_fence_text", observe(indented_fence_text)),
    ];

    assert_eq!(
        observed,
        vec![
            ("no_word", Observed::Heads(vec![code("code")])),
            ("word_rust", Observed::Heads(vec![code("rust")])),
            ("tilde_fence", Observed::Heads(vec![code("py")])),
            ("entity_word", Observed::Heads(vec![code("a&amp;b")])),
            ("indented_code", Observed::Heads(vec![])),
            ("in_list_item", Observed::Heads(vec![code("sh")])),
            ("in_quote", Observed::Heads(vec![code("go")])),
            ("diff_word", Observed::Heads(vec![code("diff")])),
            ("unclosed", Observed::Heads(vec![code("code")])),
            ("indented_two", Observed::Heads(vec![code("js")])),
            ("indented_fence_text", Observed::Heads(vec![])),
        ],
    );
}

#[test]
fn tables_copy_their_source_lines() {
    // T1, 29 B: 7c2061207c2062207c0a7c2d2d2d7c2d2d2d7c0a7c2031207c2032207c
    // ORIGINAL: one head, copy source is the three lines.
    let plain_table = "| a | b |\n|---|---|\n| 1 | 2 |";
    // T2, 23 B: 780a0a7c2061207c0a7c2d2d2d7c0a7c2031207c0a0a79
    // ORIGINAL: one head, copy source is the three table lines (not `x` or `y`).
    let between_paragraphs = "x\n\n| a |\n|---|\n| 1 |\n\ny";
    // T3, 23 B: 3e207c2061207c0a3e207c2d2d2d7c0a3e207c2031207c
    // ORIGINAL: one head, copy source keeps the quote prefix `> ` on each line.
    let in_quote = "> | a |\n> |---|\n> | 1 |";
    // T4, 22 B: 7c2061207c0a7c2d2d2d7c0a7c2031207c0a6d6f7265
    // ORIGINAL: one head, copy source includes the line `more` (it is a table row).
    let continuation_row = "| a |\n|---|\n| 1 |\nmore";
    // T5, 11 B: 7c2061207c0a7c2d2d2d7c
    // ORIGINAL: one head, copy source is the header and delimiter lines.
    let header_only = "| a |\n|---|";
    // T6, 18 B: 7c2061207c0a7c2d2d2d7c0a7c2031207c0a
    // ORIGINAL: one head, copy source is the three lines without the final LF.
    let trailing_lf = "| a |\n|---|\n| 1 |\n";

    let observed = vec![
        ("plain_table", observe(plain_table)),
        ("between_paragraphs", observe(between_paragraphs)),
        ("in_quote", observe(in_quote)),
        ("continuation_row", observe(continuation_row)),
        ("header_only", observe(header_only)),
        ("trailing_lf", observe(trailing_lf)),
    ];

    assert_eq!(
        observed,
        vec![
            (
                "plain_table",
                Observed::Heads(vec![table("| a | b |\n|---|---|\n| 1 | 2 |")]),
            ),
            (
                "between_paragraphs",
                Observed::Heads(vec![table("| a |\n|---|\n| 1 |")]),
            ),
            (
                "in_quote",
                Observed::Heads(vec![table("> | a |\n> |---|\n> | 1 |")]),
            ),
            (
                "continuation_row",
                Observed::Heads(vec![table("| a |\n|---|\n| 1 |\nmore")]),
            ),
            ("header_only", Observed::Heads(vec![table("| a |\n|---|")]),),
            (
                "trailing_lf",
                Observed::Heads(vec![table("| a |\n|---|\n| 1 |")]),
            ),
        ],
    );
}
