#[path = "display_patch/kinds.rs"]
mod kinds;

pub use kinds::patch_kinds;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchKind {
    Add,
    Del,
    Ctx,
    Hunk,
    Meta,
}

#[derive(Clone, Copy, Debug)]
pub struct PatchLine<'a> {
    pub kind: PatchKind,
    pub text: &'a [u16],
    pub old: Option<f64>,
    pub new: Option<f64>,
}

#[derive(Debug)]
pub enum SplitRow<'a> {
    Hunk {
        text: &'a [u16],
    },
    Pair {
        left: Option<usize>,
        right: Option<usize>,
    },
}

struct Header<'a> {
    old: &'a [u16],
    new: &'a [u16],
    old_count: Option<&'a [u16]>,
    new_count: Option<&'a [u16]>,
}

fn digits(text: &[u16]) -> Option<(&[u16], &[u16])> {
    let end = text
        .iter()
        .position(|c| !(48..=57).contains(c))
        .unwrap_or(text.len());
    (end != 0).then(|| text.split_at(end))
}

fn count(text: &[u16]) -> Option<(Option<&[u16]>, &[u16])> {
    if let Some(rest) = text.strip_prefix(&[44]) {
        let (n, rest) = digits(rest)?;
        Some((Some(n), rest))
    } else {
        Some((None, text))
    }
}

fn header(text: &[u16]) -> Option<Header<'_>> {
    let rest = text.strip_prefix(&[64, 64, 32, 45])?;
    let (old, rest) = digits(rest)?;
    let (old_count, rest) = count(rest)?;
    let rest = rest.strip_prefix(&[32, 43])?;
    let (new, rest) = digits(rest)?;
    let (new_count, rest) = count(rest)?;
    rest.strip_prefix(&[32, 64, 64])?;
    Some(Header {
        old,
        new,
        old_count,
        new_count,
    })
}

fn number(digits: &[u16]) -> f64 {
    // Header groups contain ASCII digits only; parse as binary64, including overflow to Infinity.
    let decimal: String = digits.iter().map(|&c| char::from(c as u8)).collect();
    decimal.parse().expect("validated decimal digit group")
}

pub fn is_patch(text: &[u16]) -> bool {
    // JS multiline anchors also begin after CR, LINE SEPARATOR and PARAGRAPH SEPARATOR.
    text.split(|c| matches!(c, 10 | 13 | 0x2028 | 0x2029))
        .any(|line| header(line).is_some())
}

pub fn parse_patch(text: &[u16]) -> Vec<PatchLine<'_>> {
    let mut out = Vec::new();
    let (mut old, mut new) = (0.0, 0.0);
    let mut in_hunk = false;
    for text in text.split(|&c| c == 10) {
        let (kind, text, o, n) = if let Some(h) = header(text) {
            old = number(h.old);
            new = number(h.new);
            in_hunk = true;
            (PatchKind::Hunk, text, None, None)
        } else if !in_hunk {
            if text.is_empty() {
                continue;
            }
            (PatchKind::Meta, text, None, None)
        } else {
            match text.first() {
                Some(43) => {
                    let n = new;
                    new += 1.0;
                    (PatchKind::Add, &text[1..], None, Some(n))
                }
                Some(45) => {
                    let o = old;
                    old += 1.0;
                    (PatchKind::Del, &text[1..], Some(o), None)
                }
                Some(32) => {
                    let (o, n) = (old, new);
                    old += 1.0;
                    new += 1.0;
                    (PatchKind::Ctx, &text[1..], Some(o), Some(n))
                }
                Some(92) => (PatchKind::Meta, text, None, None),
                _ => continue,
            }
        };
        out.push(PatchLine {
            kind,
            text,
            old: o,
            new: n,
        });
    }
    out
}

pub fn split_patch<'a>(lines: &[PatchLine<'a>]) -> Vec<SplitRow<'a>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        match lines[i].kind {
            PatchKind::Hunk => {
                out.push(SplitRow::Hunk {
                    text: lines[i].text,
                });
                i += 1;
            }
            PatchKind::Ctx => {
                out.push(SplitRow::Pair {
                    left: Some(i),
                    right: Some(i),
                });
                i += 1;
            }
            PatchKind::Del | PatchKind::Add => {
                let start = i;
                while i < lines.len() && lines[i].kind == PatchKind::Del {
                    i += 1;
                }
                let middle = i;
                while i < lines.len() && lines[i].kind == PatchKind::Add {
                    i += 1;
                }
                for k in 0..(middle - start).max(i - middle) {
                    out.push(SplitRow::Pair {
                        left: (k < middle - start).then_some(start + k),
                        right: (k < i - middle).then_some(middle + k),
                    });
                }
            }
            PatchKind::Meta => i += 1,
        }
    }
    out
}

/// Changes selections index the filtered display, without file headers or notices.
pub fn comment_lines(text: &[u16]) -> Vec<PatchLine<'_>> {
    parse_patch(text)
        .into_iter()
        .filter(|line| line.kind != PatchKind::Meta)
        .collect()
}

pub fn quote(line: &PatchLine<'_>) -> Option<Vec<u16>> {
    let prefix = match line.kind {
        PatchKind::Add => 43,
        PatchKind::Del => 45,
        PatchKind::Ctx => 32,
        _ => return None,
    };
    Some(
        std::iter::once(prefix)
            .chain(line.text.iter().copied())
            .collect(),
    )
}
