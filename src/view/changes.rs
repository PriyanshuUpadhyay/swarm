//! Read-only Git changes and file previews. Paths never follow workspace symlinks.
mod listing;
pub(super) use listing::branch_totals;
use listing::comparison_refs;
pub use listing::list;

use super::{TextPreview, discovery, lifecycle::Error};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    process::Command,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Error>;
type Counts = (Option<usize>, Option<usize>);
type FileCounts = BTreeMap<String, Counts>;
pub const FULL_BYTES: usize = 2 * 1024 * 1024;
pub const FULL_LINES: usize = 20_010;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Local,
    Branch,
}

#[derive(Serialize)]
struct Change {
    path: String,
    status: String,
    layer: String,
    added: Option<usize>,
    removed: Option<usize>,
}

struct ReadCommands<'a> {
    root: &'a str,
    deadline: Instant,
}
impl<'a> ReadCommands<'a> {
    fn new(root: &'a str) -> Self {
        Self {
            root,
            deadline: Instant::now() + Duration::from_secs(20),
        }
    }
    fn remaining(&self) -> Result<u64> {
        let seconds = self
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs();
        if seconds == 0 {
            Err(Error::Read("The workspace read timed out".into()))
        } else {
            Ok(seconds)
        }
    }
}
fn bytes(read: &ReadCommands<'_>, args: &[&str]) -> Result<Vec<u8>> {
    Ok(git_bytes(read, args, false)?.0)
}
fn git_bytes(read: &ReadCommands<'_>, args: &[&str], capped: bool) -> Result<(Vec<u8>, bool)> {
    let mut command = Command::new("git");
    command
        .args(["-C", read.root, "--literal-pathspecs"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");

    let seconds = read.remaining()?;
    let (output, overflow) = if capped {
        discovery::run_command_capped(&mut command, seconds, FULL_BYTES)
    } else {
        discovery::run_command(&mut command, seconds).map(|output| (output, false))
    }
    .map_err(Error::Read)?;

    if !overflow && !output.status.success() {
        return Err(Error::Read(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ));
    }
    Ok((output.stdout, overflow))
}
fn text(read: &ReadCommands<'_>, args: &[&str]) -> Result<String> {
    String::from_utf8(bytes(read, args)?)
        .map_err(|_| Error::Failed("Git returned invalid UTF-8".into()))
}
struct Comparison {
    pair: Vec<String>,
    refs: Vec<String>,
}
fn comparison(read: &ReadCommands<'_>, base: Option<&str>, identity: &Value) -> Result<Comparison> {
    let base = base.ok_or_else(|| Error::Invalid("Select a base ref".into()))?;
    compare_to(read, &comparison_refs(read)?, base, identity)
}
fn compare_to(
    read: &ReadCommands<'_>,
    refs: &str,
    base: &str,
    identity: &Value,
) -> Result<Comparison> {
    let entries: Vec<_> = refs
        .lines()
        .map(|line| line.splitn(3, '\t').collect::<Vec<_>>())
        .collect();
    let entry = entries
        .iter()
        .find(|entry| {
            entry[0] == base
                || entry[0].strip_prefix("refs/heads/") == Some(base)
                || entry[0].strip_prefix("refs/remotes/") == Some(base)
        })
        .ok_or_else(|| Error::Invalid("Select an available base ref".into()))?;
    let oid = if entry.get(2) == Some(&"commit") {
        entry[1].to_owned()
    } else {
        text(
            read,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}^{{commit}}", entry[0]),
            ],
        )?
        .trim()
        .into()
    };
    let head = match identity["head"].as_str() {
        Some(head) => head.to_owned(),
        None => text(read, &["rev-parse", "--verify", "HEAD"])?
            .trim()
            .into(),
    };
    let merge = text(read, &["merge-base", &oid, &head])?;
    Ok(Comparison {
        pair: vec![merge.trim().into(), head],
        refs: entries.iter().map(|entry| entry[0].to_owned()).collect(),
    })
}
fn stable(
    read: &ReadCommands<'_>,
    mode: &Mode,
    base: Option<&str>,
    path: Option<&str>,
    before: &Value,
    pair: &[String],
) -> Result<()> {
    let unchanged = std::thread::scope(|scope| {
        let identity = scope.spawn(|| identity(read, path));
        let comparison = scope.spawn(|| -> Result<Vec<String>> {
            if matches!(mode, Mode::Branch) {
                Ok(comparison(read, base, before)?.pair)
            } else {
                Ok(Vec::new())
            }
        });
        Ok::<_, Error>(
            identity.join().expect("identity read")? == *before
                && comparison.join().expect("comparison read")? == pair,
        )
    })?;
    if !unchanged {
        return Err(Error::Failed(
            "The branch comparison changed during the read. Refresh it".into(),
        ));
    }
    Ok(())
}
fn diff_args(layer: &str, pair: &[String]) -> Vec<String> {
    let mut args: Vec<String> = [
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--color=never",
    ]
    .map(String::from)
    .into();
    match layer {
        "staged" => args.push("--cached".into()),
        "conflicted" => args.push("--cc".into()),
        "branch" => args.extend_from_slice(pair),
        _ => {}
    }
    args
}
fn diff(
    read: &ReadCommands<'_>,
    layer: &str,
    pair: &[String],
    format: &[&str],
    path: Option<&str>,
) -> Result<Vec<u8>> {
    Ok(diff_read(read, layer, pair, format, path, false)?.0)
}
fn diff_read(
    read: &ReadCommands<'_>,
    layer: &str,
    pair: &[String],
    format: &[&str],
    path: Option<&str>,
    capped: bool,
) -> Result<(Vec<u8>, bool)> {
    let mut args = diff_args(layer, pair);
    args.extend(format.iter().map(|s| (*s).into()));
    args.push("--".into());
    args.extend(path.map(String::from));
    git_bytes(
        read,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        capped,
    )
}
fn patch_and_counts(
    read: &ReadCommands<'_>,
    layer: &str,
    pair: &[String],
    path: &str,
    capped: bool,
) -> Result<Option<(String, FileCounts)>> {
    std::thread::scope(|scope| {
        let patch = scope.spawn(|| diff_read(read, layer, pair, &[], Some(path), capped));
        let counts = scope.spawn(|| {
            diff(
                read,
                if layer == "conflicted" {
                    "unstaged"
                } else {
                    layer
                },
                pair,
                &["--numstat", "-z"],
                Some(path),
            )
        });

        let (data, overflow) = patch.join().expect("patch read")?;
        let counts = counts.join().expect("patch counts")?;

        if overflow {
            return Ok(None);
        }
        let patch = String::from_utf8(data)
            .map_err(|_| Error::Failed("Git returned invalid UTF-8".into()))?;
        Ok(Some((patch, stats(&counts)?)))
    })
}
fn stats(data: &[u8]) -> Result<FileCounts> {
    data.split(|b| *b == 0)
        .filter(|v| !v.is_empty())
        .map(|entry| {
            let entry = std::str::from_utf8(entry)
                .map_err(|_| Error::Failed("Git returned an invalid path".into()))?;
            let mut parts = entry.splitn(3, '\t');
            let mut count = || -> Result<Option<usize>> {
                match parts.next() {
                    Some("-") => Ok(None),
                    Some(n) => n
                        .parse()
                        .map(Some)
                        .map_err(|_| Error::Failed("Git returned invalid counts".into())),
                    None => Err(Error::Failed("Git returned incomplete counts".into())),
                }
            };
            let counts = (count()?, count()?);
            let path = parts
                .next()
                .ok_or_else(|| Error::Failed("Git returned no path".into()))?;
            Ok((path.into(), counts))
        })
        .collect()
}
fn identity(read: &ReadCommands<'_>, path: Option<&str>) -> Result<Value> {
    let mut args = vec![
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=no",
    ];
    if let Some(path) = path {
        args.extend(["--", path]);
    }
    let status = text(read, &args)?;
    let head = status
        .lines()
        .find_map(|s| s.strip_prefix("# branch.oid "))
        .filter(|s| *s != "(initial)");
    let branch = status
        .lines()
        .find_map(|s| s.strip_prefix("# branch.head "))
        .filter(|s| *s != "(detached)");
    Ok(json!({"head":head,"branch":branch}))
}

