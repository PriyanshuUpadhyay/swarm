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
