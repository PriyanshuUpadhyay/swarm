//! Public-API regression for the GFM table and strikethrough constructs of the producer entry
//! (Step 5(g) slice G2).
//!
//! ORIGINAL (markdown-it with its default table and strikethrough rules, and no footnote rule) builds a
//! table from a pipe row plus a delimiter row, strikes `~~a~~` and keeps `~a~` as text. Expected values
//! come from the independent ORIGINAL capture (`g2-original`, `breaks` false, table cells from the
//! `th`/`td` elements only) and the contract, never from candidate output. Only public returned values
//! are compared: node kinds, Text values, table alignment, link destinations and link text.
//! Every public outcome of a test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A). No input has a final LF. Positions are not compared.

use markdown::mdast::{AlignKind, Node};

/// Column alignment of a table, as the delimiter row gives it.
#[derive(Debug, PartialEq)]
enum Align {
    None,
    Left,
    Right,
    Center,
}

/// One table: its align list and the cell texts of each row, header first.
type TableOutcome = (Vec<Align>, Vec<Vec<String>>);

/// One inline piece of a paragraph, as ORIGINAL paints it.
#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    Delete(String),
    Link {
        href: String,
        label: String,
    },
    /// Any other node kind. It never equals an expected piece.
    Other,
    /// A LinkReference with no Definition of the same identifier.
    Unresolved,
}

fn text_piece(value: &str) -> Piece {
    Piece::Text(value.to_string())
}

fn link_piece(href: &str, label: &str) -> Piece {
    Piece::Link {
        href: href.to_string(),
        label: label.to_string(),
    }
}

/// Concatenate the Text values below one node, descending into inline children.
fn text(node: &Node) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out
}

fn collect_text(node: &Node, out: &mut String) {
    if let Node::Text(value) = node {
        out.push_str(&value.value);
    }
    for child in node.children().into_iter().flatten() {
        collect_text(child, out);
    }
}

/// Whether any node below (or at) `node` satisfies `kind`.
fn contains(node: &Node, kind: fn(&Node) -> bool) -> bool {
    kind(node)
        || node
            .children()
            .into_iter()
            .flatten()
            .any(|child| contains(child, kind))
}

fn is_footnote(node: &Node) -> bool {
    matches!(
        node,
        Node::FootnoteReference(_) | Node::FootnoteDefinition(_)
    )
}

fn align_of(kind: &AlignKind) -> Align {
    match kind {
        AlignKind::Left => Align::Left,
        AlignKind::Right => Align::Right,
        AlignKind::Center => Align::Center,
        AlignKind::None => Align::None,
    }
}

/// Every Table in document order, as `(align list, the cell texts of each row, header first)`.
fn tables(node: &Node, out: &mut Vec<TableOutcome>) {
    if let Node::Table(table) = node {
        let rows = table
            .children
            .iter()
            .map(|row| {
                row.children()
                    .into_iter()
                    .flatten()
                    .map(text)
                    .collect::<Vec<String>>()
            })
            .collect();
        out.push((table.align.iter().map(align_of).collect(), rows));
        return;
    }
    for child in node.children().into_iter().flatten() {
        tables(child, out);
    }
}

/// The inline pieces of the first Paragraph at the top level. Adjacent Text values are joined. A
/// LinkReference is resolved through the Definition with the same identifier; identifiers are compared,
/// no case form is hard-coded.
fn first_paragraph_pieces(root: &Node) -> Vec<Piece> {
    let Node::Root(root) = root else {
        return vec![Piece::Other];
    };
    let Some(Node::Paragraph(paragraph)) = root
        .children
        .iter()
        .find(|block| matches!(block, Node::Paragraph(_)))
    else {
        return vec![Piece::Other];
    };
    let mut pieces: Vec<Piece> = Vec::new();
    for child in &paragraph.children {
        match child {
            Node::Text(inner) => {
                if let Some(Piece::Text(previous)) = pieces.last_mut() {
                    previous.push_str(&inner.value);
                } else {
                    pieces.push(text_piece(&inner.value));
                }
            }
            Node::Delete(_) => pieces.push(Piece::Delete(text(child))),
            Node::Link(inner) => pieces.push(link_piece(&inner.url, &text(child))),
            Node::LinkReference(inner) => {
                let definition = root.children.iter().find_map(|block| match block {
                    Node::Definition(found) if found.identifier == inner.identifier => {
                        Some(found.url.as_str())
                    }
                    _ => None,
                });
                pieces.push(match definition {
                    Some(href) => link_piece(href, &text(child)),
                    None => Piece::Unresolved,
                });
            }
            _ => pieces.push(Piece::Other),
        }
    }
    pieces
}