fn local_changes(data: &[u8]) -> Result<Vec<Change>> {
    let mut files = Vec::new();
    for entry in data.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        if entry.len() < 4 || entry[2] != b' ' {
            return Err(Error::Failed("Git returned invalid status".into()));
        }
        let path = std::str::from_utf8(&entry[3..])
            .map_err(|_| Error::Failed("Git returned an invalid path".into()))?;
        let status = &entry[..2];
        let layers = if status == b"??" {
            vec![("untracked", "?".into())]
        } else if [b"DD", b"AU", b"UD", b"UA", b"DU", b"AA", b"UU"]
            .iter()
            .any(|s| s.as_slice() == status)
        {
            vec![("conflicted", String::from_utf8_lossy(status).into_owned())]
        } else {
            [("staged", status[0]), ("unstaged", status[1])]
                .into_iter()
                .filter(|(_, c)| *c != b' ')
                .map(|(l, c)| (l, char::from(c).to_string()))
                .collect()
        };
        for (layer, status) in layers {
            files.push(Change {
                path: path.into(),
                status,
                layer: layer.into(),
                added: None,
                removed: None,
            });
        }
    }
    Ok(files)
}

pub fn patch(
    root: &str,
    path: &str,
    mode: &Mode,
    base: Option<&str>,
    layer: Option<&str>,
) -> Result<Value> {
    Ok(read_patch(root, path, mode, base, layer, false)?.expect("uncapped preview"))
}

