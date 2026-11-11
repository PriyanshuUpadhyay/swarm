//! The display tree for one Markdown text, with code heads and table copy sources.

use markdown::mdast::Node;

use super::super::display_patch;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Display {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
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
        label: Option<String>,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<serde_json::Value>,
    },
    Table {
        source: String,
        align: Vec<Align>,
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Later(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
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

/// The painter for a complete display tree. New modes may be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyMode {
    Plain,
    Tree,
}

/// Use a tree only when every block and inline can be painted.
pub fn body_mode(display: Option<&Display>) -> BodyMode {
    match display {
        Some(display) if display.blocks.iter().all(block_ready) => BodyMode::Tree,
        _ => BodyMode::Plain,
    }
}

fn block_ready(block: &Block) -> bool {
    match block {
        Block::Later(_) => false,
        Block::Paragraph(children) | Block::Heading { children, .. } => {
            children.iter().all(inline_ready)
        }
        Block::Blockquote(children) => children.iter().all(block_ready),
        Block::List { items, .. } => items.iter().flatten().all(block_ready),
        Block::Table { head, rows, .. } => head
            .iter()
            .chain(rows.iter().flatten())
            .flatten()
            .all(inline_ready),
        Block::ThematicBreak | Block::Code { .. } => true,
    }
}

fn inline_ready(inline: &Inline) -> bool {
    match inline {
        Inline::Later(_) => false,
        Inline::Emphasis(children)
        | Inline::Strong(children)
        | Inline::Delete(children)
        | Inline::Link { children, .. } => children.iter().all(inline_ready),
        Inline::Text(_) | Inline::Code(_) | Inline::Break => true,
    }
}

/// Map parsed blocks and inlines to their display values.
///
/// # Errors
///
/// Returns the parser's original diagnostic if parsing fails, or a diagnostic if
/// any block or inline is more than 100 levels below the root.
pub fn display(source: &str) -> Result<Display, markdown::message::Message> {
    map_display(source, false)
}

/// Map parsed blocks and inlines, with text line ends displayed as breaks.
///
/// # Errors
///
/// Returns the parser's original diagnostic if parsing fails, or a diagnostic if
/// any block or inline is more than 100 levels below the root.
pub fn display_with_breaks(source: &str) -> Result<Display, markdown::message::Message> {
    map_display(source, true)
}

fn map_display(source: &str, breaks: bool) -> Result<Display, markdown::message::Message> {
    let tree = super::parse(source)?;
    let offset_text = super::prelude(source);
    let mut definitions = Vec::new();
    let mut pending = vec![(&tree, 0)];
    while let Some((node, level)) = pending.pop() {
        if level > 100 {
            return Err(markdown::message::Message {
                place: None,
                reason: "markdown nests deeper than 100 levels".to_owned(),
                rule_id: Box::new("nesting".to_owned()),
                source: Box::new("swarm".to_owned()),
            });
        }
        if let Node::Definition(definition) = node {
            definitions.push(definition);
        }
        if let Some(children) = node.children() {
            pending.extend(children.iter().rev().map(|child| (child, level + 1)));
        }
    }
    let children = tree.children().map_or(&[][..], Vec::as_slice);
    Ok(Display {
        blocks: map_blocks(children, source, &offset_text, &definitions, breaks),
    })
}

fn map_blocks(
    nodes: &[Node],
    source: &str,
    offset_text: &str,
    definitions: &[&markdown::mdast::Definition],
    breaks: bool,
) -> Vec<Block> {
    nodes
        .iter()
        .filter_map(|node| {
            Some(match node {
                Node::Paragraph(paragraph) => {
                    Block::Paragraph(map_inlines(&paragraph.children, definitions, breaks))
                }
                Node::Heading(heading) => Block::Heading {
                    depth: heading.depth,
                    children: map_inlines(&heading.children, definitions, breaks),
                },
                Node::ThematicBreak(_) => Block::ThematicBreak,
                Node::Blockquote(quote) => Block::Blockquote(map_blocks(
                    &quote.children,
                    source,
                    offset_text,
                    definitions,
                    breaks,
                )),
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
                            Node::ListItem(item) => Some(map_blocks(
                                &item.children,
                                source,
                                offset_text,
                                definitions,
                                breaks,
                            )),
                            _ => None,
                        })
                        .collect(),
                },
                Node::Code(code) => {
                    let label = code_label(code, offset_text);
                    let text = code_text(code, offset_text);
                    let diff = code_diff(label.as_deref(), &text);
                    Block::Code { label, text, diff }
                }
                Node::Table(table) => {
                    let mut rows = table
                        .children
                        .iter()
                        .filter_map(Node::children)
                        .map(|cells| {
                            cells
                                .iter()
                                .filter_map(Node::children)
                                .map(|inlines| map_inlines(inlines, definitions, breaks))
                                .collect::<Vec<_>>()
                        });
                    let head = rows.next().unwrap_or_default();
                    let rows = rows
                        .map(|mut row| {
                            row.resize(head.len(), Vec::new());
                            row
                        })
                        .collect();
                    Block::Table {
                        source: table_source(table, source).unwrap_or_default(),
                        align: table
                            .align
                            .iter()
                            .map(|align| match align {
                                markdown::mdast::AlignKind::None => Align::None,
                                markdown::mdast::AlignKind::Left => Align::Left,
                                markdown::mdast::AlignKind::Center => Align::Center,
                                markdown::mdast::AlignKind::Right => Align::Right,
                            })
                            .collect(),
                        head,
                        rows,
                    }
                }
                Node::Definition(_) => return None,
                _ => Block::Later("block"),
            })
        })
        .collect()
}

