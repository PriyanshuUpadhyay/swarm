//! ADR 0135: in a worktree hub, the main worktree is read-only for agents. This wall denies an
//! edit tool call whose file is in main, and a shell call that writes main's files or moves the
//! default branch. Shell text is read best effort: a path the command builds at run time passes.
//! Main's `tmp/` stays writable (`task::SCRATCH`).

use super::*;
use crate::task;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod shell;
use shell::{Token, commands};

pub const MAIN_READ_ONLY: &str = "swarm guard: main is read-only for agents. Run `swarm task start <name>`, edit and commit in the worktree it prints, then run `swarm task land <name>`; so the call is blocked";

/// Git subcommands that change a worktree, its index, or its branch.
/// `update-ref` is not here: it writes no worktree, and the match below checks the ref it moves.
const GIT_WRITES: [&str; 17] = [
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
];
/// Shell programs whose plain arguments are files they write.
const WRITERS: [&str; 12] = [
    "rm", "rmdir", "mv", "touch", "mkdir", "tee", "truncate", "chmod", "chown", "unlink", "patch",
    "dd",
];
/// The options of a writer that take the next word as their value.
fn value_options(program: &str) -> &'static [&'static str] {
    match program {
        "truncate" => &["-s", "-r", "--size", "--reference"],
        "mkdir" => &["-m", "--mode"],
        "touch" => &["-t", "-d", "-r"],
        _ => &[],
    }
}
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
    let hubs = Hubs::default();
    let denied = if ["write", "edit", "replace", "patch"]
        .iter()
        .any(|word| tool.contains(word))
    {
        edit_paths(input)
            .iter()
            .any(|path| hubs.main_target(&cwd, path))
    } else if ["bash", "shell", "command", "exec"]
        .iter()
        .any(|word| tool.contains(word))
    {
        // Only the command is shell text; a description or a prompt is prose.
        let mut text = String::new();
        for key in ["command", "cmd", "CommandLine", "commandLine"] {
            match input[key].as_array() {
                // Codex can send an argument list; quoted, it reads as one command.
                Some(words) => {
                    let words: Vec<String> = words
                        .iter()
                        .filter_map(|word| word.as_str())
                        .map(|word| format!("'{}'", word.replace('\'', r"'\''")))
                        .collect();
                    text.push_str(&words.join(" "));
                    text.push('\n');
                }
                None => strings(&input[key], &mut text),
            }
        }
        shell_writes_main(&hubs, &cwd, &text)
    } else {
        false
    };
    denied.then(|| Verdict::Deny(MAIN_READ_ONLY.into()))
}

/// Hub lookups for one call, by folder, because each costs several git runs and a call can name
/// hundreds of files in a few folders.
#[derive(Default)]
struct Hubs(RefCell<HashMap<PathBuf, Option<task::Hub>>>);

impl Hubs {
    fn of(&self, path: &Path) -> Option<task::Hub> {
        let mut dir = path.to_path_buf();
        while !dir.is_dir() {
            dir = dir.parent()?.to_path_buf();
        }
        self.0
            .borrow_mut()
            .entry(dir)
            .or_insert_with_key(|dir| task::hub(dir))
            .clone()
    }

    /// Whether `path` (resolved against `cwd`) is main's own content.
    fn main_target(&self, cwd: &Path, path: &str) -> bool {
        let resolved = task::resolve(cwd, Path::new(path));
        self.of(&resolved).is_some_and(|hub| hub.guards(&resolved))
    }
}

