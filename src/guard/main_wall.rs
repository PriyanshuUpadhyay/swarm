//! ADR 0135: in a worktree hub, the main worktree is read-only for agents. This wall denies an
//! edit tool call whose file is in main, and a shell call that writes main's files or moves the
//! default branch. Shell text is read best effort: a path the command builds at run time passes.
//! Main's `tmp/` stays writable (`task::SCRATCH`).

use super::*;
use crate::task;
use std::path::{Path, PathBuf};

mod shell;
use shell::{Token, commands};

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

fn base(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn shell_writes_main(cwd: &Path, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // Each hub check runs git, so a call outside every hub pays one check at most.
    let in_hub = task::hub(cwd).is_some();
    // A path the shell expands at run time (`$P/a`, a backtick) cannot be read here, so it passes.
    let guarded = |dir: &Path, path: &str| {
        !path.contains(['$', '`'])
            && (in_hub || path.starts_with('/'))
            && task::main_target(dir, Path::new(path)).is_some()
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
mod tests;
