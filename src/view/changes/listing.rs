// List local and branch changes with line counts.
use super::*;
use crate::view::fields::{LineTotals, MAX_SAFE_JSON_INTEGER};
pub fn list(root: &str, mode: &Mode, base: Option<&str>) -> Result<Value> {
    list_with(&ReadCommands::new(root), mode, base)
}
fn list_with(read: &ReadCommands<'_>, mode: &Mode, base: Option<&str>) -> Result<Value> {
    let before = identity(read, None)?;
    let mut files = Vec::new();
    let (refs, pair) = if matches!(mode, Mode::Branch) {
        let comparison = comparison(read, base, &before)?;
        (comparison.refs, comparison.pair)
    } else {
        (Vec::new(), Vec::new())
    };
    let refs = if matches!(mode, Mode::Branch) {
        let (names, counts) = std::thread::scope(|scope| {
            let names = scope.spawn(|| diff(read, "branch", &pair, &["--name-status", "-z"], None));
            let counts = scope.spawn(|| diff(read, "branch", &pair, &["--numstat", "-z"], None));
            Ok::<_, Error>((
                names.join().expect("change names")?,
                stats(&counts.join().expect("change counts")?)?,
            ))
        })?;
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
        refs
    } else {
        let (refs, status, staged, unstaged) = std::thread::scope(|scope| {
            let refs = scope.spawn(|| {
                text(
                    read,
                    &[
                        "for-each-ref",
                        "--format=%(refname)",
                        "refs/heads",
                        "refs/remotes",
                    ],
                )
            });
            let status = scope.spawn(|| {
                bytes(
                    read,
                    &[
                        "status",
                        "--porcelain=v1",
                        "-z",
                        "--no-renames",
                        "--untracked-files=all",
                    ],
                )
            });
            let staged = scope.spawn(|| diff(read, "staged", &[], &["--numstat", "-z"], None));
            let unstaged = scope.spawn(|| diff(read, "unstaged", &[], &["--numstat", "-z"], None));
            Ok::<_, Error>((
                refs.join().expect("refs read")?,
                status.join().expect("status read")?,
                stats(&staged.join().expect("staged counts")?)?,
                stats(&unstaged.join().expect("unstaged counts")?)?,
            ))
        })?;
        for mut file in local_changes(&status)? {
            let (added, removed) = if file.layer == "untracked" {
                match untracked(read, &file.path)? {
                    Some(source) => (Some(source.full_lines), Some(0)),
                    None => (None, None),
                }
            } else {
                (if file.layer == "staged" {
                    &staged
                } else {
                    &unstaged
                })
                .get(&file.path)
                .copied()
                .unwrap_or((None, None))
            };
            file.added = added;
            file.removed = removed;
            files.push(file);
        }
        refs.lines().map(str::to_owned).collect()
    };
    stable(read, mode, base, None, &before, &pair)?;
    Ok(json!({"files":files,"refs":refs,"comparison":pair,"identity":before}))
}

pub(super) fn comparison_refs(read: &ReadCommands<'_>) -> Result<String> {
    text(
        read,
        &[
            "for-each-ref",
            "--format=%(refname)%09%(objectname)%09%(objecttype)",
            "refs/heads",
            "refs/remotes",
        ],
    )
}

pub(in crate::view) fn branch_totals(
    root: &str,
    branch: &str,
    deadline: Instant,
) -> Option<LineTotals> {
    let mut read = ReadCommands::new(root);
    read.deadline = read.deadline.min(deadline);
    let before = identity(&read, None).ok()?;
    if before["branch"] != branch {
        return None;
    }

    let refs = comparison_refs(&read).ok()?;
    let own = format!("refs/heads/{branch}");
    let names = || {
        refs.lines()
            .filter_map(|line| line.split_once('\t').map(|(name, _)| name))
    };
    // Match the Changes pane's defaultBase order, not its current user selection.
    let base = [
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
        "refs/remotes/origin/master",
        "refs/heads/main",
        "refs/heads/master",
    ]
    .into_iter()
    .find(|name| *name != own && names().any(|r| r == *name))
    .or_else(|| names().find(|r| *r != own))?;

    let pair = compare_to(&read, &refs, base, &before).ok()?.pair;
    let counts = stats(&diff(&read, "branch", &pair, &["--numstat", "-z"], None).ok()?).ok()?;
    stable(&read, &Mode::Branch, Some(base), None, &before, &pair).ok()?;

    let total = |sum: u64, count: Option<usize>| {
        let sum = sum.checked_add(count.unwrap_or(0) as u64)?;
        (sum <= MAX_SAFE_JSON_INTEGER).then_some(sum)
    };
    let (added_lines, removed_lines) = counts
        .into_values()
        .try_fold((0, 0), |(added, removed), (a, r)| {
            Some((total(added, a)?, total(removed, r)?))
        })?;
    Some(LineTotals {
        added_lines,
        removed_lines,
    })
}
