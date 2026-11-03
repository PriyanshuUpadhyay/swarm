//! Shell text split into simple commands, for the main wall. Best effort, not a shell parser.

#[derive(Debug, PartialEq)]
pub(super) enum Token {
    Word(String),
    /// An output redirect; the next word is its target.
    Out,
    /// An input redirect or a fd copy; the next word is not a file written.
    In,
}

/// Shell text split into simple commands. Quotes group words, and here-doc bodies are skipped.
pub(super) fn commands(text: &str) -> Vec<Vec<Token>> {
    let mut all = vec![Vec::new()];
    let mut word = String::new();
    let mut started = false;
    let (mut single, mut double) = (false, false);
    let mut heredocs: Vec<String> = Vec::new();
    let mut want_delimiter = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let push = |all: &mut Vec<Vec<Token>>,
                word: &mut String,
                started: &mut bool,
                want: &mut bool,
                heredocs: &mut Vec<String>| {
        if *started {
            if *want {
                heredocs.push(std::mem::take(word));
                *want = false;
            } else {
                all.last_mut()
                    .unwrap()
                    .push(Token::Word(std::mem::take(word)));
            }
        }
        word.clear();
        *started = false;
    };
    while i < chars.len() {
        let c = chars[i];
        if single {
            if c == '\'' {
                single = false;
            } else {
                word.push(c);
            }
        } else if double {
            match c {
                '"' => double = false,
                '\\' if i + 1 < chars.len() => {
                    i += 1;
                    word.push(chars[i]);
                }
                _ => word.push(c),
            }
        } else {
            match c {
                '\'' => {
                    single = true;
                    started = true;
                }
                '"' => {
                    double = true;
                    started = true;
                }
                // A backslash before a newline joins the lines, as the shell does.
                '\\' if chars.get(i + 1) == Some(&'\n') => i += 1,
                '\\' if i + 1 < chars.len() => {
                    i += 1;
                    word.push(chars[i]);
                    started = true;
                }
                // A `#` that starts a word starts a comment, up to the end of the line.
                '#' if !started => {
                    while chars.get(i + 1).is_some_and(|next| *next != '\n') {
                        i += 1;
                    }
                }
                ' ' | '\t' => push(
                    &mut all,
                    &mut word,
                    &mut started,
                    &mut want_delimiter,
                    &mut heredocs,
                ),
                '\n' | ';' | '|' | '&' | '(' | ')' | '`' => {
                    if c == '&' && chars.get(i + 1) == Some(&'>') {
                        push(
                            &mut all,
                            &mut word,
                            &mut started,
                            &mut want_delimiter,
                            &mut heredocs,
                        );
                        all.last_mut().unwrap().push(Token::Out);
                        i += 2;
                        while chars.get(i) == Some(&'>') {
                            i += 1;
                        }
                        continue;
                    }
                    push(
                        &mut all,
                        &mut word,
                        &mut started,
                        &mut want_delimiter,
                        &mut heredocs,
                    );
                    all.push(Vec::new());
                    if c == '\n' && !heredocs.is_empty() {
                        // Skip each here-doc body up to its closing line.
                        let rest: String = chars[i + 1..].iter().collect();
                        let mut skipped = 0;
                        let mut lines = rest.split_inclusive('\n');
                        for delimiter in heredocs.drain(..) {
                            for line in lines.by_ref() {
                                skipped += line.chars().count();
                                if line.trim() == delimiter {
                                    break;
                                }
                            }
                        }
                        i += skipped;
                    }
                }
                '>' => {
                    // A fd number before `>` belongs to the redirect, not the command.
                    if started && word.chars().all(|d| d.is_ascii_digit()) {
                        word.clear();
                        started = false;
                    }
                    push(
                        &mut all,
                        &mut word,
                        &mut started,
                        &mut want_delimiter,
                        &mut heredocs,
                    );
                    i += 1;
                    let mut token = Token::Out;
                    while let Some(next) = chars.get(i) {
                        match next {
                            '>' | '|' => i += 1,
                            // `>&2` copies a fd, but `>& file` writes the file.
                            '&' => {
                                i += 1;
                                let fd = chars
                                    .get(i..)
                                    .unwrap_or_default()
                                    .iter()
                                    .find(|c| **c != ' ')
                                    .is_some_and(|c| c.is_ascii_digit() || *c == '-');
                                if fd {
                                    token = Token::In;
                                }
                            }
                            _ => break,
                        }
                    }
                    all.last_mut().unwrap().push(token);
                    continue;
                }
                '<' => {
                    push(
                        &mut all,
                        &mut word,
                        &mut started,
                        &mut want_delimiter,
                        &mut heredocs,
                    );
                    // `<<<` is a here-string and `1<<20` a shift; neither starts a here-doc.
                    if chars.get(i + 1) == Some(&'<') && chars.get(i + 2) == Some(&'<') {
                        i += 3;
                        all.last_mut().unwrap().push(Token::In);
                        continue;
                    }
                    let shift = chars
                        .get(i + 2..)
                        .unwrap_or_default()
                        .iter()
                        .find(|c| !matches!(c, '-' | ' ' | '\t'))
                        .is_some_and(|c| c.is_ascii_digit());
                    // Inside `$(( … ))` or `(( … ))`, `<<` is a shift.
                    // Only this line counts, so a here-doc body or an earlier line cannot turn it off.
                    let line_start = chars[..i]
                        .iter()
                        .rposition(|c| *c == '\n')
                        .map_or(0, |at| at + 1);
                    let before: String = chars[line_start..i].iter().collect();
                    let arithmetic = before.matches("((").count() > before.matches("))").count();
                    if chars.get(i + 1) == Some(&'<') && !shift && !arithmetic {
                        i += 2;
                        if chars.get(i) == Some(&'-') {
                            i += 1;
                        }
                        want_delimiter = true;
                        continue;
                    }
                    all.last_mut().unwrap().push(Token::In);
                }
                _ => {
                    word.push(c);
                    started = true;
                }
            }
        }
        i += 1;
    }
    push(
        &mut all,
        &mut word,
        &mut started,
        &mut want_delimiter,
        &mut heredocs,
    );
    all.retain(|command| !command.is_empty());
    all
}
