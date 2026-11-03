//! ADR 0135: in a worktree hub, the main worktree is read-only for agents. This wall denies an
//! edit tool call whose file is in main, and a shell call that writes main's files or moves the
//! default branch. Shell text is read best effort: a path the command builds at run time passes.
//! Main's `tmp/` stays writable (`task::SCRATCH`).

use super::*;
use crate::task;
use std::path::{Path, PathBuf};

pub const MAIN_READ_ONLY: &str = "swarm guard: main is read-only for agents. Run `swarm task start <name>`, edit and commit in the worktree it prints, then run `swarm task land <name>`; so the call is blocked";

/// Git subcommands that change a worktree, its index, or its branch.
const GIT_WRITES: [&str; 18] = [
    "add",
    "am",
    "apply",
    "checkout",
    "cherry-pick",
    "clean",
    "commit",
    "merge",
    "mv",
    "pull",
    "rebase",
    "reset",
    "restore",
    "revert",
    "rm",
    "stash",
    "switch",
    "update-ref",
];
/// Shell programs whose plain arguments are files they write.
const WRITERS: [&str; 12] = [
    "rm", "rmdir", "mv", "touch", "mkdir", "tee", "truncate", "chmod", "chown", "unlink", "patch",
    "dd",
];
/// Shell programs that write only their last argument.
const COPIERS: [&str; 3] = ["cp", "ln", "install"];
/// Words that start a command without being the command.
const PREFIXES: [&str; 12] = [
    "sudo", "env", "command", "exec", "nohup", "time", "then", "do", "else", "if", "!", "{",
];

pub fn main_wall(provider: Provider, event: &str, payload: &str) -> Option<Verdict> {
    let payload: serde_json::Value = serde_json::from_str(payload).ok()?;
    let call = normalize(provider, event, &payload);
    let tool = call["tool"].as_str()?.to_ascii_lowercase();
    let input = &call["input"];
    let mut cwd = PathBuf::from(call["cwd"].as_str()?);
    // A shell call can name its own folder; Codex sends `workdir`, AGY `Cwd`.
    if let Some(dir) = ["workdir", "cwd", "Cwd", "directory"]
        .iter()
        .find_map(|key| input[key].as_str())
    {
        cwd = task::resolve(&cwd, Path::new(dir));
    }
    let denied = if ["write", "edit", "replace", "patch"]
        .iter()
        .any(|word| tool.contains(word))
    {
        edit_paths(input)
            .iter()
            .any(|path| task::main_target(&cwd, Path::new(path)).is_some())
    } else {
        let mut text = String::new();
        strings(input, &mut text);
        shell_writes_main(&cwd, &text)
    };
    denied.then(|| Verdict::Deny(MAIN_READ_ONLY.into()))
}

fn strings(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::String(text) => {
            out.push_str(text);
            out.push('\n');
        }
        serde_json::Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        serde_json::Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

/// The files an edit tool names: a value under a key with `path` or `file` in it, and each file
/// header of an `apply_patch` body.
fn edit_paths(input: &serde_json::Value) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(map) = input.as_object() {
        for (key, value) in map {
            let key = key.to_ascii_lowercase();
            if (key.contains("path") || key.contains("file"))
                && let Some(path) = value.as_str()
            {
                paths.push(path.to_string());
            }
        }
    }
    let mut text = String::new();
    strings(input, &mut text);
    for line in text.lines() {
        for header in [
            "*** Add File: ",
            "*** Update File: ",
            "*** Delete File: ",
            "*** Move to: ",
        ] {
            if let Some(path) = line.strip_prefix(header) {
                paths.push(path.trim().to_string());
            }
        }
    }
    paths
}

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    /// An output redirect; the next word is its target.
    Out,
    /// An input redirect or a fd copy; the next word is not a file written.
    In,
}

