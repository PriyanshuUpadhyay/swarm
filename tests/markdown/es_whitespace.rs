//! Public-API regression for ECMAScript-whitespace reference matching (Unit12b, ES-WHITESPACE-V2 section 6).
//!
//! Expected values come from the ORIGINAL renderer (independent capture, Unit12b original-capture/report.md)
//! and the contract, never from candidate output. Only public returned values are compared.
//! Every public outcome of a test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A) and `\u{a0}` is U+00A0 (UTF-8 bytes C2 A0). No input has a final LF.

use markdown::ParseOptions;
use markdown::mdast::Node;

/// Walk the tree and collect Definition and LinkReference identifiers in document order.
fn collect(node: &Node, definitions: &mut Vec<String>, references: &mut Vec<String>) {
    match node {
        Node::Definition(definition) => definitions.push(definition.identifier.clone()),
        Node::LinkReference(reference) => references.push(reference.identifier.clone()),
        _ => {}
    }
    for child in node.children().into_iter().flatten() {
        collect(child, definitions, references);
    }
}

/// `(definition identifiers, link-reference identifiers)` of one tree.
fn identifiers(tree: &Node) -> (Vec<String>, Vec<String>) {
    let (mut definitions, mut references) = (Vec::new(), Vec::new());
    collect(tree, &mut definitions, &mut references);
    (definitions, references)
}

/// One input through the trial Some path: the link-reference identifiers and the first Definition identifier.
/// A parse error becomes `None`; no message is compared.
fn through_parse(source: &str) -> Option<(Vec<String>, Option<String>)> {
    let tree = swarm::view::transcript::markdown::parse(source).ok()?;
    let (definitions, references) = identifiers(&tree);
    Some((references, definitions.first().cloned()))
}

#[test]
fn a_reference_binds_the_definition_that_the_original_binds() {
    // P1, 31 B: 5b6120625d0a0a5b61c2a0625d3a2068747470733a2f2f652e746573742f64
    // ORIGINAL: one link, href https://e.test/d.
    let p1 = "[a b]\n\n[a\u{a0}b]: https://e.test/d";
    // P2, 30 B: 5b785d0a0a5bc2a078c2a05d3a2068747470733a2f2f652e746573742f61
    // ORIGINAL: one link, href https://e.test/a.
    let p2 = "[x]\n\n[\u{a0}x\u{a0}]: https://e.test/a";
    // P3, 55 B: 5b6120625d0a0a5b61c2a0625d3a2068747470733a2f2f652e746573742f310a5b6120625d3a2068747470733a2f2f652e746573742f32
    // ORIGINAL: one link, href https://e.test/1, the first Definition.
    let p3 = "[a b]\n\n[a\u{a0}b]: https://e.test/1\n[a b]: https://e.test/2";

    let outcomes = vec![through_parse(p1), through_parse(p2), through_parse(p3)];

    // Each input has one LinkReference whose identifier equals the identifier of the bound (first) Definition.
    let bound =
        |identifier: &str| Some((vec![identifier.to_string()], Some(identifier.to_string())));
    assert_eq!(
        outcomes,
        vec![bound("a\u{a0}b"), bound("\u{a0}x\u{a0}"), bound("a\u{a0}b")],
    );
}

#[test]
fn the_default_mode_still_leaves_a_whitespace_mismatch_unresolved() {
    // G1 = P1, 31 B: 5b6120625d0a0a5b61c2a0625d3a2068747470733a2f2f652e746573742f64
    // Default options and `to_html` keep today's behavior: no LinkReference and no link.
    let source = "[a b]\n\n[a\u{a0}b]: https://e.test/d";

    let tree = markdown::to_mdast(source, &ParseOptions::default())
        .map(|tree| identifiers(&tree).1)
        .map_err(|error| error.to_string());
    let has_link = markdown::to_html(source).contains("<a");

    assert_eq!((tree, has_link), (Ok(Vec::new()), false));
}
