//! Pi text blocks, images, and literal skill expansion.

use super::claude_json::{self, Object};
use super::pi::Fact;
use super::{SnapshotError, Work};
use serde_json::{json, value::RawValue};

pub(super) fn join_text(
    content: Option<&RawValue>,
    work: &mut dyn Work,
) -> Result<String, SnapshotError> {
    let Some(content) = content else {
        return Ok(String::new());
    };
    if let Ok(text) = serde_json::from_str::<String>(content.get()) {
        return Ok(text);
    }
    let mut parts = Vec::new();
    for raw in claude_json::array(content).unwrap_or_default() {
        work.check()?;
        if let Some(block) = Object::parse(raw)
            && block.text("type") == "text"
        {
            parts.push(block.text("text"));
        }
    }
    Ok(parts.join("\n"))
}

pub(super) fn append_images(
    fact: &mut Fact,
    content: Option<&RawValue>,
    role: &str,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    for raw in content.and_then(claude_json::array).unwrap_or_default() {
        work.check()?;
        if let Some(block) = Object::parse(raw)
            && block.text("type") == "image"
        {
            fact.push(
                "image",
                json!({"role": role, "media_type": block.text("mimeType")}),
            );
        }
    }
    Ok(())
}

pub(super) fn append_skill(
    fact: &mut Fact,
    text: &str,
    work: &mut dyn Work,
) -> Result<bool, SnapshotError> {
    const OPEN: &str = "<skill name=\"";
    const CLOSE: &str = "\n</skill>";
    let Some(start) = text.find(OPEN) else {
        return Ok(false);
    };
    let head = &text[start + OPEN.len()..];
    let Some(name_end) = head.find("\" location=\"") else {
        return Ok(false);
    };
    if name_end == 0 {
        return Ok(false);
    }
    let Some(tag_end) = head.find("\">\n") else {
        return Ok(false);
    };
    let rest = &head[tag_end + "\">\n".len()..];
    let Some(mut body_len) = rest.find(CLOSE) else {
        return Ok(false);
    };
    let mut next = Some(body_len);
    while let Some(at) = next {
        work.check()?;
        let tail = &rest[at + CLOSE.len()..];
        if tail.is_empty() || tail.starts_with("\n\n") {
            body_len = at;
            break;
        }
        next = rest[at + 1..].find(CLOSE).map(|next| at + 1 + next);
    }
    let name = &head[..name_end];
    let before = text[..start].trim_matches([' ', '\t', '\r', '\n']);
    let after = rest[body_len + CLOSE.len()..].trim_matches([' ', '\t', '\r', '\n']);
    let words = match (before.is_empty(), after.is_empty()) {
        (false, false) => format!("{before}\n\n{after}"),
        (false, true) => before.to_owned(),
        _ => after.to_owned(),
    };
    fact.push(
        "system_message",
        json!({
            "kind": "command",
            "text": format!("<command-message>{name}</command-message>\n<command-name>/skill:{name}</command-name>\n<command-args>{words}</command-args>"),
        }),
    );
    fact.push_with_meta(
        "system_message",
        json!({"kind": "skill_body", "text": &rest[..body_len]}),
        fact.child_meta(),
    );
    Ok(true)
}
