//! Read-only Git changes and file previews. Paths never follow workspace symlinks.
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
            Err(Error::Failed("The workspace read timed out".into()))
        } else {
            Ok(seconds)
        }
    }
}
fn bytes(read: &ReadCommands<'_>, args: &[&str]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .args(["-C", read.root, "--literal-pathspecs"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");
    let output = discovery::run_command(&mut command, read.remaining()?).map_err(Error::Failed)?;
    if !output.status.success() {
        return Err(Error::Failed(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ));
    }
    Ok(output.stdout)
}
fn text(read: &ReadCommands<'_>, args: &[&str]) -> Result<String> {
    String::from_utf8(bytes(read, args)?)
        .map_err(|_| Error::Failed("Git returned invalid UTF-8".into()))
}
fn comparison(read: &ReadCommands<'_>, base: Option<&str>) -> Result<Vec<String>> {
    let base = base.ok_or_else(|| Error::Invalid("Select a base ref".into()))?;
    let refs = text(
        read,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let reference = refs
        .lines()
        .find(|r| {
            *r == base
                || r.strip_prefix("refs/heads/") == Some(base)
                || r.strip_prefix("refs/remotes/") == Some(base)
        })
        .ok_or_else(|| Error::Invalid("Select an available base ref".into()))?;
    let oid = text(
        read,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )?;
    let head = text(read, &["rev-parse", "--verify", "HEAD"])?;
    let merge = text(read, &["merge-base", oid.trim(), head.trim()])?;
    Ok(vec![merge.trim().into(), head.trim().into()])
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
    let mut args = diff_args(layer, pair);
    args.extend(format.iter().map(|s| (*s).into()));
    args.push("--".into());
    args.extend(path.map(String::from));
    bytes(read, &args.iter().map(String::as_str).collect::<Vec<_>>())
}
fn stats(data: &[u8]) -> Result<BTreeMap<String, Counts>> {
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
fn identity(read: &ReadCommands<'_>) -> Result<Value> {
    let status = text(
        read,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=no",
        ],
    )?;
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

pub fn list(root: &str, mode: &Mode, base: Option<&str>) -> Result<Value> {
    list_with(&ReadCommands::new(root), mode, base)
}
fn list_with(read: &ReadCommands<'_>, mode: &Mode, base: Option<&str>) -> Result<Value> {
    let before = identity(read)?;
    let mut files = Vec::new();
    let refs = text(
        read,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    let pair = if matches!(mode, Mode::Branch) {
        comparison(read, base)?
    } else {
        Vec::new()
    };
    if matches!(mode, Mode::Branch) {
        let names = diff(read, "branch", &pair, &["--name-status", "-z"], None)?;
        let counts = stats(&diff(read, "branch", &pair, &["--numstat", "-z"], None)?)?;
        let parts: Vec<_> = names.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
        if parts.len() % 2 != 0 {
            return Err(Error::Failed("Git returned incomplete changes".into()));
        }
        for entry in parts.as_chunks::<2>().0 {
            let path = String::from_utf8(entry[1].into())
                .map_err(|_| Error::Failed("Git returned an invalid path".into()))?;
            let (added, removed) = counts.get(&path).copied().unwrap_or((None, None));
            files.push(Change {
                path,
                status: String::from_utf8_lossy(entry[0]).into(),
                layer: "branch".into(),
                added,
                removed,
            });
        }
    } else {
        let status = bytes(
            read,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--no-renames",
                "--untracked-files=all",
            ],
        )?;
        let staged = stats(&diff(read, "staged", &[], &["--numstat", "-z"], None)?)?;
        let unstaged = stats(&diff(read, "unstaged", &[], &["--numstat", "-z"], None)?)?;
        for entry in status.split(|b| *b == 0).filter(|s| !s.is_empty()) {
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
                let (added, removed) = if layer == "untracked" {
                    match untracked(read, path)? {
                        Some(source) => (Some(source.full_lines), Some(0)),
                        None => (None, None),
                    }
                } else {
                    (if layer == "staged" {
                        &staged
                    } else {
                        &unstaged
                    })
                    .get(path)
                    .copied()
                    .unwrap_or((None, None))
                };
                files.push(Change {
                    path: path.into(),
                    status,
                    layer: layer.into(),
                    added,
                    removed,
                });
            }
        }
    }
    if identity(read)? != before
        || (matches!(mode, Mode::Branch) && comparison(read, base)? != pair)
    {
        return Err(Error::Failed(
            "The branch comparison changed during the read. Refresh it".into(),
        ));
    }
    Ok(
        json!({"files":files,"refs":refs.lines().collect::<Vec<_>>(),"comparison":pair,"identity":before}),
    )
}

pub fn patch(
    root: &str,
    path: &str,
    mode: &Mode,
    base: Option<&str>,
    layer: Option<&str>,
) -> Result<Value> {
    validate_path(path)?;
    let read = ReadCommands::new(root);
    let listing = list_with(&read, mode, base)?;
    let candidates: Vec<_> = listing["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["path"] == path && layer.is_none_or(|l| f["layer"] == l))
        .collect();
    let file = candidates
        .first()
        .ok_or_else(|| Error::Invalid("Select a changed file and layer".into()))?;
    if candidates.len() > 1 {
        return Err(Error::Invalid(
            "Select staged or unstaged for this file".into(),
        ));
    }
    let layer = file["layer"].as_str().unwrap();
    let pair: Vec<String> = serde_json::from_value(listing["comparison"].clone()).unwrap();
    let mut full = None;
    let source = if layer == "untracked" {
        let Some(source) = untracked(&read, path)? else {
            return Ok(
                json!({"path":path,"notice":"Binary file or symbolic link","added":file["added"],"removed":file["removed"]}),
            );
        };
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
        full = Some((
            source.full_lines + 4 + usize::from(!ending.is_empty()),
            header.len() + source.full_bytes + source.full_lines + ending.len(),
        ));
        let patch = source
            .text
            .split_inclusive('\n')
            .map(|l| format!("+{l}"))
            .collect::<String>();
        format!(
            "{header}{patch}{}",
            if source.text.len() == source.full_bytes {
                ending
            } else {
                ""
            }
        )
    } else {
        String::from_utf8(diff(&read, layer, &pair, &[], Some(path))?)
            .map_err(|_| Error::Failed("Git returned invalid UTF-8".into()))?
    };
    let mut preview = TextPreview::new(&source, 200);
    if let Some((lines, bytes)) = full {
        preview.is_truncated |= bytes > preview.full_bytes;
        preview.full_lines = lines;
        preview.full_bytes = bytes;
        preview.hidden_lines = lines.saturating_sub(preview.text.lines().count());
    }
    if identity(&read)? != listing["identity"]
        || (matches!(mode, Mode::Branch) && comparison(&read, base)? != pair)
    {
        return Err(Error::Failed(
            "The branch comparison changed during the read. Refresh it".into(),
        ));
    }
    Ok(
        json!({"path":path,"layer":layer,"patch":preview.text,"is_truncated":preview.is_truncated,
        "full_lines":preview.full_lines,"full_bytes":preview.full_bytes,"hidden_lines":preview.hidden_lines,"added":file["added"],"removed":file["removed"]}),
    )
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
    let Some(mut file) = open_file(read.root, path)? else {
        return Ok(None);
    };
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
        prefix.extend_from_slice(&bytes[..size.min(48_000 - prefix.len())]);
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
