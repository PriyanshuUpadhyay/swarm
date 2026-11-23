//! Which tool calls each agent CLI draws as one group, so the chat groups the same calls.
//! pi and AGY get no group here: pi draws every call alone, and AGY's rule is not known yet.

use super::claude_json::Object;

/// Claude Code's `collapsed_read_search` group (2.1.329, default view).
pub(super) const READ_SEARCH: &str = "read_search";
/// Codex's "Explored" cell (rust-v0.169.0 `exec_cell::is_exploring_call`).
pub(super) const EXPLORE: &str = "explore";

/// The part of a Claude read/search group that a call counts as: "read", "search", or "list".
/// MCP calls also collapse in Claude Code, but their label is not confirmed, so they stay alone.
pub(super) fn claude_part(name: &str, input: Option<&Object<'_>>) -> Option<&'static str> {
    match name {
        "Read" => Some("read"),
        "Grep" | "Glob" => Some("search"),
        "Bash" => bash_part(&input?.text("command")),
        _ => None,
    }
}

/// Claude Code's Bash read/search test: every segment is a search, read, list, or neutral command,
/// and no output goes to a file. The strongest part wins: search, then read, then list.
fn bash_part(command: &str) -> Option<&'static str> {
    const SEARCH: &[&str] = &[
        "find", "grep", "rg", "ag", "ack", "locate", "which", "whereis",
    ];
    const READ: &[&str] = &[
        "cat", "head", "tail", "less", "more", "wc", "stat", "file", "strings", "jq", "awk", "cut",
        "sort", "uniq", "tr",
    ];
    const LIST: &[&str] = &["ls", "tree", "du"];
    const NEUTRAL: &[&str] = &["echo", "printf", "true", "false", ":"];
    const FIND_WRITES: &[&str] = &[
        "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fprint", "-fprintf", "-fls",
    ];
    let (mut search, mut read, mut list) = (false, false, false);
    for segment in segments(command)? {
        let words: Vec<&str> = segment.split_whitespace().collect();
        let &first = words.first()?;
        for (i, word) in words.iter().enumerate() {
            if let Some(at) = word.find('>') {
                let target = if word[at + 1..].is_empty() {
                    words.get(i + 1).copied().unwrap_or("")
                } else {
                    &word[at + 1..]
                };
                if !(target == "/dev/null" || target.starts_with('&')) {
                    return None;
                }
            }
        }
        if first == "find" && words.iter().any(|word| FIND_WRITES.contains(word)) {
            return None;
        }
        if first == "sort"
            && words
                .iter()
                .any(|word| *word == "-o" || word.starts_with("--output"))
        {
            return None;
        }
        if SEARCH.contains(&first) {
            search = true;
        } else if READ.contains(&first) {
            read = true;
        } else if LIST.contains(&first) {
            list = true;
        } else if !NEUTRAL.contains(&first) {
            return None;
        }
    }
    [(search, "search"), (read, "read"), (list, "list")]
        .into_iter()
        .find(|(on, _)| *on)
        .map(|(_, part)| part)
}

/// Splits on `|`, `||`, `&&`, and `;` outside quotes. A command substitution or a background `&`
/// returns None, so such a command never collapses.
fn segments(command: &str) -> Option<Vec<&str>> {
    let bytes = command.as_bytes();
    let (mut out, mut start, mut quote, mut i) = (Vec::new(), 0, None, 0);
    while i < bytes.len() {
        let byte = bytes[i];
        match quote {
            Some(q) if byte == q => quote = None,
            Some(_) => {}
            None => match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'`' => return None,
                b'$' if bytes.get(i + 1) == Some(&b'(') => return None,
                b';' | b'|' | b'&' => {
                    let pair = bytes.get(i + 1) == Some(&byte);
                    // `2>&1` and `&>` are redirects, not separators.
                    if byte == b'&' && !pair {
                        if i > 0 && bytes[i - 1] == b'>' || bytes.get(i + 1) == Some(&b'>') {
                            i += 1;
                            continue;
                        }
                        return None;
                    }
                    out.push(&command[start..i]);
                    i += if pair { 2 } else { 1 };
                    start = i;
                    continue;
                }
                _ => {}
            },
        }
        i += 1;
    }
    out.push(&command[start..]);
    Some(
        out.into_iter()
            .filter(|segment| !segment.trim().is_empty())
            .collect(),
    )
}

/// Codex explores when every parsed command is a read, a file list, or a search.
pub(super) fn codex_explores(parsed: &[Object<'_>]) -> bool {
    !parsed.is_empty()
        && parsed
            .iter()
            .all(|part| matches!(part.text("type").as_str(), "read" | "list_files" | "search"))
}

#[cfg(test)]
mod tests {
    use super::bash_part;

    #[test]
    fn bash_reads_and_searches_collapse_and_writes_do_not() {
        assert_eq!(bash_part("rg -n fold src | head -20"), Some("search"));
        assert_eq!(
            bash_part("cat a.rs 2>/dev/null && wc -l b.rs"),
            Some("read")
        );
        assert_eq!(bash_part("ls src; echo done"), Some("list"));
        assert_eq!(bash_part("cat a.rs > b.rs"), None);
        assert_eq!(bash_part("git status"), None);
    }
}
