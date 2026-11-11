//! Linked destination and title values, checked against the ORIGINAL `renderMarkdown`.
//!
//! Expected URL, title, definition and completion values come only from the sealed ORIGINAL captures
//! of lane C (roots `u11-lane-c-original-01` to `-06`: raw public HTML plus the pinned `parseLinkDestination` helper).
//! `observe` reads the public parse tree only and never repairs a value. It projects URL, title, definition
//! and reference facts, not the renderer HTML, so the bare-URL link a failed definition gets in the ORIGINAL is not selected.
//! The last test keeps the CURRENT public HTML of `to_html_with_options` unchanged. Its expected string was
//! observed from the existing Rust API before any change, not from the ORIGINAL.
//! Valid for the simple paragraph, blockquote and top-level definition inputs below, not for general documents.

use markdown::mdast::Node;
use markdown::{Options, ParseOptions, to_html_with_options, to_mdast};
use swarm::view::transcript::markdown::parse;

const ORIGIN: &str = "https://e.test/";

#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    Link {
        url: String,
        title: Option<String>,
        label: String,
    },
    Reference(String),
    Other(String),
}

#[derive(Debug, PartialEq)]
enum Block {
    Paragraph(Vec<Piece>),
    Definition { url: String, title: Option<String> },
    Quote(Vec<Block>),
    Other(String),
}

fn link(url: &str, title: Option<&str>, label: &str) -> Piece {
    Piece::Link {
        url: url.to_owned(),
        title: title.map(str::to_owned),
        label: label.to_owned(),
    }
}

fn text(value: &str) -> Piece {
    Piece::Text(value.to_owned())
}

fn definition(url: &str, title: Option<&str>) -> Block {
    Block::Definition {
        url: url.to_owned(),
        title: title.map(str::to_owned),
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

fn pieces(children: &[Node]) -> Vec<Piece> {
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
            Node::Link(inner) => out.push(Piece::Link {
                url: inner.url.clone(),
                title: inner.title.clone(),
                label: label_of(&inner.children),
            }),
            Node::LinkReference(inner) => out.push(Piece::Reference(label_of(&inner.children))),
            Node::Break(_) => out.push(Piece::Other("break".to_owned())),
            other => out.push(Piece::Other(format!("{other:?}"))),
        }
    }
    out
}

fn blocks(children: &[Node]) -> Vec<Block> {
    children
        .iter()
        .map(|child| match child {
            Node::Paragraph(inner) => Block::Paragraph(pieces(&inner.children)),
            Node::Definition(inner) => Block::Definition {
                url: inner.url.clone(),
                title: inner.title.clone(),
            },
            Node::Blockquote(inner) => Block::Quote(blocks(&inner.children)),
            other => Block::Other(format!("{other:?}")),
        })
        .collect()
}

fn root(node: Node) -> Vec<Block> {
    let Node::Root(root) = node else {
        panic!("parser returned no root");
    };
    blocks(&root.children)
}

fn observe(source: &str) -> Vec<Block> {
    root(parse(source).expect("source parses"))
}

fn keep(value: &str) -> String {
    value.to_owned()
}

fn https_only(value: &str) -> bool {
    value.starts_with("https://")
}

fn differences(cases: Vec<(String, Vec<Block>)>) -> Vec<String> {
    cases
        .into_iter()
        .filter_map(|(source, expected)| {
            let actual = observe(&source);
            (actual != expected)
                .then(|| format!("{source:?}\n  expected {expected:?}\n  actual   {actual:?}"))
        })
        .collect()
}