/// `None`: the full patch exceeded a cap.
pub fn full_patch(
    root: &str,
    path: &str,
    mode: &Mode,
    base: Option<&str>,
    layer: Option<&str>,
) -> Result<Option<Value>> {
    read_patch(root, path, mode, base, layer, true)
}

fn fits_full(lines: usize, bytes: usize) -> bool {
    lines <= FULL_LINES && bytes <= FULL_BYTES
}

fn read_patch(
    root: &str,
    path: &str,
    mode: &Mode,
    base: Option<&str>,
    layer: Option<&str>,
    expand: bool,
) -> Result<Option<Value>> {
    validate_path(path)?;
    let read = ReadCommands::new(root);
    let before = identity(&read, Some(path))?;

    let pair = if matches!(mode, Mode::Branch) {
        comparison(&read, base, &before)?.pair
    } else {
        Vec::new()
    };

    let (candidates, mut source) = if matches!(mode, Mode::Branch) {
        if expand && layer.is_some_and(|layer| layer != "branch") {
            return Err(Error::Invalid("Select a changed file and layer".into()));
        }
        let Some((source, counts)) = patch_and_counts(&read, "branch", &pair, path, expand)? else {
            return Ok(None);
        };

        let files = counts
            .get(path)
            .map(|&(added, removed)| Change {
                path: path.into(),
                status: String::new(),
                layer: "branch".into(),
                added,
                removed,
            })
            .into_iter()
            .collect();

        (files, Some(source))
    } else {
        let files = local_changes(&bytes(
            &read,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--no-renames",
                "--untracked-files=all",
                "--",
                path,
            ],
        )?)?;

        (files, None)
    };

    let mut candidates: Vec<_> = candidates
        .into_iter()
        .filter(|f: &Change| f.path == path && layer.is_none_or(|l| f.layer == l))
        .collect();
    if candidates.len() > 1 {
        return Err(Error::Invalid(
            "Select staged or unstaged for this file".into(),
        ));
    }
    let mut file = candidates
        .pop()
        .ok_or_else(|| Error::Invalid("Select a changed file and layer".into()))?;
    let layer = file.layer.as_str();

    if !matches!(mode, Mode::Branch) && layer != "untracked" {
        let Some((patch, counts)) = patch_and_counts(&read, layer, &pair, path, expand)? else {
            return Ok(None);
        };
        source = Some(patch);
        (file.added, file.removed) = counts.get(path).copied().unwrap_or((None, None));
    }

    let mut full = None;
    let source = if layer == "untracked" {
        let Some(source) = untracked_source(&read, path, expand)? else {
            return Ok(Some(
                json!({"path":path,"notice":"Binary file or symbolic link","added":file.added,"removed":file.removed}),
            ));
        };

        file.added = Some(source.full_lines);
        file.removed = Some(0);
        let name = serde_json::to_string(&format!("b/{path}")).unwrap();
        let header = format!(
            "diff --git /dev/null {name}\n--- /dev/null\n+++ {name}\n@@ -0,0 +1,{} @@\n",
            source.full_lines
        );
        let ending = if source.full_bytes > 0 && !source.ends_with_newline {
            "\n\\ No newline at end of file\n"
        } else {
            ""
        };

        let lines = source.full_lines + 4 + usize::from(!ending.is_empty());
        let bytes = header.len() + source.full_bytes + source.full_lines + ending.len();
        if expand && !fits_full(lines, bytes) {
            return Ok(None);
        }
        full = Some((lines, bytes));

        let mut patch = header;
        for line in source.text.split_inclusive('\n') {
            patch.push('+');
            patch.push_str(line);
        }
        if source.text.len() == source.full_bytes {
            patch.push_str(ending);
        }
        patch
    } else {
        source.expect("tracked patch")
    };

    let mut cut = false;
    let mut preview = if expand {
        let lines = source.lines().count();
        let bytes = source.len();
        if !fits_full(lines, bytes) {
            return Ok(None);
        }

        TextPreview {
            text: source,
            full_lines: lines,
            full_bytes: bytes,
            ..Default::default()
        }
    } else {
        let p = TextPreview::new(&source, 200);
        cut = p.ends_with_partial_line(&source);
        p
    };

    if let Some((lines, bytes)) = full {
        preview.is_truncated |= bytes > preview.full_bytes;
        preview.full_lines = lines;
        preview.full_bytes = bytes;
        preview.hidden_lines = lines.saturating_sub(preview.text.lines().count());
    }

    stable(&read, mode, base, Some(path), &before, &pair)?;
    let mut reply = json!({"path":path,"layer":layer,"patch":preview.text,"display":super::transcript::patch_wire::changes_patch(&preview.text),"is_truncated":preview.is_truncated,
        "full_lines":preview.full_lines,"full_bytes":preview.full_bytes,"hidden_lines":preview.hidden_lines,"added":file.added,"removed":file.removed});
    if cut {
        reply["partial_last_line"] = json!(true);
    }
    Ok(Some(reply))
}

