//! Code heads and table copy sources in document order.

use markdown::mdast::Node;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Code { label: String },
    Table { source: String },
}

/// Collect fenced code labels and original table source lines at any depth.
///
/// # Errors
///
/// Returns the parser's original diagnostic if parsing fails.
pub fn heads(source: &str) -> Result<Vec<Head>, markdown::message::Message> {
    let tree = crate::parse(source)?;
    let offset_text = crate::prelude(source);
    let mut pending = vec![&tree];
    let mut heads = Vec::new();

    while let Some(node) = pending.pop() {
        match node {
            Node::Code(code)
                if code.position.as_ref().is_some_and(|position| {
                    matches!(
                        offset_text.as_bytes().get(position.start.offset),
                        Some(b'`' | b'~')
                    )
                }) =>
            {
                heads.push(Head::Code {
                    label: code.lang.clone().unwrap_or_else(|| "code".to_owned()),
                });
            }
            Node::Table(table) => {
                if let Some(position) = &table.position {
                    let start = position.start.line.saturating_sub(1);
                    // Keep raw CR and NUL bytes; parser offsets use the prelude text.
                    let source = source
                        .split('\n')
                        .skip(start)
                        .take(position.end.line.saturating_sub(start))
                        .collect::<Vec<_>>()
                        .join("\n");
                    heads.push(Head::Table { source });
                }
            }
            _ => {}
        }

        if let Some(children) = node.children() {
            pending.extend(children.iter().rev());
        }
    }

    Ok(heads)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Display {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        depth: u8,
        children: Vec<Inline>,
    },
    ThematicBreak,
    Blockquote(Vec<Block>),
    List {
        ordered: bool,
        start: Option<u32>,
        tight: bool,
        items: Vec<Vec<Block>>,
    },
    Code {
        text: String,
    },
    Later(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Delete(Vec<Inline>),
    Code(String),
    Break,
    Link {
        url: String,
        title: Option<String>,
        children: Vec<Inline>,
    },
    Later(&'static str),
}

/// Map parsed blocks and inlines to their display values.
///
/// # Errors
///
/// Returns the parser's original diagnostic if parsing fails.
pub fn display(source: &str) -> Result<Display, markdown::message::Message> {
    let tree = crate::parse(source)?;
    let offset_text = crate::prelude(source);
    let children = tree.children().map_or(&[][..], Vec::as_slice);
    Ok(Display {
        blocks: map_blocks(children, &offset_text),
    })
}

fn map_blocks(nodes: &[Node], source: &str) -> Vec<Block> {
    nodes
        .iter()
        .filter_map(|node| {
            Some(match node {
                Node::Paragraph(paragraph) => Block::Paragraph(map_inlines(&paragraph.children)),
                Node::Heading(heading) => Block::Heading {
                    depth: heading.depth,
                    children: map_inlines(&heading.children),
                },
                Node::ThematicBreak(_) => Block::ThematicBreak,
                Node::Blockquote(quote) => Block::Blockquote(map_blocks(&quote.children, source)),
                Node::List(list) => Block::List {
                    ordered: list.ordered,
                    start: list.ordered.then(|| list.start.unwrap_or(1)),
                    tight: !(list.spread
                        || list
                            .children
                            .iter()
                            .any(|item| matches!(item, Node::ListItem(item) if item.spread))),
                    items: list
                        .children
                        .iter()
                        .filter_map(|item| match item {
                            Node::ListItem(item) => Some(map_blocks(&item.children, source)),
                            _ => None,
                        })
                        .collect(),
                },
                Node::Code(code) => Block::Code {
                    text: code_text(code, source),
                },
                Node::Table(_) => Block::Later("table"),
                Node::Definition(_) => return None,
                _ => Block::Later("block"),
            })
        })
        .collect()
}

fn map_inlines(nodes: &[Node]) -> Vec<Inline> {
    let mut inlines = Vec::new();
    for node in nodes {
        let inline = match node {
            Node::Text(text) => Inline::Text(text.value.clone()),
            Node::Emphasis(emphasis) => Inline::Emphasis(map_inlines(&emphasis.children)),
            Node::Strong(strong) => Inline::Strong(map_inlines(&strong.children)),
            Node::Delete(delete) => Inline::Delete(map_inlines(&delete.children)),
            Node::InlineCode(code) => Inline::Code(code.value.clone()),
            Node::Break(_) => Inline::Break,
            Node::Link(link) => Inline::Link {
                url: link.url.clone(),
                title: link.title.clone(),
                children: map_inlines(&link.children),
            },
            Node::LinkReference(_) => Inline::Later("reference"),
            _ => Inline::Later("inline"),
        };
        inlines.push(inline);
    }
    inlines
}

fn code_text(code: &markdown::mdast::Code, source: &str) -> String {
    let mut text = code.value.clone();
    let Some(position) = &code.position else {
        return text;
    };
    let Some(raw) = source.get(position.start.offset..position.end.offset) else {
        return text;
    };
    if !matches!(raw.as_bytes().first(), Some(b'`' | b'~')) {
        // Drop trailing blank lines, but keep spaces within all other lines.
        while text
            .rsplit('\n')
            .next()
            .is_some_and(|line| line.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
        {
            let Some(last_lf) = text.rfind('\n') else {
                text.clear();
                break;
            };
            text.truncate(last_lf);
        }
        text.push('\n');
        return text;
    }

    // The parser removes the opening LF and one final LF from fenced values.
    // Position lines distinguish an empty fence from a fence with a blank line.
    let line_breaks = position.end.line.saturating_sub(position.start.line);
    let has_content_line = !text.is_empty() || line_breaks > 1;
    let value_lines = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
    // A closing fence occupies one more line than an unterminated content value.
    let closed = !raw.ends_with('\n') && line_breaks > value_lines;
    let ends_without_lf = position.end.offset == source.len() && !raw.ends_with('\n');
    if has_content_line && (closed || !ends_without_lf) {
        text.push('\n');
    }
    text
}
