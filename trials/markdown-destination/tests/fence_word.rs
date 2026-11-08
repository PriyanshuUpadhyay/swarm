//! Public-API regression for the fence label of the producer entry (Step 5(g) slice G4).
//!
//! ORIGINAL (markdown-it plus `markdown.ts:53`) paints the first whitespace-free word of the raw text
//! after the fence marker: `info.trim().split(/\s+/)[0]`, where `\s` is the JavaScript whitespace set.
//! It does not decode entities or backslash escapes in that word, and an empty word shows no label.
//! Expected values come from the independent ORIGINAL capture (`g4-original`, fence token `info` and
//! the word, `breaks` false) and the contract, never from candidate output. Only the public `lang` of
//! the first Code node is compared. Every public outcome of the test is collected before its single
//! final assertion.
//!
//! Escapes: `\n` is LF (0x0A), `\\` is one backslash, `\u{a0}` is U+00A0 (C2 A0) and `\u{feff}` is
//! U+FEFF (EF BB BF). No input has a final LF. Positions are not compared.

use markdown::mdast::Node;

/// What one input gives for the `lang` of its first Code node.
#[derive(Debug, PartialEq)]
enum Observed {
    /// The parse failed.
    ParseError,
    /// The parse worked but the tree has no Code node.
    NoCode,
    /// The `lang` of the first Code node.
    Lang(Option<String>),
}

/// The first Code node in document order.
fn first_code(node: &Node) -> Option<&markdown::mdast::Code> {
    if let Node::Code(code) = node {
        return Some(code);
    }
    node.children().into_iter().flatten().find_map(first_code)
}

/// The `lang` of the first Code node of one input through the trial entry.
fn observe(source: &str) -> Observed {
    match markdown_destination::parse(source) {
        Err(_) => Observed::ParseError,
        Ok(tree) => {
            first_code(&tree).map_or(Observed::NoCode, |code| Observed::Lang(code.lang.clone()))
        }
    }
}

fn word(value: &str) -> Observed {
    Observed::Lang(Some(value.to_string()))
}

#[test]
fn fence_word_is_the_original_label() {
    // C1, 16 B: 6060606126616d703b620a780a606060
    // ORIGINAL: info `a&amp;b`, word `a&amp;b` (the entity is not decoded).
    let entity = "```a&amp;b\nx\n```";
    // C2, 13 B: 606060615c2a620a780a606060
    // ORIGINAL: info `a\*b`, word `a\*b` (the escape is not decoded).
    let backslash_escape = "```a\\*b\nx\n```";
    // C3, 15 B: 60606061c2a06220630a780a606060
    // ORIGINAL: info `a`, U+00A0, `b c`; word `a` (U+00A0 splits the word).
    let no_break_space_splits = "```a\u{a0}b c\nx\n```";
    // C4, 13 B: 606060c2a020620a780a606060
    // ORIGINAL: info U+00A0, space, `b`; word `b` (a leading U+00A0 is trimmed).
    let no_break_space_leads = "```\u{a0} b\nx\n```";
    // C5, 18 B: 6060606426233130353b66660a780a606060
    // ORIGINAL: info `d&#105;ff`, word `d&#105;ff` (not `diff`).
    let numeric_entity = "```d&#105;ff\nx\n```";
    // C6, 15 B: 6060607275737420780a780a606060
    // ORIGINAL: info `rust x`, word `rust`.
    let word_then_meta = "```rust x\nx\n```";
    // C7, 9 B: 6060600a780a606060
    // ORIGINAL: info empty, word empty (no label).
    let no_info = "```\nx\n```";
    // C8, 11 B: 7e7e7e70790a780a7e7e7e
    // ORIGINAL: tilde fence, info `py`, word `py`.
    let tilde_fence = "~~~py\nx\n~~~";
    // C9, 14 B: 60606061efbbbf620a780a606060
    // ORIGINAL: info `a`, U+FEFF, `b`; word `a` (U+FEFF splits the word).
    let byte_order_mark_splits = "```a\u{feff}b\nx\n```";

    let observed = vec![
        ("entity", observe(entity)),
        ("backslash_escape", observe(backslash_escape)),
        ("no_break_space_splits", observe(no_break_space_splits)),
        ("no_break_space_leads", observe(no_break_space_leads)),
        ("numeric_entity", observe(numeric_entity)),
        ("word_then_meta", observe(word_then_meta)),
        ("no_info", observe(no_info)),
        ("tilde_fence", observe(tilde_fence)),
        ("byte_order_mark_splits", observe(byte_order_mark_splits)),
    ];

    assert_eq!(
        observed,
        vec![
            ("entity", word("a&amp;b")),
            ("backslash_escape", word("a\\*b")),
            ("no_break_space_splits", word("a")),
            ("no_break_space_leads", word("b")),
            ("numeric_entity", word("d&#105;ff")),
            ("word_then_meta", word("rust")),
            ("no_info", Observed::Lang(None)),
            ("tilde_fence", word("py")),
            ("byte_order_mark_splits", word("a")),
        ],
    );
}
