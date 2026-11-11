pub(super) const READERS: &[(&str, &str, &str)] = &[
    (
        "claude",
        "claude.rs",
        include_str!("../../src/view/transcript/normalize/claude.rs"),
    ),
    (
        "claude",
        "claude_meta.rs",
        include_str!("../../src/view/transcript/normalize/claude_meta.rs"),
    ),
    (
        "claude",
        "claude_tools.rs",
        include_str!("../../src/view/transcript/normalize/claude_tools.rs"),
    ),
    (
        "codex",
        "codex.rs",
        include_str!("../../src/view/transcript/normalize/codex.rs"),
    ),
    (
        "codex",
        "codex_facts.rs",
        include_str!("../../src/view/transcript/normalize/codex_facts.rs"),
    ),
    (
        "codex",
        "codex_items.rs",
        include_str!("../../src/view/transcript/normalize/codex_items.rs"),
    ),
    (
        "codex",
        "codex_tools.rs",
        include_str!("../../src/view/transcript/normalize/codex_tools.rs"),
    ),
    (
        "codex",
        "codex_context.rs",
        include_str!("../../src/view/transcript/normalize/codex_context.rs"),
    ),
    (
        "codex",
        "codex_changes.rs",
        include_str!("../../src/view/transcript/normalize/codex_changes.rs"),
    ),
    (
        "codex",
        "codex_questions.rs",
        include_str!("../../src/view/transcript/normalize/codex_questions.rs"),
    ),
    (
        "pi",
        "pi.rs",
        include_str!("../../src/view/transcript/normalize/pi.rs"),
    ),
    (
        "pi",
        "pi_text.rs",
        include_str!("../../src/view/transcript/normalize/pi_text.rs"),
    ),
    (
        "agy",
        "agy.rs",
        include_str!("../../src/view/transcript/normalize/agy.rs"),
    ),
];

fn literal(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    if !text.starts_with('"') {
        return None;
    }
    let mut escaped = false;
    for (index, ch) in text.char_indices().skip(1) {
        if ch == '"' && !escaped {
            return Some((
                serde_json::from_str(&text[..=index]).ok()?,
                &text[index + 1..],
            ));
        }
        escaped = ch == '\\' && !escaped;
    }
    None
}

// Read literal match arms, type comparisons, and matches! patterns. Field names and emitted
// event names are not provider kinds. The guard below also checks a new-kind example.
pub(super) fn reader_kinds(source: &str) -> std::collections::BTreeSet<String> {
    let mut kinds = std::collections::BTreeSet::new();
    let mut pending = Vec::new();
    for line in source.lines() {
        let mut text = line.trim().trim_start_matches('|').trim();
        let mut names = Vec::new();
        while let Some((name, rest)) = literal(text) {
            names.push(name);
            text = rest.trim();
            if let Some(rest) = text.strip_prefix('|') {
                text = rest.trim();
            } else {
                break;
            }
        }
        if names.is_empty() {
            pending.clear();
        } else if text.starts_with("=>") || text.starts_with("if ") && text.contains("=>") {
            pending.extend(names);
            kinds.extend(pending.drain(..));
        } else if text.is_empty() {
            pending.extend(names);
        } else {
            pending.clear();
        }
    }
    for marker in [".text(\"type\")", "kind", "role", "record_type"] {
        for (at, _) in source.match_indices(marker) {
            if !marker.starts_with('.')
                && source[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch == '.' || ch == '_' || ch.is_alphanumeric())
            {
                continue;
            }
            let rest = &source[at + marker.len()..];
            if let Some(rest) = rest.trim_start().strip_prefix("==")
                && let Some((name, _)) = literal(rest)
            {
                kinds.insert(name);
            }
        }
    }
    for rest in source.split("matches!(").skip(1) {
        let mut depth = 1;
        let mut first = None;
        let mut end = None;
        let mut string = false;
        let mut escaped = false;
        for (index, ch) in rest.char_indices() {
            if ch == '"' && !escaped {
                string = !string;
            }
            if !string {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(index);
                            break;
                        }
                    }
                    ',' if depth == 1 && first.is_none() => first = Some(index),
                    _ => (),
                }
            }
            escaped = ch == '\\' && !escaped;
        }
        if let (Some(first), Some(end)) = (first, end) {
            let subject = rest[..first].trim();
            if subject == "kind.as_str()"
                || subject == "role.as_str()"
                || subject.contains(".text(\"type\")")
            {
                let mut text = &rest[first + 1..end];
                while let Some(at) = text.find('"') {
                    if let Some((name, rest)) = literal(&text[at..]) {
                        kinds.insert(name);
                        text = rest;
                    } else {
                        break;
                    }
                }
            }
        }
    }
    kinds.remove("");
    kinds
}
