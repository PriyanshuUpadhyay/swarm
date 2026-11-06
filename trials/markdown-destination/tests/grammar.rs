//! Destination grammar checked against the ORIGINAL `renderMarkdown`, one test per decision group.
//!
//! Expected values come only from the sealed ORIGINAL captures of lane C round 1
//! (`capture-1.stdout` = `capture-2.stdout`, sha256 8afdb30f06bf0570acc605e18f91879b179dde7a91b123aa8cd0e8cea3a19652).
//! `observe` reads the public parse tree only and never repairs a destination. It is valid for the
//! simple paragraph and top-level definition inputs below. The ORIGINAL returns HTML, so the span
//! check asserts the public AST end offset on the raw source bytes, not an ORIGINAL position.
//! Not covered here, by design: bare-URL linkify of the rejected depth-33 line, and the NUL child text.

use markdown::mdast::Node;
use markdown_destination::parse;

#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    Link { href: String, label: String },
}

fn text(value: &str) -> Piece {
    Piece::Text(value.to_owned())
}

fn link(href: &str, label: &str) -> Piece {
    Piece::Link {
        href: href.to_owned(),
        label: label.to_owned(),
    }
}

fn label_of(children: &[Node]) -> String {
    children
        .iter()
        .map(|child| match child {
            Node::Text(inner) => inner.value.clone(),
            other => format!("{other:?}"),
        })
        .collect()
}

fn pieces(children: &[Node], blocks: &[Node]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    for child in children {
        match child {
            Node::Text(inner) => {
                if let Some(Piece::Text(previous)) = out.last_mut() {
                    previous.push_str(&inner.value);
                } else {
                    out.push(text(&inner.value));
                }
            }
            Node::Link(inner) => out.push(link(&inner.url, &label_of(&inner.children))),
            Node::LinkReference(inner) => {
                let definition = blocks.iter().find_map(|block| match block {
                    Node::Definition(found) if found.identifier == inner.identifier => {
                        Some(found.url.as_str())
                    }
                    _ => None,
                });
                out.push(match definition {
                    Some(href) => link(href, &label_of(&inner.children)),
                    None => text(&format!("unresolved reference {:?}", inner.identifier)),
                });
            }
            other => out.push(text(&format!("unexpected {other:?}"))),
        }
    }
    out
}

/// One entry per paragraph. A definition the parser accepts shows up only through its references.
fn observe_node(node: Node) -> Vec<Vec<Piece>> {
    let Node::Root(root) = node else {
        panic!("parser returned no root");
    };
    root.children
        .iter()
        .filter_map(|block| match block {
            Node::Paragraph(paragraph) => Some(pieces(&paragraph.children, &root.children)),
            Node::Definition(_) => None,
            other => Some(vec![text(&format!("unexpected {other:?}"))]),
        })
        .collect()
}

fn observe(source: &str) -> Vec<Vec<Piece>> {
    observe_node(parse(source).expect("source parses"))
}

/// Source byte range of the only link in a one-paragraph tree, from the public AST positions.
fn link_span(node: &Node) -> Option<(usize, usize)> {
    let Node::Root(root) = node else { return None };
    let [Node::Paragraph(paragraph)] = root.children.as_slice() else {
        return None;
    };
    let [Node::Link(found)] = paragraph.children.as_slice() else {
        return None;
    };
    let position = found.position.as_ref()?;
    Some((position.start.offset, position.end.offset))
}

fn assert_original_outcomes(cases: Vec<(&str, Vec<Vec<Piece>>)>) {
    let failed: Vec<String> = cases
        .into_iter()
        .filter_map(|(source, expected)| {
            let actual = observe(source);
            (actual != expected)
                .then(|| format!("{source:?}\n  expected {expected:?}\n  actual   {actual:?}"))
        })
        .collect();
    assert!(
        failed.is_empty(),
        "{} case(s) differ from ORIGINAL:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

fn https_only(value: &str) -> bool {
    value.starts_with("https://")
}

fn depth_source(depth: usize) -> String {
    format!(
        "[d]: https://e.test/{}x{}\n\n[d]",
        "(".repeat(depth),
        ")".repeat(depth)
    )
}

#[test]
fn defined_label_before_a_premature_end_stays_text() {
    // ORIGINAL `<p>[a](</p>`: no link, and the shortcut reference for `a` is not used.
    assert_original_outcomes(vec![(
        "[a]: https://e.test/ref\n\n[a](",
        vec![vec![text("[a](")]],
    )]);
}

#[test]
fn escaped_control_tab_and_del_are_accepted_in_a_raw_destination() {
    // ORIGINAL keeps the backslash and encodes the escaped byte.
    assert_original_outcomes(vec![
        (
            "[x](https://e.test/a\\\u{1}b)",
            vec![vec![link("https://e.test/a%5C%01b", "x")]],
        ),
        (
            "[x](https://e.test/a\\\tb)",
            vec![vec![link("https://e.test/a%5C%09b", "x")]],
        ),
        (
            "[x](https://e.test/a\\\u{7f}b)",
            vec![vec![link("https://e.test/a%5C%7Fb", "x")]],
        ),
    ]);
}

#[test]
fn definition_paren_depth_32_is_accepted_and_33_is_rejected() {
    // ORIGINAL: depth 32 defines `[d]`; at depth 33 the definition fails and the final `[d]` is text.
    let mut failed = Vec::new();
    let href = format!("https://e.test/{}x{}", "(".repeat(32), ")".repeat(32));
    let accepted = observe(&depth_source(32));
    if accepted != vec![vec![link(&href, "d")]] {
        failed.push(format!("depth 32 expected one link, actual {accepted:?}"));
    }
    let rejected = observe(&depth_source(33));
    if rejected.last() != Some(&vec![text("[d]")]) {
        failed.push(format!(
            "depth 33 expected final paragraph [d] as text, actual last {:?}",
            rejected.last()
        ));
    }
    assert!(
        failed.is_empty(),
        "{} case(s) differ from ORIGINAL:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

#[test]
fn angle_autolink_admits_del_and_nul_with_the_normalized_href() {
    // The link text is not asserted: ORIGINAL shows U+FFFD for NUL, which is a separate open gate.
    let mut failed = Vec::new();
    for (source, href) in [
        ("<https://e.test/a\u{7f}b>", "https://e.test/a%7Fb"),
        ("<https://e.test/a\0b>", "https://e.test/a%EF%BF%BDb"),
    ] {
        let node = parse(source).expect("source parses");
        let span = link_span(&node);
        let observed = observe_node(node);
        let href_ok = matches!(observed.as_slice(), [one] if matches!(one.as_slice(), [Piece::Link { href: got, .. }] if got == href));
        if !href_ok || span != Some((0, source.len())) {
            failed.push(format!(
                "{source:?} expected link {href:?} over bytes 0..{}, actual {observed:?} span {span:?}",
                source.len()
            ));
        }
    }
    // Hook activation by validation alone, normalizer absent: admission and span only.
    let source = "<https://e.test/a\u{7f}b>";
    let node = markdown::to_mdast(
        source,
        &markdown::ParseOptions {
            validate_link: Some(https_only),
            ..markdown::ParseOptions::default()
        },
    )
    .expect("source parses");
    let span = link_span(&node);
    if span != Some((0, source.len())) {
        failed.push(format!(
            "validate-only {source:?} expected a link over bytes 0..{}, actual span {span:?}",
            source.len()
        ));
    }
    assert!(
        failed.is_empty(),
        "{} case(s) differ from ORIGINAL:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
