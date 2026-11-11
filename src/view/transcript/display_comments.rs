use super::display_patch::{self, PatchKind, PatchLine};
use serde::Deserialize;
use serde_json::{Value, json};

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
            .filter_map(|line| display_patch::quote(line))
            .collect(),
    }
}

pub fn range_value(patch: &str, from: usize, to: usize) -> Value {
    let utf16: Vec<_> = patch.encode_utf16().collect();
    let lines = display_patch::comment_lines(&utf16);
    let range = comment_range(&lines, from, to);
    json!({
        "lines": String::from_utf16_lossy(&range.label),
        "quoted": range.quoted.iter().map(|line| String::from_utf16_lossy(line)).collect::<Vec<_>>()
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    path: String,
    lines: String,
    quoted: Vec<String>,
    text: String,
}

pub fn comment_message(comments: &[Comment]) -> String {
    let mut blocks = vec![format!("Review comments on the diff ({}):", comments.len())];
    for comment in comments {
        let mut lines = vec![format!("{} ({}):", comment.path, comment.lines)];
        lines.extend(comment.quoted.iter().map(|quote| format!("> {quote}")));
        lines.push(comment.text.clone());
        blocks.push(lines.join("\n"));
    }
    blocks.join("\n\n")
}
