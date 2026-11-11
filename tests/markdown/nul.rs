//! Public-API regression for NUL replacement in the producer entry (Unit12d, NUL-V3 section 4).
//!
//! Expected values come from the ORIGINAL renderer (independent capture, u12cd-original report) and the
//! contract, never from candidate output. Only public returned values are compared.
//! Every public outcome of the test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A), `\u{0}` is U+0000 (byte 00) and `\u{fffd}` is U+FFFD (EF BF BD).
//! No input has a final LF. Positions are not compared.

use markdown::mdast::Node;

/// Collect the InlineCode values in document order.
fn inline_code(node: &Node, out: &mut Vec<String>) {
    if let Node::InlineCode(code) = node {
        out.push(code.value.clone());
    }
    for child in node.children().into_iter().flatten() {
        inline_code(child, out);
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

/// Collect Definition identifiers, LinkReference identifiers and the text inside each LinkReference.
fn references(
    node: &Node,
    definitions: &mut Vec<String>,
    link_references: &mut Vec<String>,
    link_texts: &mut Vec<String>,
) {
    match node {
        Node::Definition(definition) => definitions.push(definition.identifier.clone()),
        Node::LinkReference(reference) => {
            link_references.push(reference.identifier.clone());
            let mut visible = String::new();
            text(node, &mut visible);
            link_texts.push(visible);
        }
        _ => {}
    }
    for child in node.children().into_iter().flatten() {
        references(child, definitions, link_references, link_texts);
    }
}

/// The InlineCode values of one input through the trial entry. A parse error becomes `None`.
fn code_values(source: &str) -> Option<Vec<String>> {
    let tree = swarm::view::transcript::markdown::parse(source).ok()?;
    let mut values = Vec::new();
    inline_code(&tree, &mut values);
    Some(values)
}

/// `(LinkReference count, Definition count, the two identifier lists are equal, text per LinkReference)`.
/// A parse error becomes `None`. The identifier case form is not hard-coded; the two lists are compared.
fn link_outcome(source: &str) -> Option<(usize, usize, bool, Vec<String>)> {
    let tree = swarm::view::transcript::markdown::parse(source).ok()?;
    let (mut definitions, mut link_references, mut link_texts) =
        (Vec::new(), Vec::new(), Vec::new());
    references(
        &tree,
        &mut definitions,
        &mut link_references,
        &mut link_texts,
    );
    Some((
        link_references.len(),
        definitions.len(),
        link_references == definitions,
        link_texts,
    ))
}

#[test]
fn a_nul_becomes_u_fffd_before_parsing_as_in_the_original() {
    // N1, 5 B: 6061006260
    // ORIGINAL: `<code>` content `a` U+FFFD `b` (bytes 61efbfbd62).
    let n1 = "`a\u{0}b`";
    // N2, 32 B: 5b6100625d0a0a5b61efbfbd625d3a2068747470733a2f2f652e746573742f78
    // ORIGINAL: one link to https://e.test/x with text `a` U+FFFD `b`; the reference binds the definition.
    let n2 = "[a\u{0}b]\n\n[a\u{fffd}b]: https://e.test/x";

    let outcomes = (code_values(n1), link_outcome(n2));

    assert_eq!(
        outcomes,
        (
            Some(vec!["a\u{fffd}b".to_string()]),
            Some((1, 1, true, vec!["a\u{fffd}b".to_string()])),
        ),
    );
}