fn code_diff(label: Option<&str>, text: &str) -> Option<serde_json::Value> {
    if !matches!(label, Some("diff" | "patch")) {
        return None;
    }
    let utf16: Vec<_> = text.encode_utf16().collect();
    if display_patch::is_patch(&utf16) {
        return Some(super::diff_fence::patch(text));
    }
    let prefix: Vec<_> = text
        .strip_suffix('\n')
        .unwrap_or(text)
        .split('\n')
        .map(|line| {
            let kind = if line.starts_with('+') {
                "add"
            } else if line.starts_with('-') {
                "del"
            } else if line.starts_with("@@") {
                "hunk"
            } else {
                "ctx"
            };
            serde_json::json!({"kind": kind, "text": format!("{line}\n")})
        })
        .collect();
    Some(serde_json::json!({"prefix": prefix}))
}

fn map_inlines(
    nodes: &[Node],
    definitions: &[&markdown::mdast::Definition],
    breaks: bool,
) -> Vec<Inline> {
    let mut inlines = Vec::new();
    for node in nodes {
        let inline = match node {
            Node::Text(text) if breaks => {
                for (index, piece) in text.value.split('\n').enumerate() {
                    if index > 0 {
                        inlines.push(Inline::Break);
                    }
                    if !piece.is_empty() {
                        inlines.push(Inline::Text(piece.to_owned()));
                    }
                }
                continue;
            }
            Node::Text(text) => Inline::Text(text.value.clone()),
            Node::Emphasis(emphasis) => {
                Inline::Emphasis(map_inlines(&emphasis.children, definitions, breaks))
            }
            Node::Strong(strong) => {
                Inline::Strong(map_inlines(&strong.children, definitions, breaks))
            }
            Node::Delete(delete) => {
                Inline::Delete(map_inlines(&delete.children, definitions, breaks))
            }
            Node::InlineCode(code) => Inline::Code(code.value.replace('\n', " ")),
            Node::Break(_) => Inline::Break,
            Node::Link(link) => Inline::Link {
                url: link.url.clone(),
                title: link.title.clone(),
                children: map_inlines(&link.children, definitions, breaks),
            },
            Node::LinkReference(reference) => {
                if let Some(definition) = definitions
                    .iter()
                    .find(|definition| definition.identifier == reference.identifier)
                {
                    Inline::Link {
                        url: definition.url.clone(),
                        title: definition.title.clone(),
                        children: map_inlines(&reference.children, definitions, breaks),
                    }
                } else {
                    Inline::Later("reference")
                }
            }
            _ => Inline::Later("inline"),
        };
        inlines.push(inline);
    }
    inlines
}

fn code_label(code: &markdown::mdast::Code, offset_text: &str) -> Option<String> {
    code.position
        .as_ref()
        .filter(|position| {
            matches!(
                offset_text.as_bytes().get(position.start.offset),
                Some(b'`' | b'~')
            )
        })
        .map(|_| code.lang.clone().unwrap_or_else(|| "code".to_owned()))
}

fn table_source(table: &markdown::mdast::Table, source: &str) -> Option<String> {
    table.position.as_ref().map(|position| {
        let start = position.start.line.saturating_sub(1);
        // Keep raw CR and NUL bytes; parser offsets use the prelude text.
        source
            .split('\n')
            .skip(start)
            .take(position.end.line.saturating_sub(start))
            .collect::<Vec<_>>()
            .join("\n")
    })
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
