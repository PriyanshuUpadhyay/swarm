//! Public-API regression for line-ending normalization in the producer entry (Step 5(g) slice G3).
//!
//! ORIGINAL (markdown-it) turns every CRLF and every lone CR into LF before it parses, so no CR reaches
//! a paragraph or a code block. Expected values come from the independent ORIGINAL capture
//! (`g3-original`, `breaks` false) and the contract, never from candidate output. Only public returned
//! values are compared: the Text values of each Paragraph and each Code value, in document order.
//! Every public outcome of the test is collected before its single final assertion.
//!
//! Escapes: `\r` is CR (0x0D), `\n` is LF (0x0A), `\0` is U+0000 (byte 00) and `\u{fffd}` is U+FFFD
//! (EF BF BD). No input has a final LF. Positions are not compared.

use markdown::mdast::Node;

/// One block of the output, as ORIGINAL paints it.
#[derive(Debug, PartialEq)]
enum Block {
    /// The concatenated Text values below one Paragraph.
    Paragraph(String),
    /// The value of one Code block.
    Code(String),
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

/// Collect each Paragraph text and each Code value in document order.
fn blocks(node: &Node, out: &mut Vec<Block>) {
    match node {
        Node::Paragraph(_) => {
            let mut visible = String::new();
            text(node, &mut visible);
            out.push(Block::Paragraph(visible));
        }
        Node::Code(code) => out.push(Block::Code(code.value.clone())),
        _ => {
            for child in node.children().into_iter().flatten() {
                blocks(child, out);
            }
        }
    }
}

/// The blocks of one input through the trial entry. A parse error becomes `None`.
fn outcome(source: &str) -> Option<Vec<Block>> {
    let tree = swarm::view::transcript::markdown::parse(source).ok()?;
    let mut found = Vec::new();
    blocks(&tree, &mut found);
    Some(found)
}

fn paragraph(value: &str) -> Block {
    Block::Paragraph(value.to_string())
}

fn code(value: &str) -> Block {
    Block::Code(value.to_string())
}

#[test]
fn cr_and_crlf_become_lf_as_in_the_original() {
    // L1, 4 B: 610d0a62
    // ORIGINAL: one paragraph with the text `a`, LF, `b`.
    let l1 = "a\r\nb";
    // L2, 3 B: 610d62
    // ORIGINAL: the same paragraph as L1.
    let l2 = "a\rb";
    // L3, 14 B: 6060600d0a780d0a790d0a606060
    // ORIGINAL: one code block with the content x, LF, y, LF; markdown-it keeps that last LF in the fence
    // content, and mdast Code.value drops it, so the value is x, LF, y.
    let l3 = "```\r\nx\r\ny\r\n```";
    // L4, 5 B: 61000d0a62
    // ORIGINAL: one paragraph with the text `a`, U+FFFD, LF, `b`.
    let l4 = "a\0\r\nb";

    let outcomes = vec![outcome(l1), outcome(l2), outcome(l3), outcome(l4)];

    assert_eq!(
        outcomes,
        vec![
            Some(vec![paragraph("a\nb")]),
            Some(vec![paragraph("a\nb")]),
            Some(vec![code("x\ny")]),
            Some(vec![paragraph("a\u{fffd}\nb")]),
        ],
    );
}
