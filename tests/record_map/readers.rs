use std::collections::BTreeSet;

pub(super) fn readers() -> Vec<(String, String, String)> {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/view/transcript/normalize");
    let mut readers = Vec::new();

    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let file = path.file_name().unwrap().to_str().unwrap();
        let cli = file.split(['_', '.']).next().unwrap();
        if swarm::view::transcript::source::NativeFormat::from_name(cli).is_some()
            && path.extension().is_some_and(|ext| ext == "rs")
        {
            readers.push((
                cli.to_owned(),
                file.to_owned(),
                std::fs::read_to_string(&path).unwrap(),
            ));
        }
    }

    readers.sort();
    readers
}

fn literal(text: &str) -> (String, &str) {
    let mut stream = serde_json::Deserializer::from_str(text).into_iter::<String>();
    let value = stream
        .next()
        .expect("record-map lexer: missing string literal")
        .expect("record-map lexer: unsupported string literal; use JSON-compatible escapes");
    (value, &text[stream.byte_offset()..])
}

// Character tokens must not expose braces or quotes to the discriminator scan.
fn character(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('\'')?;
    let first = rest
        .chars()
        .next()
        .expect("record-map lexer: unfinished character");
    let mut end = first.len_utf8();

    if first == '\\' {
        let escaped = rest[end..]
            .chars()
            .next()
            .expect("record-map lexer: unfinished escape");
        assert!(
            matches!(escaped, '\\' | '\'' | '"' | 'n' | 'r' | 't' | '0'),
            "record-map lexer: unsupported character escape"
        );
        end += escaped.len_utf8();
    }

    if rest[end..].starts_with('\'') {
        return Some(&rest[end + 1..]);
    }
    if first.is_ascii_alphabetic() || first == '_' {
        // A lifetime has no closing quote, such as 'a or '_ .
        let end = rest
            .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .unwrap_or(rest.len());
        assert!(
            !rest[end..].starts_with('\''),
            "record-map lexer: unsupported character literal"
        );
        return None;
    }
    panic!("record-map lexer: unsupported character literal");
}

#[derive(Debug)]
struct Token {
    text: String,
    quoted: bool,
}

fn tokens(mut text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    while !text.is_empty() {
        text = text.trim_start();
        if text.starts_with("//") {
            text = text.split_once('\n').map_or("", |(_, rest)| rest);
            continue;
        }

        if text.is_empty() {
            break;
        }

        let raw = text.strip_prefix("br").or_else(|| text.strip_prefix('r'));
        assert!(
            !raw.is_some_and(|rest| rest.trim_start_matches('#').starts_with('"')),
            "record-map lexer: unsupported raw string literal"
        );
        assert!(
            !text.starts_with("/*"),
            "record-map lexer: unsupported block comment"
        );

        if text.starts_with('"') {
            let (value, rest) = literal(text);
            tokens.push(Token {
                text: value,
                quoted: true,
            });
            text = rest;
            continue;
        }

        if let Some(rest) = character(text) {
            tokens.push(Token {
                text: "<char>".into(),
                quoted: false,
            });
            text = rest;
            continue;
        }

        let length = if text.starts_with("==") || text.starts_with("=>") {
            2
        } else if text.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_') {
            text.find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .unwrap_or(text.len())
        } else {
            text.chars().next().unwrap().len_utf8()
        };
        tokens.push(Token {
            text: text[..length].into(),
            quoted: false,
        });
        text = &text[length..];
    }
    tokens
}

fn subject(tokens: &[Token]) -> Option<String> {
    let mut tokens = tokens;
    if tokens.len() >= 4
        && tokens[tokens.len() - 4..]
            .iter()
            .map(|token| token.text.as_str())
            .eq([".", "as_str", "(", ")"])
    {
        tokens = &tokens[..tokens.len() - 4];
    }

    let last = tokens.last()?;
    if matches!(
        last.text.as_str(),
        "kind" | "role" | "record_type" | "status" | "command" | "stop" | "error" | "name"
    ) && !last.quoted
    {
        if tokens.len() >= 3 && tokens[tokens.len() - 2].text == "." {
            return Some(format!("{}.{}", tokens[tokens.len() - 3].text, last.text));
        }
        return Some(last.text.clone());
    }

    if tokens.len() >= 4 {
        let tail = &tokens[tokens.len() - 4..];
        if tail[1].text == "["
            && tail[2].quoted
            && tail[3].text == "]"
            && matches!(tail[2].text.as_str(), "type" | "kind" | "status")
        {
            return Some(format!("{}.{}", tail[0].text, tail[2].text));
        }
    }

    if tokens.len() >= 6 {
        let tail = &tokens[tokens.len() - 6..];
        if tail[1].text == "."
            && tail[2].text == "text"
            && tail[3].text == "("
            && tail[5].text == ")"
            && tail[4].quoted
            && matches!(tail[4].text.as_str(), "type" | "kind" | "status")
        {
            return Some(format!("{}.{}", tail[0].text, tail[4].text));
        }
    }
    None
}

