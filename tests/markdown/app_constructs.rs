//! Public-API regression for the app constructs of the producer entry (Step 5(g) slice G1).
//!
//! ORIGINAL (markdown-it with `html: false` and the image rule disabled) paints raw HTML as text and
//! image syntax as `!` plus a link. Expected values come from the independent ORIGINAL capture
//! (`g1-original` and `g1-original-i3b`, `breaks` false) and the contract, never from candidate output.
//! Only public returned values are compared: node kinds, Text values, link destinations and link text.
//! Every public outcome of a test is collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A). No input has a final LF. Positions are not compared.

use markdown::mdast::Node;

/// One inline piece of a paragraph, as ORIGINAL paints it.
#[derive(Debug, PartialEq)]
enum Piece {
    Text(String),
    Link {
        href: String,
        label: String,
    },
    /// Any other node kind, by name. It never equals an expected piece.
    Other(&'static str),
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
fn text(node: &Node, out: &mut String) {
    if let Node::Text(value) = node {
        out.push_str(&value.value);
    }
    for child in node.children().into_iter().flatten() {
        text(child, out);
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

fn is_html(node: &Node) -> bool {
    matches!(node, Node::Html(_))
}

fn is_image(node: &Node) -> bool {
    matches!(node, Node::Image(_) | Node::ImageReference(_))
}

/// The text of each Paragraph, in document order.
fn paragraph_texts(node: &Node, out: &mut Vec<String>) {
    if let Node::Paragraph(_) = node {
        let mut visible = String::new();
        text(node, &mut visible);
        out.push(visible);
        return;
    }
    for child in node.children().into_iter().flatten() {
        paragraph_texts(child, out);
    }
}

/// The inline pieces of the first Paragraph at the top level. Adjacent Text values are joined. A
/// LinkReference is resolved through the Definition with the same identifier; identifiers are compared,
/// no case form is hard-coded.
fn first_paragraph_pieces(root: &Node) -> Vec<Piece> {
    let Node::Root(root) = root else {
        return vec![Piece::Other("not a root")];
    };
    let Some(Node::Paragraph(paragraph)) = root
        .children
        .iter()
        .find(|block| matches!(block, Node::Paragraph(_)))
    else {
        return vec![Piece::Other("no paragraph")];
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
            Node::Link(inner) => {
                let mut label = String::new();
                text(child, &mut label);
                pieces.push(link_piece(&inner.url, &label));
            }
            Node::LinkReference(inner) => {
                let definition = root.children.iter().find_map(|block| match block {
                    Node::Definition(found) if found.identifier == inner.identifier => {
                        Some(found.url.as_str())
                    }
                    _ => None,
                });
                let mut label = String::new();
                text(child, &mut label);
                pieces.push(match definition {
                    Some(href) => link_piece(href, &label),
                    None => Piece::Unresolved,
                });
            }
            Node::Image(_) => pieces.push(Piece::Other("image")),
            Node::ImageReference(_) => pieces.push(Piece::Other("image reference")),
            Node::Html(_) => pieces.push(Piece::Other("html")),
            _ => pieces.push(Piece::Other("other")),
        }
    }
    pieces
}

/// `(any Html node, the text of each Paragraph)` of one input through the trial entry. A parse error
/// becomes `None`.
fn html_outcome(source: &str) -> Option<(bool, Vec<String>)> {
    let tree = swarm::view::transcript::markdown::parse(source).ok()?;
    let mut paragraphs = Vec::new();
    paragraph_texts(&tree, &mut paragraphs);
    Some((contains(&tree, is_html), paragraphs))
}

/// `(any Image or ImageReference node, the pieces of the first Paragraph)` of one input through the
/// trial entry. A parse error becomes `None`.
fn image_outcome(source: &str) -> Option<(bool, Vec<Piece>)> {
    let tree = swarm::view::transcript::markdown::parse(source).ok()?;
    Some((contains(&tree, is_image), first_paragraph_pieces(&tree)))
}

#[test]
fn raw_html_stays_text_as_in_the_original() {
    // H1, 11 B: 6869203c623e783c2f623e
    // ORIGINAL: one paragraph with the text `hi <b>x</b>`.
    let h1 = "hi <b>x</b>";
    // H2, 14 B: 3c6469763e0a780a3c2f6469763e
    // ORIGINAL: one paragraph with the text `<div>`, LF, `x`, LF, `</div>`.
    let h2 = "<div>\nx\n</div>";
    // H3, 10 B: 3c212d2d2063202d2d3e
    // ORIGINAL: one paragraph with the text `<!-- c -->`.
    let h3 = "<!-- c -->";

    let outcomes = vec![html_outcome(h1), html_outcome(h2), html_outcome(h3)];

    assert_eq!(
        outcomes,
        vec![
            Some((false, vec!["hi <b>x</b>".to_string()])),
            Some((false, vec!["<div>\nx\n</div>".to_string()])),
            Some((false, vec!["<!-- c -->".to_string()])),
        ],
    );
}

#[test]
fn image_syntax_is_a_bang_and_a_link_as_in_the_original() {
    // I1, 22 B: 215b615d2868747470733a2f2f652e746573742f7529
    // ORIGINAL: `!` then a link `a` to https://e.test/u.
    let i1 = "![a](https://e.test/u)";
    // I2, 30 B: 215b615d5b725d0a0a5b725d3a2068747470733a2f2f652e746573742f75
    // ORIGINAL: the same HTML as I1; the definition line paints nothing.
    let i2 = "![a][r]\n\n[r]: https://e.test/u";
    // I3b, 28 B: 5b215b615d2868747470733a2f2f652e746573742f69295d282f6c29
    // ORIGINAL: `[!`, a link `a` to https://e.test/i, then the text `](/l)`; the outer link does not form.
    let i3b = "[![a](https://e.test/i)](/l)";

    let outcomes = vec![image_outcome(i1), image_outcome(i2), image_outcome(i3b)];

    assert_eq!(
        outcomes,
        vec![
            Some((
                false,
                vec![text_piece("!"), link_piece("https://e.test/u", "a")]
            )),
            Some((
                false,
                vec![text_piece("!"), link_piece("https://e.test/u", "a")]
            )),
            Some((
                false,
                vec![
                    text_piece("[!"),
                    link_piece("https://e.test/i", "a"),
                    text_piece("](/l)"),
                ]
            )),
        ],
    );
}