/// Shell text split into simple commands. Quotes group words, and here-doc bodies are skipped.
fn commands(text: &str) -> Vec<Vec<Token>> {
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
                '\\' if i + 1 < chars.len() => {
                    i += 1;
                    word.push(chars[i]);
                    started = true;
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
                            '&' => {
                                token = Token::In;
                                i += 1;
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
                    if chars.get(i + 1) == Some(&'<') && chars.get(i + 2) != Some(&'<') {
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

fn base(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn shell_writes_main(cwd: &Path, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // Each hub check runs git, so a call outside every hub pays one check at most.
    let in_hub = task::hub(cwd).is_some();
    let guarded = |dir: &Path, path: &str| {
        (in_hub || path.starts_with('/')) && task::main_target(dir, Path::new(path)).is_some()
    };
    if in_hub && text.to_ascii_lowercase().contains("core.hookspath") {
        return true;
    }
    let mut cwd = cwd.to_path_buf();
    for command in commands(text) {
        // Redirect targets, wherever they sit in the command.
        let mut words = Vec::new();
        let mut tokens = command.iter();
        while let Some(token) = tokens.next() {
            match token {
                Token::Out => {
                    if let Some(Token::Word(target)) = tokens.next()
                        && !target.starts_with("/dev/")
                        && guarded(&cwd, target)
                    {
                        return true;
                    }
                }
                Token::In => {
                    tokens.next();
                }
                Token::Word(word) => words.push(word.as_str()),
            }
        }
        let start = words
            .iter()
            .position(|word| {
                !PREFIXES.contains(word) && (!word.contains('=') || word.starts_with('-'))
            })
            .unwrap_or(words.len());
        let Some((program, args)) = words[start..].split_first() else {
            continue;
        };
        let program = base(program);
        // `cd <dir> && …` moves the rest of the call; `cd` with no folder or `-` is not followed.
        if program == "cd" {
            if let Some(dir) = args.iter().find(|arg| !arg.starts_with('-')) {
                cwd = task::resolve(&cwd, Path::new(dir));
            }
            continue;
        }
        if program == "git" {
            if git_writes_main(&cwd, args) {
                return true;
            }
            continue;
        }
        if program == "wt" && in_hub {
            let plain: Vec<&&str> = args.iter().filter(|arg| !arg.starts_with('-')).collect();
            if matches!(plain.as_slice(), [first, ..] if **first == "merge")
                || matches!(plain.as_slice(), [first, second, ..] if **first == "step" && **second == "push")
            {
                return true;
            }
            continue;
        }
        let plain: Vec<&str> = args
            .iter()
            .copied()
            .filter(|arg| !arg.starts_with('-'))
            .collect();
        let targets: Vec<&str> = if WRITERS.contains(&program) {
            plain
                .iter()
                .map(|arg| arg.strip_prefix("of=").unwrap_or(arg))
                .collect()
        } else if COPIERS.contains(&program) {
            plain.last().copied().into_iter().collect()
        } else if ["sed", "perl"].contains(&program)
            && args
                .iter()
                .any(|arg| arg.starts_with("-i") || *arg == "--in-place")
        {
            // The first plain argument is the script.
            plain.iter().skip(1).copied().collect()
        } else {
            Vec::new()
        };
        if targets.iter().any(|target| guarded(&cwd, target)) {
            return true;
        }
    }
    false
}

/// A git command that writes main's worktree, or that moves the default branch from anywhere
/// in the hub.
fn git_writes_main(cwd: &Path, args: &[&str]) -> bool {
    let mut dir = cwd.to_path_buf();
    let mut rest = args;
    while let Some((first, tail)) = rest.split_first() {
        match *first {
            "-C" => {
                let Some((path, tail)) = tail.split_first() else {
                    return false;
                };
                dir = task::resolve(&dir, Path::new(path));
                rest = tail;
            }
            "-c" | "--git-dir" | "--work-tree" | "--namespace" => {
                rest = tail.get(1..).unwrap_or_default();
            }
            option if option.starts_with('-') => rest = tail,
            _ => break,
        }
    }
    let Some((sub, sub_args)) = rest.split_first() else {
        return false;
    };
    let Some(hub) = task::hub(&dir) else {
        return false;
    };
    if GIT_WRITES.contains(sub) && hub.guards(&task::resolve(&dir, Path::new("."))) {
        return true;
    }
    let default = hub.default.as_str();
    let full = format!("refs/heads/{default}");
    let names_default = |arg: &&str| {
        *arg == default
            || *arg == full
            || arg.ends_with(&format!(":{default}"))
            || arg.ends_with(&format!(":{full}"))
    };
    match *sub {
        "update-ref" => sub_args.iter().any(names_default),
        "push" | "fetch" => sub_args.contains(&".") && sub_args.iter().any(names_default),
        "branch" => {
            sub_args.iter().any(|arg| {
                [
                    "-f", "--force", "-M", "-m", "-D", "-d", "-C", "--delete", "--move",
                ]
                .contains(arg)
            }) && sub_args.iter().any(names_default)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_into_main_are_denied_and_writes_elsewhere_pass() {
        let dir = crate::task::tests::hub_fixture("wall");
        let main = dir.join("wt/main");
        let task = dir.join("wt/task");
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&main)
                .args(["worktree", "add", "-q", "-b", "swarm/task"])
                .arg(&task)
                .status()
                .unwrap()
                .success()
        );
        let (main, task) = (main.to_string_lossy(), task.to_string_lossy());
        let claude = |tool: &str, input: serde_json::Value, cwd: &str| {
            let payload = serde_json::json!({"tool_name": tool, "tool_input": input, "cwd": cwd});
            main_wall(Provider::Claude, "PreToolUse", &payload.to_string())
        };
        let write = |path: String| serde_json::json!({"file_path": path, "content": "x"});
        let bash = |command: &str| serde_json::json!({"command": command});

        for (tool, input, cwd) in [
            ("Write", write(format!("{main}/src/a.rs")), task.as_ref()),
            (
                "Edit",
                write(format!("{main}/../main/a.txt")),
                task.as_ref(),
            ),
            ("Bash", bash("echo x > a.txt"), main.as_ref()),
            (
                "Bash",
                bash(&format!("cat <<'EOF' > {main}/b\nhi\nEOF")),
                task.as_ref(),
            ),
            ("Bash", bash("git commit -qam fix"), main.as_ref()),
            (
                "Bash",
                bash(&format!("git -C {main} reset --hard")),
                task.as_ref(),
            ),
            ("Bash", bash("git push . HEAD:main"), task.as_ref()),
            ("Bash", bash("git branch -f main HEAD"), task.as_ref()),
            (
                "Bash",
                bash("sed -i '' s/a/b/ a.txt && echo ok"),
                main.as_ref(),
            ),
            ("Bash", bash("wt merge"), task.as_ref()),
            (
                "Bash",
                bash(&format!("cd {main} && echo x >> a.txt")),
                task.as_ref(),
            ),
        ] {
            assert_eq!(
                claude(tool, input.clone(), cwd),
                Some(Verdict::Deny(MAIN_READ_ONLY.into())),
                "{tool} {input} in {cwd}"
            );
        }
        for (tool, input, cwd) in [
            ("Write", write(format!("{task}/a.txt")), main.as_ref()),
            (
                "Write",
                write(format!("{main}/tmp/flow/01-frame.md")),
                main.as_ref(),
            ),
            ("Read", write(format!("{main}/a.txt")), main.as_ref()),
            (
                "Bash",
                bash("rg -n x src 2>/dev/null > /dev/null; git status; git log -1"),
                main.as_ref(),
            ),
            (
                "Bash",
                bash("echo '> a.txt' && cp a.txt tmp/a.txt"),
                main.as_ref(),
            ),
            (
                "Bash",
                bash(&format!("cat <<'EOF' > {task}/b\nrm a.txt\nEOF")),
                main.as_ref(),
            ),
            (
                "Bash",
                bash("git commit -qam fix && swarm task land"),
                task.as_ref(),
            ),
            (
                "Bash",
                bash(&format!(
                    "cd {task} && printf 'x' >> a.txt && git commit -qam x"
                )),
                main.as_ref(),
            ),
        ] {
            assert_eq!(
                claude(tool, input.clone(), cwd),
                None,
                "{tool} {input} in {cwd}"
            );
        }
        // Codex sends an argument list and its own folder; its patch names files in headers.
        let codex = serde_json::json!({"tool_name": "apply_patch", "cwd": "/", "tool_input":
            {"command": format!("*** Begin Patch\n*** Update File: {main}/a.txt\n*** End Patch")}});
        assert!(main_wall(Provider::Codex, "PreToolUse", &codex.to_string()).is_some());
        let shell = serde_json::json!({"tool_name": "exec_command", "cwd": "/", "tool_input":
            {"cmd": ["bash", "-lc", "rm a.txt"], "workdir": main}});
        assert!(main_wall(Provider::Codex, "PreToolUse", &shell.to_string()).is_some());
    }
}
