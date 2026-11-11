use super::{PatchKind, header, number};

pub fn patch_kinds(lines: &[&[u16]]) -> Vec<PatchKind> {
    let (mut old, mut added) = (0.0, 0.0);
    lines
        .iter()
        .map(|text| {
            if old > 0.0 || added > 0.0 {
                let kind = match text.first() {
                    Some(92) => return PatchKind::Meta,
                    Some(43) => PatchKind::Add,
                    Some(45) => PatchKind::Del,
                    _ => PatchKind::Ctx,
                };
                if kind != PatchKind::Add {
                    old -= 1.0;
                }
                if kind != PatchKind::Del {
                    added -= 1.0;
                }
                return kind;
            }
            if let Some(h) = header(text) {
                old = h.old_count.map_or(1.0, number);
                added = h.new_count.map_or(1.0, number);
                return PatchKind::Hunk;
            }
            let meta = [
                b"diff ".as_slice(),
                b"index ",
                b"--- ",
                b"+++ ",
                b"new file",
                b"deleted file",
                b"similarity",
                b"rename ",
            ]
            .iter()
            .any(|prefix| {
                text.len() >= prefix.len()
                    && text.iter().zip(*prefix).all(|(&a, &b)| a == u16::from(b))
            });
            if meta {
                PatchKind::Meta
            } else {
                PatchKind::Ctx
            }
        })
        .collect()
}
