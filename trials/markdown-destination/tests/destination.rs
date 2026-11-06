//! Destination admission and href normalization, checked against the ORIGINAL `renderMarkdown`.
//!
//! The expected values were captured from the ORIGINAL entry twice and sealed before any
//! baseline Rust ran. The seals and the raw captures are listed in the checker reports.
//! `observe` reads the parsed tree only. For a reference link it reads the `url` of the matching
//! `Definition` node that the parser produced. It never repairs a destination. It is valid only for
//! the simple paragraph and top-level definition cases below, not for nested or general documents.

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
fn observe(source: &str) -> Vec<Vec<Piece>> {
    let Node::Root(root) = parse(source).expect("source parses") else {
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

#[test]
fn accepted_destinations_keep_the_original_normalized_href() {
    assert_original_outcomes(vec![
        (
            "[doc](https://example.com/caf\u{e9}/a%20b?q=x)",
            vec![vec![link("https://example.com/caf%C3%A9/a%20b?q=x", "doc")]],
        ),
        // Enclosed resource destination `(<...>)`, not an unbracketed `<url>` autolink.
        (
            "[doc](<https://example.com/a b>)",
            vec![vec![link("https://example.com/a%20b", "doc")]],
        ),
        (
            "[docs]: https://example.com/caf\u{e9}\n\n[see][docs] and [docs]",
            vec![vec![
                link("https://example.com/caf%C3%A9", "see"),
                text(" and "),
                link("https://example.com/caf%C3%A9", "docs"),
            ]],
        ),
        (
            "[doc](HTTP://Example.COM/x)",
            vec![vec![link("HTTP://Example.COM/x", "doc")]],
        ),
        // Non-ASCII host: ORIGINAL converts it with the pinned punycode.js `toASCII`.
        (
            "[doc](https://b\u{fc}cher.example/x)",
            vec![vec![link("https://xn--bcher-kva.example/x", "doc")]],
        ),
        // Unbracketed `<url>` autolink. The sample keeps the display text equal to the source
        // text, so the separate display-text behavior cannot cause a difference.
        (
            "<https://example.com/caf\u{e9}?q=x>",
            vec![vec![link(
                "https://example.com/caf%C3%A9?q=x",
                "https://example.com/caf\u{e9}?q=x",
            )]],
        ),
        // A long numeric character reference and a named one inside an accepted destination.
        (
            "[doc](https://example.com/&#000000065;?x=&amp;)",
            vec![vec![link("https://example.com/A?x=&", "doc")]],
        ),
    ]);
}

#[test]
fn rejected_destinations_fall_back_to_text_or_the_shortcut_reference() {
    assert_original_outcomes(vec![
        (
            "[bad](javascript:void)",
            vec![vec![text("[bad](javascript:void)")]],
        ),
        (
            "[evil]: javascript:void\n\n[evil]",
            vec![vec![text("[evil]: javascript:void")], vec![text("[evil]")]],
        ),
        (
            "[ok]: https://example.com/ok\n\n[ok](javascript:void)",
            vec![vec![
                link("https://example.com/ok", "ok"),
                text("(javascript:void)"),
            ]],
        ),
    ]);
}
