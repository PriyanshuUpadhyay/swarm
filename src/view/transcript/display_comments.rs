use super::display_patch::{PatchKind, PatchLine};

#[derive(Debug)]
pub struct CommentRange {
    pub label: Vec<u16>,
    pub quoted: Vec<Vec<u16>>,
}

fn number_text(n: f64) -> String {
    if n == f64::INFINITY {
        return "Infinity".into();
    }
    // Callers supply nonnegative integral counters. serde_json uses the locked zmij
    // shortest correctly rounded decimal primitive; only ECMA decimal placement differs.
    let decimal = serde_json::to_string(&n).expect("finite binary64 serialization");
    let (mantissa, exponent) = decimal
        .split_once('e')
        .map_or((decimal.as_str(), 0), |(m, e)| {
            (m, e.parse::<i32>().expect("decimal exponent"))
        });
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let digits: String = mantissa.chars().filter(|&c| c != '.').collect();
    let digits = digits.trim_end_matches('0');
    if digits.is_empty() {
        return "0".into();
    }
    if point <= 21 {
        let mut out = digits.to_owned();
        out.extend(std::iter::repeat_n('0', point as usize - digits.len()));
        out
    } else {
        let mut out = digits[..1].to_owned();
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push_str(&format!("e+{}", point - 1));
        out
    }
}

pub fn comment_range(lines: &[PatchLine<'_>], from: usize, to: usize) -> CommentRange {
    let start = from.min(to).min(lines.len());
    let last = from.max(to);
    // Clamp before the inclusive-to-exclusive conversion, even for usize::MAX.
    let end = if last >= lines.len() {
        lines.len()
    } else {
        last + 1
    };
    let picked: Vec<_> = lines[start..end]
        .iter()
        .filter(|l| matches!(l.kind, PatchKind::Add | PatchKind::Del | PatchKind::Ctx))
        .collect();
    let news: Vec<_> = picked
        .iter()
        .filter(|l| l.kind != PatchKind::Del)
        .filter_map(|l| l.new)
        .collect();
    let (nums, prefix) = if news.is_empty() {
        (picked.iter().filter_map(|l| l.old).collect(), "old line")
    } else {
        (news, "line")
    };
    let label = if nums.is_empty() {
        String::new()
    } else {
        let lo = nums.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = nums.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if lo == hi {
            format!("{prefix} {}", number_text(lo))
        } else {
            format!("{prefix}s {}-{}", number_text(lo), number_text(hi))
        }
    };
    CommentRange {
        label: label.encode_utf16().collect(),
        quoted: picked
            .iter()
            .map(|l| {
                let prefix = match l.kind {
                    PatchKind::Add => 43,
                    PatchKind::Del => 45,
                    _ => 32,
                };
                std::iter::once(prefix)
                    .chain(l.text.iter().copied())
                    .collect()
            })
            .collect(),
    }
}