/// Each string in a JSON value, one per line.
pub(super) fn strings(value: &serde_json::Value, out: &mut String) {
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

fn shell_writes_main(hubs: &Hubs, cwd: &Path, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let mut cwd = cwd.to_path_buf();
    // Outside every hub only an absolute path is looked up, so such a call runs few git checks.
    let mut in_hub = hubs.of(&cwd).is_some();
    // A path the shell expands at run time (`$P/a`, a backtick) cannot be read here, so it passes.
    let guarded = |in_hub: bool, dir: &Path, path: &str| {
        !path.contains(['$', '`'])
            && (in_hub || path.starts_with(['/', '~']))
            && hubs.main_target(dir, path)
    };
    for command in commands(text) {
        // Redirect targets, wherever they sit in the command.
        let mut words = Vec::new();
        let mut tokens = command.iter();
        while let Some(token) = tokens.next() {
            match token {
                Token::Out => {
                    if let Some(Token::Word(target)) = tokens.next()
                        && !target.starts_with("/dev/")
                        && guarded(in_hub, &cwd, target)
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
        // Skip prefix words, their options (`env -i`, `time -p`), and `NAME=value` words.
        let mut start = 0;
        let mut after_prefix = false;
        while let Some(word) = words.get(start) {
            if PREFIXES.contains(word) {
                after_prefix = true;
            } else if !(after_prefix && word.starts_with('-'))
                && (!word.contains('=') || word.starts_with('-'))
            {
                break;
            }
            start += 1;
        }
        let Some((program, args)) = words[start..].split_first() else {
            continue;
        };
        let program = base(program);
        // `cd <dir> && …` moves the rest of the call; `cd` with no folder or `-` is not followed.
        if program == "cd" {
            if let Some(dir) = args.iter().find(|arg| !arg.starts_with('-')) {
                cwd = task::resolve(&cwd, Path::new(dir));
                in_hub = hubs.of(&cwd).is_some();
            }
            continue;
        }
        // `bash -c '<script>'` and `sh -lc …`: the script is shell text too.
        if ["bash", "sh", "zsh", "dash"].contains(&program)
            && let Some(at) = args
                .iter()
                .position(|arg| arg.starts_with('-') && !arg.starts_with("--") && arg.contains('c'))
            && let Some(script) = args.get(at + 1)
            && shell_writes_main(hubs, &cwd, script)
        {
            return true;
        }
        if program == "git" {
            if git_writes_main(hubs, &cwd, args) {
                return true;
            }
            continue;
        }
        if program == "wt" && wt_writes_main(hubs, &cwd, args) {
            return true;
        }
        if program == "wt" {
            continue;
        }
        // Plain arguments, without an option's value (`truncate -s 10`, `mkdir -m 700`).
        let mut plain: Vec<&str> = Vec::new();
        let takes_value = value_options(program);
        let mut skip = false;
        for arg in args {
            if std::mem::take(&mut skip) {
                continue;
            }
            if arg.starts_with('-') {
                skip = takes_value.contains(arg);
            } else {
                plain.push(arg);
            }
        }
        let targets: Vec<&str> = if program == "dd" {
            plain
                .iter()
                .filter_map(|arg| arg.strip_prefix("of="))
                .collect()
        } else if ["chmod", "chown"].contains(&program) {
            // The first plain argument is the mode or the owner, unless a mode such as `-x`
            // came as an option word.
            let dash_mode = args.iter().any(|arg| {
                arg.strip_prefix('-').is_some_and(|mode| {
                    !mode.is_empty() && mode.chars().all(|c| "rwxXst".contains(c))
                })
            });
            plain
                .iter()
                .skip(usize::from(!dash_mode))
                .copied()
                .collect()
        } else if WRITERS.contains(&program) {
            plain
        } else if COPIERS.contains(&program) {
            plain.last().copied().into_iter().collect()
        } else if ["sed", "perl"].contains(&program)
            && args.iter().any(|arg| in_place(program, arg))
        {
            in_place_files(args)
        } else {
            Vec::new()
        };
        if targets.iter().any(|target| guarded(in_hub, &cwd, target)) {
            return true;
        }
    }
    false
}

/// A Worktrunk command that lands into main, writes in it, or removes it. An agent lands with
/// `swarm task land`, so `wt merge` is denied anywhere in a hub.
fn wt_writes_main(hubs: &Hubs, cwd: &Path, args: &[&str]) -> bool {
    let mut dir = cwd.to_path_buf();
    let mut plain = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if *arg == "-C" {
            if let Some(path) = rest.next() {
                dir = task::resolve(&dir, Path::new(path));
            }
        } else if *arg == "--config" {
            rest.next();
        } else if !arg.starts_with('-') {
            plain.push(*arg);
        }
    }
    let Some(hub) = hubs.of(&dir) else {
        return false;
    };
    let in_main = task::resolve(&dir, Path::new(".")).starts_with(&hub.main);
    match plain.as_slice() {
        ["merge" | "promote" | "for-each", ..] => true,
        ["step", "push", ..] => true,
        ["step", ..] => in_main,
        // `wt remove` with no branch removes the worktree it runs in.
        ["remove"] => in_main,
        // A branch, `@` (the worktree it runs in), or a path can name main.
        ["remove", targets @ ..] => targets.iter().any(|target| {
            *target == hub.default
                || (*target == "@" && in_main)
                || task::resolve(&dir, Path::new(target)) == hub.main
        }),
        _ => false,
    }
}

/// Whether `arg` turns on in-place editing: `-i`, `-i.bak`, `--in-place`, or a short-flag cluster
/// whose flags come before `i` (`sed -Ei`, `perl -pi`); `perl -Mstrict` is a module, not `-i`.
fn in_place(program: &str, arg: &str) -> bool {
    let Some(flags) = arg.strip_prefix('-') else {
        return false;
    };
    if flags.starts_with("-in-place") {
        return true;
    }
    // After a letter that takes the rest of the word as its value (`perl -Mstrict`, `sed -es/i/j/`),
    // an `i` is not the flag; before it, any `i` is (`perl -0777pi`, `-CSDpi`, `sed -bi`, BSD `-I`).
    let (value_letters, flag_letters) = if program == "sed" {
        ("efl", "iI")
    } else {
        ("MmIeExdF", "i")
    };
    flags
        .chars()
        .take_while(|c| !value_letters.contains(*c))
        .any(|c| flag_letters.contains(c))
}

/// The files an in-place `sed` or `perl` edits: its plain words, without the script, which is the
/// value of `-e` or `-f`, or else the first plain word.
fn in_place_files<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut files = Vec::new();
    let mut script_option = false;
    let mut words = args.iter();
    while let Some(arg) = words.next() {
        if ["-e", "-f", "--expression", "--file"].contains(arg) {
            script_option = true;
            words.next();
        } else if !arg.starts_with('-') {
            files.push(*arg);
        }
    }
    if !script_option && !files.is_empty() {
        files.remove(0);
    }
    files
}

/// A git command that writes main's worktree, or that moves the default branch from anywhere
/// in the hub.
fn git_writes_main(hubs: &Hubs, cwd: &Path, args: &[&str]) -> bool {
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
            // The work tree a git write changes is the one `--work-tree` names.
            "--work-tree" => {
                let Some((path, tail)) = tail.split_first() else {
                    return false;
                };
                dir = task::resolve(&dir, Path::new(path));
                rest = tail;
            }
            option if option.starts_with("--work-tree=") => {
                dir = task::resolve(&dir, Path::new(&option["--work-tree=".len()..]));
                rest = tail;
            }
            "-c" | "--git-dir" | "--namespace" => {
                rest = tail.get(1..).unwrap_or_default();
            }
            option if option.starts_with('-') => rest = tail,
            _ => break,
        }
    }
    let Some((sub, sub_args)) = rest.split_first() else {
        return false;
    };
    let Some(hub) = hubs.of(&dir) else {
        return false;
    };
    // Main's `tmp/` is scratch for files, but a git write there still changes main.
    let reads = *sub == "stash" && matches!(sub_args.first(), Some(&"list" | &"show"));
    if GIT_WRITES.contains(sub)
        && !reads
        && task::resolve(&dir, Path::new(".")).starts_with(&hub.main)
    {
        return true;
    }
    // A hooks path set or unset turns the ref hook off (or on) for the whole hub.
    if *sub == "config"
        && sub_args
            .iter()
            .any(|arg| arg.eq_ignore_ascii_case("core.hookspath"))
        && sub_args
            .iter()
            .all(|arg| !["--get", "--list", "-l"].contains(arg))
        && (sub_args.iter().filter(|arg| !arg.starts_with('-')).count() > 1
            || sub_args.contains(&"--unset"))
    {
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
    // The ref a command writes is its first plain argument; a later one is only read.
    // `-m <reason>` takes a value, so that word is not the ref.
    let mut words = sub_args.iter();
    let mut first_plain = None;
    while let Some(arg) = words.next() {
        if *arg == "-m" {
            words.next();
        } else if !arg.starts_with('-') {
            first_plain = Some(arg);
            break;
        }
    }
    let has = |flags: &[&str]| sub_args.iter().any(|arg| flags.contains(arg));
    match *sub {
        "worktree" if matches!(sub_args.first(), Some(&"remove" | &"move")) => sub_args[1..]
            .iter()
            .filter(|arg| !arg.starts_with('-'))
            .any(|arg| task::resolve(&dir, Path::new(arg)) == hub.main),
        "update-ref" => first_plain.is_some_and(names_default),
        // Setting the hub's or main's HEAD changes which branch is the default; a task worktree's
        // HEAD is its own.
        "symbolic-ref" => {
            let here = task::resolve(&dir, Path::new("."));
            (here.starts_with(&hub.main) || here.starts_with(&hub.common))
                && sub_args.iter().filter(|arg| !arg.starts_with('-')).count() > 1
        }
        "remote" => sub_args.first() == Some(&"set-head"),
        "push" | "fetch" => sub_args.contains(&".") && sub_args.iter().any(names_default),
        // A rename or a delete changes the default branch when it names it anywhere.
        "branch" if has(&["-M", "-m", "-D", "-d", "--delete", "--move"]) => {
            sub_args.iter().any(names_default)
        }
        "branch" if has(&["-f", "--force", "-C"]) => first_plain.is_some_and(names_default),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