// Function names locate each provider's discriminator. Kinds stay in RECORD_MAP only.
// An unrecognized discriminator fails closed, so moving one needs an explicit group here.
fn group(cli: &str, file: &str, function: &str, subject: &str) -> Option<&'static str> {
    match (cli, subject) {
        (_, "record_type" | "value.type") => Some(""),

        ("claude", "block.type" | "part.type") => Some("block"),
        ("claude", "kind") => Some(match function {
            "line" => "",
            "attachment" => "attachment",
            "system" => "system",
            "messages" => "block",
            _ => panic!("unclassified discriminator {file}/{function}/{subject}"),
        }),

        ("codex", "role") | ("pi", "role") => Some("message"),
        ("codex", "kind") => Some(match function {
            "parse" | "append" => "response_item",
            "event_message" => "event_msg",
            "message" | "output" => "message",
            _ => panic!("unclassified discriminator {file}/{function}/{subject}"),
        }),
        ("codex", "item.type") => Some("item_completed"),
        ("codex", "item.kind") => Some("item_completed/Extension"),
        ("codex", "part.type") => Some(if file == "codex_items.rs" {
            "item_content"
        } else {
            "message"
        }),
        ("codex", "change.type") => Some("file_change"),
        ("codex", "action.type") => Some("local_shell"),

        ("pi", "kind") | ("agy", "kind") => Some(""),
        ("pi", "block.type") => Some("block"),

        ("agy", "status") => Some("status"),
        (_, field) if field.ends_with(".status") => None,
        (_, "status" | "command" | "stop" | "error" | "name" | "self.kind")
        | ("claude", "origin.kind") => None,
        _ => panic!("unclassified discriminator {cli}/{file}/{function}/{subject}"),
    }
}

fn add(kinds: &mut BTreeSet<String>, group: &str, name: &str) {
    if !name.is_empty() {
        kinds.insert(if group.is_empty() {
            name.to_owned()
        } else {
            format!("{group}/{name}")
        });
    }
}

fn add_patterns(
    kinds: &mut BTreeSet<String>,
    context: (&str, &str, &str),
    input: &[Token],
    patterns: &[Token],
) {
    let names: Vec<_> = patterns.iter().filter(|token| token.quoted).collect();
    if names.is_empty() {
        return;
    }

    let (cli, file, function) = context;
    let subject = subject(input).unwrap_or_else(|| {
        panic!("record-map guard: unreadable matches! discriminator in {cli}/{file}/{function}")
    });
    if let Some(group) = group(cli, file, function, &subject) {
        for token in names {
            add(kinds, group, &token.text);
        }
    }
}

// Read named match arms, comparisons and matches! patterns with their discriminator groups.
pub(super) fn reader_kinds(cli: &str, file: &str, source: &str) -> BTreeSet<String> {
    let tokens = tokens(source);
    let mut kinds = BTreeSet::new();
    let mut function = "";
    let mut braces: Vec<Option<&str>> = Vec::new();
    let mut matches = std::collections::BTreeMap::new();

    for (index, token) in tokens.iter().enumerate() {
        let text = token.text.as_str();
        if text == "fn" && !token.quoted {
            function = &tokens[index + 1].text;
        }

        if text == "match"
            && !token.quoted
            && let Some(end) =
                (index + 1..tokens.len()).find(|&i| tokens[i].text == "{" && !tokens[i].quoted)
        {
            let group = subject(&tokens[index + 1..end]).map_or(Some("<unreadable>"), |subject| {
                group(cli, file, function, &subject)
            });
            matches.insert(end, group);
        }

        if text == "{" && !token.quoted {
            braces.push(matches.remove(&index).flatten());
        } else if text == "}" && !token.quoted {
            braces.pop();
        }

        if token.quoted
            && tokens
                .get(index.wrapping_sub(1))
                .is_none_or(|token| token.text != "==")
            && let Some(Some(group)) = braces.last()
        {
            let mut end = index + 1;
            while tokens.get(end).is_some_and(|token| token.text == "|")
                && tokens.get(end + 1).is_some_and(|token| token.quoted)
            {
                end += 2;
            }
            if tokens
                .get(end)
                .is_some_and(|token| matches!(token.text.as_str(), "=>" | "if"))
            {
                assert_ne!(
                    *group, "<unreadable>",
                    "record-map guard: unreadable discriminator in {cli}/{file}/{function}"
                );

                for token in tokens[index..end].iter().filter(|token| token.quoted) {
                    add(&mut kinds, group, &token.text);
                }
            }
        }

        if text == "=="
            && !token.quoted
            && let Some(subject) = subject(&tokens[..index])
            && let Some(group) = group(cli, file, function, &subject)
            && let Some(name) = tokens.get(index + 1).filter(|token| token.quoted)
        {
            add(&mut kinds, group, &name.text);
        }

        if text != "matches"
            || token.quoted
            || tokens.get(index + 1).is_none_or(|token| token.text != "!")
        {
            continue;
        }

        let mut depth = 0;
        let mut comma = None;
        for end in index + 2..tokens.len() {
            if tokens[end].quoted {
                continue;
            }

            match tokens[end].text.as_str() {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;

                    if depth == 0 {
                        if let Some(comma) = comma {
                            add_patterns(
                                &mut kinds,
                                (cli, file, function),
                                &tokens[index + 3..comma],
                                &tokens[comma + 1..end],
                            );
                        }

                        break;
                    }
                }
                "," if depth == 1 && comma.is_none() => comma = Some(end),
                _ => (),
            }
        }
    }

    kinds
}