fn validate_path(path: &str) -> Result<()> {
    if path
        .split('/')
        .any(|p| p.is_empty() || p == "." || p == ".." || p == ".git" || p.contains('\0'))
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}

struct Untracked {
    text: String,
    full_lines: usize,
    full_bytes: usize,
    ends_with_newline: bool,
}
fn untracked(read: &ReadCommands<'_>, path: &str) -> Result<Option<Untracked>> {
    untracked_source(read, path, false)
}
fn untracked_source(read: &ReadCommands<'_>, path: &str, full: bool) -> Result<Option<Untracked>> {
    let Some(mut file) = open_file(read.root, path)? else {
        return Ok(None);
    };
    let limit = if full { FULL_BYTES } else { 48_000 };
    let mut prefix = Vec::new();
    let mut buffer = [0; 8192];
    let (mut full_lines, mut full_bytes, mut ends_with_newline) = (0, 0, false);
    loop {
        read.remaining()?;
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        let bytes = &buffer[..size];
        if bytes.contains(&0) {
            return Ok(None);
        }
        full_bytes += size;
        full_lines += bytes.iter().filter(|b| **b == b'\n').count();
        ends_with_newline = bytes.last() == Some(&b'\n');
        if full && !fits_full(full_lines, full_bytes) {
            break;
        }
        prefix.extend_from_slice(&bytes[..size.min(limit - prefix.len())]);
    }
    full_lines += usize::from(full_bytes > 0 && !ends_with_newline);
    let text = match std::str::from_utf8(&prefix) {
        Ok(text) => text.to_owned(),
        Err(err) if err.error_len().is_none() && full_bytes > prefix.len() => {
            std::str::from_utf8(&prefix[..err.valid_up_to()])
                .unwrap()
                .to_owned()
        }
        Err(_) => return Ok(None),
    };
    Ok(Some(Untracked {
        text,
        full_lines,
        full_bytes,
        ends_with_newline,
    }))
}

fn open_file(root: &str, path: &str) -> Result<Option<File>> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::OpenOptionsExt,
        },
    };
    validate_path(path)?;
    #[cfg(target_os = "macos")]
    let (no_follow, directory_only, non_block, close_on_exec) = (0x100, 0x100000, 0x4, 0x1000000);
    #[cfg(not(target_os = "macos"))]
    let (no_follow, directory_only, non_block, close_on_exec) = (0x20000, 0x10000, 0x800, 0x80000);
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(no_follow | directory_only)
        .open(root)?;
    let parts: Vec<_> = path.split('/').collect();
    unsafe extern "C" {
        fn openat(fd: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
    }
    for (index, part) in parts.iter().enumerate() {
        let final_part = index + 1 == parts.len();
        let name = CString::new(*part).map_err(|_| Error::Forbidden)?;
        // SAFETY: the name is terminated, the parent descriptor stays open, and no creation flag is set.
        let fd = unsafe {
            openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                no_follow | non_block | close_on_exec | if final_part { 0 } else { directory_only },
            )
        };
        if fd < 0 {
            return Ok(None);
        }
        // SAFETY: openat returned a new owned descriptor.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(directory.metadata()?.is_file().then_some(directory))
}
fn read_text(root: &str, path: &str) -> Result<Option<String>> {
    let Some(file) = open_file(root, path)? else {
        return Ok(None);
    };
    let mut source = Vec::new();
    file.take(256 * 1024 + 1).read_to_end(&mut source)?;
    if source.len() > 256 * 1024 || source.contains(&0) {
        return Ok(None);
    }
    Ok(String::from_utf8(source).ok())
}
pub fn file(root: &str, path: &str) -> Result<Value> {
    match read_text(root, path)? {
        Some(source) => Ok(json!({"path":path,"preview":TextPreview::new(&source, 200)})),
        None => Ok(
            json!({"path":path,"notice":"Binary, symbolic link, or file exceeds the 256 KiB preview limit"}),
        ),
    }
}