/// The tables of one input through the trial entry. A parse error becomes `None`.
fn table_outcome(source: &str) -> Option<Vec<TableOutcome>> {
    let tree = markdown_destination::parse(source).ok()?;
    let mut found = Vec::new();
    tables(&tree, &mut found);
    Some(found)
}

/// The pieces of the first Paragraph of one input through the trial entry. A parse error becomes `None`.
fn piece_outcome(source: &str) -> Option<Vec<Piece>> {
    let tree = markdown_destination::parse(source).ok()?;
    Some(first_paragraph_pieces(&tree))
}

/// `(any footnote node, the pieces of the first Paragraph)` of one input through the trial entry. A
/// parse error becomes `None`.
fn footnote_outcome(source: &str) -> Option<(bool, Vec<Piece>)> {
    let tree = markdown_destination::parse(source).ok()?;
    Some((contains(&tree, is_footnote), first_paragraph_pieces(&tree)))
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|cell| cell.to_string()).collect()
}

#[test]
fn tables_build_as_in_the_original() {
    // TB1, 31 B: 7c2061207c2062207c0a7c203a2d207c202d3a207c0a7c2063207c2064207c
    // ORIGINAL: one table; header `a` (left), `b` (right); body row `c` (left), `d` (right).
    let tb1 = "| a | b |\n| :- | -: |\n| c | d |";
    // TB2, 21 B: 61207c20620a2d2d2d207c202d2d2d0a63207c2064
    // ORIGINAL: one table; header `a`, `b`; body row `c`, `d`; no alignment.
    let tb2 = "a | b\n--- | ---\nc | d";

    let outcomes = vec![table_outcome(tb1), table_outcome(tb2)];

    assert_eq!(
        outcomes,
        vec![
            Some(vec![(
                vec![Align::Left, Align::Right],
                vec![row(&["a", "b"]), row(&["c", "d"])],
            )]),
            Some(vec![(
                vec![Align::None, Align::None],
                vec![row(&["a", "b"]), row(&["c", "d"])],
            )]),
        ],
    );
}

#[test]
fn double_tilde_strikes_and_single_tilde_stays_text_as_in_the_original() {
    // ST1, 5 B: 7e7e617e7e
    // ORIGINAL: one paragraph holding a strike (`<s>`) with the text `a`.
    let st1 = "~~a~~";
    // ST2, 3 B: 7e617e
    // ORIGINAL: one paragraph with the plain text `~a~`.
    let st2 = "~a~";

    let outcomes = vec![piece_outcome(st1), piece_outcome(st2)];

    assert_eq!(
        outcomes,
        vec![
            Some(vec![Piece::Delete("a".to_string())]),
            Some(vec![text_piece("~a~")]),
        ],
    );
}

#[test]
fn footnote_syntax_stays_off_as_in_the_original() {
    // FN1, 29 B: 615b5e315d0a0a5b5e315d3a2068747470733a2f2f652e746573742f66
    // ORIGINAL: one paragraph: the text `a`, then a link `^1` to https://e.test/f. The definition line
    // paints nothing; there is no footnote markup.
    let fn1 = "a[^1]\n\n[^1]: https://e.test/f";

    let outcome = footnote_outcome(fn1);

    assert_eq!(
        outcome,
        Some((
            false,
            vec![text_piece("a"), link_piece("https://e.test/f", "^1")]
        )),
    );
}
