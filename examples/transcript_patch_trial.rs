#[path = "../src/view/transcript/display_comments.rs"]
mod display_comments;
#[path = "../src/view/transcript/display_patch.rs"]
mod display_patch;

use display_patch::{PatchKind, PatchLine, SplitRow};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, error::Error, fs, path::Path};

#[derive(Deserialize)]
struct PatchInput {
    id: String,
    text: Option<String>,
    lines: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct CommentInput {
    id: String,
    patch: String,
    from: usize,
    to: usize,
}

#[derive(Deserialize)]
struct Task {
    patch: Vec<PatchInput>,
    comment: Vec<CommentInput>,
    render: Vec<Value>,
    find: Vec<Value>,
}

fn text(units: &[u16]) -> Result<String, std::string::FromUtf16Error> {
    // JSON input is valid UTF8; the core only splits LF or removes one ASCII prefix.
    String::from_utf16(units)
}

fn kind(k: PatchKind) -> &'static str {
    match k {
        PatchKind::Add => "add",
        PatchKind::Del => "del",
        PatchKind::Ctx => "ctx",
        PatchKind::Hunk => "hunk",
        PatchKind::Meta => "meta",
    }
}

fn line(l: &PatchLine<'_>) -> Result<Value, Box<dyn Error>> {
    let mut out = json!({"kind": kind(l.kind), "text": text(l.text)?});
    if let Some(n) = l.old {
        out["old"] = json!(n);
    }
    if let Some(n) = l.new {
        out["new"] = json!(n);
    }
    Ok(out)
}

fn diff_lines<'a>(lines: &[PatchLine<'a>]) -> Vec<PatchLine<'a>> {
    lines
        .iter()
        .copied()
        .filter(|l| l.kind != PatchKind::Meta)
        .collect()
}

fn patch(input: PatchInput) -> Result<(String, Value), Box<dyn Error>> {
    let out = if let Some(raw) = input.lines {
        let units: Vec<Vec<u16>> = raw.iter().map(|s| s.encode_utf16().collect()).collect();
        let lines: Vec<&[u16]> = units.iter().map(Vec::as_slice).collect();
        let joined: Vec<u16> = raw.join("\n").encode_utf16().collect();
        json!({"isPatch": display_patch::is_patch(&joined),
            "kinds": display_patch::patch_kinds(&lines).into_iter().map(kind).collect::<Vec<_>>()})
    } else {
        let units: Vec<u16> = input
            .text
            .ok_or("text must be a string")?
            .encode_utf16()
            .collect();
        let parsed = display_patch::parse_patch(&units);
        let split = display_patch::split_patch(&parsed)
            .into_iter()
            .map(|row| -> Result<Value, Box<dyn Error>> {
                Ok(match row {
                    SplitRow::Hunk { text: hunk } => json!({"hunk": text(hunk)?}),
                    SplitRow::Pair { left, right } => json!({
                        "left": left.map(|i| line(&parsed[i])).transpose()?,
                        "right": right.map(|i| line(&parsed[i])).transpose()?
                    }),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        json!({"isPatch": display_patch::is_patch(&units), "split": split,
            "parsed": parsed.iter().map(line).collect::<Result<Vec<_>, _>>()?,
            "diffLines": diff_lines(&parsed).iter().map(line).collect::<Result<Vec<_>, _>>()?})
    };
    Ok((input.id, out))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 8 || args[2] != "task" || args[4] != "--composed" || args[6] != "--out" {
        return Err("expected CANDIDATE task INPUTS --composed FILE --out DIR".into());
    }
    let task: Task = serde_json::from_slice(&fs::read(&args[3])?)?;
    let composed: Vec<Value> = serde_json::from_slice(&fs::read(&args[5])?)?;
    if !task.render.is_empty() || !task.find.is_empty() || !composed.is_empty() {
        return Err("this private consumer accepts the patch/comment task subset".into());
    }
    let patches = task
        .patch
        .into_iter()
        .map(patch)
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut comments = BTreeMap::new();
    for c in task.comment {
        let units: Vec<u16> = c.patch.encode_utf16().collect();
        let lines = diff_lines(&display_patch::parse_patch(&units));
        let range = display_comments::comment_range(&lines, c.from, c.to);
        comments.insert(
            c.id,
            json!({"lines": text(&range.label)?,
            "quoted": range.quoted.iter().map(|s| text(s)).collect::<Result<Vec<_>, _>>()?}),
        );
    }
    let out = Path::new(&args[7]);
    fs::create_dir_all(out)?;
    fs::write(out.join("patch.json"), serde_json::to_vec(&patches)?)?;
    fs::write(out.join("comment.json"), serde_json::to_vec(&comments)?)?;
    for name in ["markdown.json", "find.json"] {
        fs::write(out.join(name), b"{}")?;
    }
    for name in ["c1-composed.json", "raw-composed.json"] {
        fs::write(out.join(name), b"[]")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_with_neither_text_nor_lines_is_rejected() {
        let input = PatchInput {
            id: "synthetic".into(),
            text: None,
            lines: None,
        };
        assert!(patch(input).is_err());
    }
}
