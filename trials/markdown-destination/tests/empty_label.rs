//! Public-API regression for definitions whose label folds to empty (Unit12c, EMPTY-LABEL-V3 section 5).
//!
//! Expected values come from the ORIGINAL renderer (independent capture, u12cd-original report) and the
//! contract, never from candidate output. Only public returned values are compared.
//! Every public outcome of the test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A), `\u{a0}` is U+00A0 (UTF-8 bytes C2 A0) and `\u{3000}` is U+3000 (E3 80 80).
//! No input has a final LF. ORIGINAL turns a bare URL into a link (linkify); that is out of scope here,
//! so only the visible text of each paragraph is compared.

use markdown::mdast::Node;

/// Collect the Definition identifiers in document order.
fn definitions(node: &Node, out: &mut Vec<String>) {
    if let Node::Definition(definition) = node {
        out.push(definition.identifier.clone());
    }
    for child in node.children().into_iter().flatten() {
        definitions(child, out);
    }
}

/// Concatenate the Text values below one node, descending into inline children.
fn text(node: &Node, out: &mut String) {
    if let Node::Text(value) = node {
        out.push_str(&value.value);
    }
    for child in node.children().into_iter().flatten() {
        text(child, out);
    }
}

/// The visible text of each Paragraph in document order.
fn paragraphs(node: &Node, out: &mut Vec<String>) {
    if let Node::Paragraph(_) = node {
        let mut visible = String::new();
        text(node, &mut visible);
        out.push(visible);
    }
    for child in node.children().into_iter().flatten() {
        paragraphs(child, out);
    }
}

/// `(Definition identifiers, visible text per Paragraph)` of one input through the trial entry.
/// A parse error becomes `None`; no message is compared.
fn through_parse(source: &str) -> Option<(Vec<String>, Vec<String>)> {
    let tree = markdown_destination::parse(source).ok()?;
    let (mut found, mut visible) = (Vec::new(), Vec::new());
    definitions(&tree, &mut found);
    paragraphs(&tree, &mut visible);
    Some((found, visible))
}

#[test]
fn a_definition_with_an_empty_folded_label_stays_text_as_in_the_original() {
    // E1, 28 B: 5bc2a05d0a0a5bc2a05d3a2068747470733a2f2f652e746573742f75
    // ORIGINAL: no reference link, no Definition; paragraph 1 text `[U+00A0]`, paragraph 2 text
    // `[U+00A0]: https://e.test/u` (the URL is an auto-link there).
    let e1 = "[\u{a0}]\n\n[\u{a0}]: https://e.test/u";
    // E2s, 23 B: 5be380805d3a2068747470733a2f2f652e746573742f76
    // ORIGINAL: no Definition; one paragraph with text `[U+3000]: https://e.test/v`.
    let e2s = "[\u{3000}]: https://e.test/v";

    let outcomes = vec![through_parse(e1), through_parse(e2s)];

    assert_eq!(
        outcomes,
        vec![
            Some((
                Vec::new(),
                vec![
                    "[\u{a0}]".to_string(),
                    "[\u{a0}]: https://e.test/u".to_string(),
                ],
            )),
            Some((Vec::new(), vec!["[\u{3000}]: https://e.test/v".to_string()])),
        ],
    );
}
