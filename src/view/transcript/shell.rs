//! Shell and command output text and rows.

use super::{Row, RowKind, ShellRun, TextPreview, tagged};

pub fn clean_shell(text: &str) -> String {
    fn decode(text: &str) -> String {
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }
    let mut decoded = String::new();
    let mut rest = text;
    while let Some((before, body)) = rest.split_once("<persisted-output>") {
        decoded += &decode(before);
        decoded += "<persisted-output>";
        let end = body
            .find("</persisted-output>")
            .map(|index| index + "</persisted-output>".len())
            .unwrap_or(body.len());
        decoded += &body[..end];
        rest = &body[end..];
    }
    decoded += &decode(rest);
    let normalized = decoded.replace("\r\n", "\n");
    let mut characters = normalized.chars().peekable();
    let mut output = String::new();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            match characters.next() {
                Some('[') => {
                    while characters
                        .peek()
                        .is_some_and(|character| (' '..='?').contains(character))
                    {
                        characters.next();
                    }
                    if characters
                        .peek()
                        .is_some_and(|character| ('@'..='~').contains(character))
                    {
                        characters.next();
                    }
                }
                Some(']') => {
                    for character in characters.by_ref() {
                        if character == '\u{07}' || character == '\u{1b}' {
                            break;
                        }
                    }
                    if characters.peek() == Some(&'\\') {
                        characters.next();
                    }
                }
                _ => (),
            }
        } else if !character.is_control() || matches!(character, '\n' | '\t') {
            output.push(character);
        }
    }
    output
}
pub fn command_output_text(text: &str) -> String {
    let parts: Vec<_> = ["local-command-stdout", "local-command-stderr"]
        .into_iter()
        .filter_map(|tag| tagged(text, tag))
        .collect();
    clean_shell(&if parts.is_empty() {
        text.into()
    } else {
        parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    })
}
pub fn shell_run(command: Option<String>, text: &str, output_id: String) -> ShellRun {
    let output_text = shell_output_text(text);
    let mut output = TextPreview::new(&output_text, 120);
    output.id = output_id;
    ShellRun {
        command,
        output,
        exit_code: tagged(text, "bash-exit-code").and_then(|code| code.trim().parse().ok()),
    }
}
pub fn shell_output_text(text: &str) -> String {
    let stdout = clean_shell(tagged(text, "bash-stdout").unwrap_or(""));
    let stderr = clean_shell(tagged(text, "bash-stderr").unwrap_or(""));
    let stdout = if stdout == "(Bash completed with no output)" {
        ""
    } else {
        stdout.trim_end_matches(['\n', '\r'])
    };
    [stdout, stderr.trim_end_matches(['\n', '\r'])]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
pub(crate) fn shell_row(run: ShellRun, id: String) -> Row {
    let text = [
        run.command.as_ref().map(|command| format!("$ {command}")),
        (!run.output.text.is_empty()).then(|| run.output.text.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n");
    let mut row = Row::new(RowKind::Shell, text, id);
    row.shell = Some(run);
    row.starts_turn = true;
    row
}