fn assert_no_differences(failed: Vec<String>) {
    assert!(
        failed.is_empty(),
        "{} case(s) differ from ORIGINAL:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

fn assert_original_outcomes(cases: Vec<(String, Vec<Block>)>) {
    assert_no_differences(differences(cases));
}

fn paragraph_link(url: &str, title: Option<&str>, label: &str) -> Vec<Block> {
    vec![Block::Paragraph(vec![link(url, title, label)])]
}

fn case(source: String, expected: Vec<Block>) -> (String, Vec<Block>) {
    (source, expected)
}

#[test]
fn resource_destination() {
    let url = "https://e.test/a%5C%0Ab";
    assert_original_outcomes(vec![
        // Backslash plus a physical line break stays in a raw destination: LF, CR and CRLF alike.
        case(
            format!("[x]({ORIGIN}a\\\nb)"),
            paragraph_link(url, None, "x"),
        ),
        case(
            format!("[x]({ORIGIN}a\\\rb)"),
            paragraph_link(url, None, "x"),
        ),
        case(
            format!("[x]({ORIGIN}a\\\r\nb)"),
            paragraph_link(url, None, "x"),
        ),
        // The same inside an enclosed destination.
        case(
            format!("[x](<{ORIGIN}a\\\nb>)"),
            paragraph_link(url, None, "x"),
        ),
        // The blockquote prefix of the continuation line is not part of the URL.
        case(
            format!("> [x]({ORIGIN}a\\\n> b)"),
            vec![Block::Quote(paragraph_link(url, None, "x"))],
        ),
        // Control: a backslash followed by the letter n is not a line break.
        case(
            format!("[x]({ORIGIN}a\\nb)"),
            paragraph_link("https://e.test/a%5Cnb", None, "x"),
        ),
        // An enclosed destination keeps the three spaces that follow the escaped line break (ORIGINAL root 08).
        case(
            format!("[a](<{ORIGIN}a\\\n   b>)"),
            paragraph_link("https://e.test/a%5C%0A%20%20%20b", None, "a"),
        ),
    ]);
}

fn failed_definition(source: &str) -> Result<(), String> {
    let found = observe(source);
    let [Block::Paragraph(first), Block::Paragraph(last)] = found.as_slice() else {
        return Err(format!(
            "expected two paragraphs and no definition: {found:?}"
        ));
    };
    let starts_as_text = first
        .iter()
        .map(|piece| match piece {
            Piece::Text(value) => value.as_str(),
            Piece::Link { label, .. } => label.as_str(),
            Piece::Reference(_) | Piece::Other(_) => "",
        })
        .collect::<String>()
        .starts_with("[d]: <https://e.test/a");
    let keeps_tail = matches!(first.last(), Some(Piece::Text(value)) if value.ends_with("b>"));
    if !starts_as_text || !keeps_tail || last != &vec![text("[d]")] {
        return Err(format!("definition was not rejected as text: {found:?}"));
    }
    Ok(())
}

#[test]
fn definition_boundary_and_completion() {
    let url = "https://e.test/a%5C";
    let mut failed: Vec<String> = Vec::new();
    let mut check = |ok: bool, what: String| {
        if !ok {
            failed.push(what);
        }
    };
    // A raw destination ends at the escaped line end: the next line is an ordinary paragraph.
    let actual = observe(&format!("[d]: {ORIGIN}a\\\nb\n\n[d]"));
    let expected = vec![
        definition(url, None),
        Block::Paragraph(vec![text("b")]),
        Block::Paragraph(vec![Piece::Reference("d".to_owned())]),
    ];
    check(actual == expected, format!("raw, next line b: {actual:?}"));
    // A quoted next line is neither part of the destination nor its title.
    let quoted = format!("[d]: {ORIGIN}a\\\n\"not a title\"\n\n[d]");
    let actual = observe(&quoted);
    let expected = vec![
        definition(url, None),
        Block::Paragraph(vec![text("\"not a title\"")]),
        Block::Paragraph(vec![Piece::Reference("d".to_owned())]),
    ];
    check(
        actual == expected,
        format!("raw, quoted next line: {actual:?}"),
    );
    // An enclosed destination cannot cross its line, with or without the backslash.
    for source in [
        format!("[d]: <{ORIGIN}a\\\nb>\n\n[d]"),
        format!("[d]: <{ORIGIN}a\nb>\n\n[d]"),
    ] {
        if let Err(why) = failed_definition(&source) {
            check(false, format!("{source:?}: {why}"));
        }
    }
    // A destination on the next line stays allowed.
    let actual = observe(&format!("[d]:\n{ORIGIN}a\n\n[d]"));
    let expected = vec![
        definition("https://e.test/a", None),
        Block::Paragraph(vec![Piece::Reference("d".to_owned())]),
    ];
    check(
        actual == expected,
        format!("destination on next line: {actual:?}"),
    );
    // The raw decoded value keeps the escaped terminal LF (pinned helper `parseLinkDestination`, `str` of 18 units).
    let options = ParseOptions {
        normalize_link: Some(keep),
        ..ParseOptions::default()
    };
    let actual = root(to_mdast(&quoted, &options).expect("source parses"));
    check(
        actual.first() == Some(&definition("https://e.test/a\\\n", None)),
        format!("identity normalizer value: {actual:?}"),
    );
    assert!(
        failed.is_empty(),
        "{} case(s) differ from ORIGINAL:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

#[test]
fn linked_titles() {
    let url = "https://e.test/a";
    assert_original_outcomes(vec![
        // Backslash plus a physical newline stays in the title: resource, definition and blockquote.
        case(
            format!("[x]({ORIGIN}a \"t\\\nu\")"),
            paragraph_link(url, Some("t\\\nu"), "x"),
        ),
        case(
            format!("[d]: {ORIGIN}a \"t\\\nu\"\n\n[d]"),
            vec![
                definition(url, Some("t\\\nu")),
                Block::Paragraph(vec![Piece::Reference("d".to_owned())]),
            ],
        ),
        case(
            format!("> [x]({ORIGIN}a \"t\\\n> u\")"),
            vec![Block::Quote(paragraph_link(url, Some("t\\\nu"), "x"))],
        ),
        // Role pair: the resource title keeps the three continuation spaces, the definition title drops them.
        case(
            format!("[a]({ORIGIN}a \"t\n   u\")"),
            paragraph_link(url, Some("t\n   u"), "a"),
        ),
        case(
            format!("[a]: {ORIGIN}a \"t\n   u\"\n\n[a]"),
            vec![
                definition(url, Some("t\nu")),
                Block::Paragraph(vec![Piece::Reference("a".to_owned())]),
            ],
        ),
        // Two spaces before a physical line ending stay in the title for both roles (ORIGINAL root 07).
        case(
            format!("[a]({ORIGIN}a \"t  \nu\")"),
            paragraph_link(url, Some("t  \nu"), "a"),
        ),
        case(
            format!("[d]: {ORIGIN}a \"t  \nu\"\n\n[d]"),
            vec![
                definition(url, Some("t  \nu")),
                Block::Paragraph(vec![Piece::Reference("d".to_owned())]),
            ],
        ),
    ]);
}

#[test]
fn physical_vs_generated() {
    let url = "https://e.test/a";
    let mut failed = differences(vec![
        // A physical CRLF becomes LF; a numeric reference to CR stays CR; a numeric LF stays in the destination.
        case(
            format!("[x]({ORIGIN}a \"t\r\nu\")"),
            paragraph_link(url, Some("t\nu"), "x"),
        ),
        case(
            format!("[x]({ORIGIN}a \"t&#13;u\")"),
            paragraph_link(url, Some("t\ru"), "x"),
        ),
        case(
            format!("[x]({ORIGIN}a&#10;b)"),
            paragraph_link("https://e.test/a%0Ab", None, "x"),
        ),
    ]);
    // Validate-only hook on a safe https URL: the long numeric title becomes one U+FFFD (no URL hook runs on a title).
    let options = ParseOptions {
        validate_link: Some(https_only),
        ..ParseOptions::default()
    };
    let source = format!("[a]({ORIGIN}b \"&#99999999999;\")");
    let actual = root(to_mdast(&source, &options).expect("source parses"));
    let expected = paragraph_link("https://e.test/b", Some("\u{fffd}"), "a");
    if actual != expected {
        failed.push(format!(
            "validate-only {source:?}\n  expected {expected:?}\n  actual   {actual:?}"
        ));
    }
    assert_no_differences(failed);
}

#[test]
fn html_scope_guard() {
    // Observed from the current public `to_html_with_options` before any change; the long numeric titles stay literal there.
    const DOCUMENT: &str = "[a](https://e.test/b \"&#99999999999;\")\n\n[d]: https://e.test/a \"&#99999999999;\"\n\n[d]\n\n[x](https://e.test/a\\\nb)";
    const HTML: &str = "<p><a href=\"https://e.test/b\" title=\"&amp;#99999999999;\">a</a></p>\n<p><a href=\"https://e.test/a\" title=\"&amp;#99999999999;\">d</a></p>\n<p>[x](https://e.test/a<br />\nb)</p>";
    let hooks = Options {
        parse: ParseOptions {
            normalize_link: Some(keep),
            validate_link: Some(https_only),
            ..ParseOptions::default()
        },
        ..Options::default()
    };
    let with_hooks = to_html_with_options(DOCUMENT, &hooks).expect("document compiles");
    let plain = to_html_with_options(DOCUMENT, &Options::default()).expect("document compiles");
    assert_eq!(with_hooks, HTML, "HTML with link hooks changed");
    assert_eq!(plain, HTML, "HTML without link hooks changed");
}
