//! Slow filesystem and subprocess reads, kept outside the pure view models.
mod titles;
use titles::read_title;

use super::{
    choices::OwnerChoices,
    fields::{self, WorkspaceFields},
    tree::{self, ProjectFact, WorkspaceFact},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Drain stdout while waiting, and reap the child on timeout.
pub fn run(program: &str, args: &[&str], seconds: u64) -> Option<Vec<u8>> {
    let mut command = Command::new(program);
    command.args(args);
    run_command(&mut command, seconds)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| output.stdout)
}

pub fn run_command(command: &mut Command, seconds: u64) -> Result<Output, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let (output, receive) = std::sync::mpsc::channel();
    let out_sender = output.clone();
    let out_read = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = out_sender.send((true, stdout.read_to_end(&mut bytes).map(|_| bytes)));
    });
    let err_read = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = output.send((false, stderr.read_to_end(&mut bytes).map(|_| bytes)));
    });
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let pid = child.id();
    let (finished, completion) = std::sync::mpsc::channel();
    let wait = std::thread::spawn(move || {
        let status = child.wait();
        let _ = finished.send(());
        (child, status)
    });
    let timed_out = completion
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .is_err();
    if timed_out {
        stop_group(pid);
    }
    let (mut child, status) = wait.join().expect("command wait thread");
    let status = if timed_out || status.is_err() {
        stop(&mut child);
        Err("command timed out or could not be read".to_owned())
    } else {
        status.map_err(|e| e.to_string())
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut failure = None;
    for _ in 0..2 {
        match receive.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok((out, Ok(bytes))) => {
                if out {
                    stdout = bytes;
                } else {
                    stderr = bytes;
                }
            }
            other => {
                stop(&mut child);
                failure = Some(format!("command output failed: {other:?}"));
                break;
            }
        }
    }
    let _ = out_read.join();
    let _ = err_read.join();
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(Output {
        status: status?,
        stdout,
        stderr,
    })
}
fn stop(child: &mut std::process::Child) {
    stop_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}
fn stop_group(pid: u32) {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        // SAFETY: this child owns its process group; kill accepts plain integers.
        unsafe {
            kill(-(pid as i32), 9);
        }
    }
}
fn git(path: &str, args: &[&str]) -> Option<String> {
    let mut flags = vec!["-C", path];
    flags.extend(args);
    String::from_utf8(run("git", &flags, 5)?)
        .ok()
        .map(|s| s.trim().into())
}
pub fn resolve(path: &str) -> Option<String> {
    // A nested bare hub takes precedence over Git's surrounding checkout discovery.
    for ancestor in Path::new(path).ancestors() {
        let bare = ancestor.join(".bare");
        if bare.is_dir() {
            return Some(bare.to_string_lossy().into_owned());
        }
        let git = ancestor.join(".git");
        if git.is_dir() {
            return Some(git.to_string_lossy().into_owned());
        }
        if let Ok(pointer) = std::fs::read_to_string(&git)
            && let Some(directory) = pointer.trim().strip_prefix("gitdir: ")
        {
            let directory = ancestor.join(directory);
            if let Ok(common) = std::fs::read_to_string(directory.join("commondir")) {
                return std::fs::canonicalize(directory.join(common.trim()))
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned());
            }
        }
    }
    git(
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
}

/// The project a folder belongs to. All worktrees of one repository share the id, so the app's
/// project list and the project board agree on it.
pub fn project_id(path: &str) -> String {
    id_for(path, resolve(path).as_deref())
}
fn id_for(path: &str, common: Option<&str>) -> String {
    common.map_or_else(|| format!("folder:{path}"), |c| format!("repository:{c}"))
}

pub fn parse_worktrees(bytes: &[u8]) -> Vec<WorkspaceFact> {
    let mut result = Vec::new();
    let mut current: Option<WorkspaceFact> = None;
    for token in bytes.split(|b| *b == 0) {
        let line = String::from_utf8_lossy(token);
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(value) = current.take() {
                result.push(value);
            }
            current = Some(WorkspaceFact {
                path: path.into(),
                ..Default::default()
            });
        } else if let Some(value) = &mut current {
            if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                value.branch = Some(branch.into());
            } else if line == "bare" {
                value.bare = true;
            } else if line == "detached" {
                value.detached = true;
            } else if line == "locked" || line.starts_with("locked ") {
                value.locked = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                value.missing = true;
            }
        }
    }
    if let Some(value) = current {
        result.push(value);
    }
    result
}
#[derive(Clone, Default)]
pub struct Discovery {
    catalog_at: Option<i64>,
    catalog_paths: BTreeSet<String>,
    catalog: Vec<ProjectFact>,
    /// Per session: the log, its title, when it was read, and the log's mtime then.
    titles: BTreeMap<String, (String, Option<String>, i64, Option<u128>)>,
    pub fields: BTreeMap<String, WorkspaceFields>,
    dirty_at: BTreeMap<String, i64>,
    github_at: BTreeMap<String, i64>,
}
impl Discovery {
    pub fn forget_catalog(&mut self) {
        self.catalog_at = None;
    }
    pub fn augment(&mut self, snapshot: &mut Value, owner: &OwnerChoices, at: i64) {
        let paths: BTreeSet<String> = snapshot["sessions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["cwd"].as_str().map(str::to_owned))
            .chain(owner.project_paths.iter().cloned())
            .collect();
        if self.catalog_paths != paths || self.catalog_at.is_none_or(|last| at - last >= 10) {
            self.catalog_at = Some(at);
            self.catalog_paths = paths.clone();
            let mut projects: BTreeMap<String, ProjectFact> = BTreeMap::new();
            for path in paths {
                let common = resolve(&path);
                let project_path = common
                    .as_ref()
                    .map(|c| {
                        if [".git", ".bare"].contains(&tree::basename(c).as_str()) {
                            Path::new(c)
                                .parent()
                                .unwrap()
                                .to_string_lossy()
                                .into_owned()
                        } else {
                            c.clone()
                        }
                    })
                    .unwrap_or_else(|| path.clone());
                let id = id_for(&path, common.as_deref());
                let project = projects.entry(id.clone()).or_insert_with(|| {
                    let workspaces = if let Some(common) = &common {
                        run(
                            "git",
                            &["-C", common, "worktree", "list", "--porcelain", "-z"],
                            5,
                        )
                        .map(|b| parse_worktrees(&b))
                        .unwrap_or_default()
                    } else {
                        vec![WorkspaceFact {
                            path: path.clone(),
                            ..Default::default()
                        }]
                    };
                    let launch_directory = workspaces
                        .iter()
                        .find(|w| w.branch.as_deref() == Some("main") && !w.bare)
                        .or_else(|| workspaces.iter().find(|w| !w.bare))
                        .map(|w| w.path.clone())
                        .unwrap_or_else(|| project_path.clone());
                    ProjectFact {
                        id,
                        path: project_path,
                        common,
                        launch_directory,
                        workspaces,
                    }
                });
                if owner.project_paths.contains(&path) {
                    project.launch_directory = path;
                }
            }
            self.catalog = projects.into_values().collect();
        }
        snapshot["projects"] = serde_json::to_value(&self.catalog).unwrap();
        // Archived sessions get the same facts, so the sidebar folds them under their workspace.
        for key in ["sessions", "archived"] {
            let Some(sessions) = snapshot[key].as_array_mut() else {
                continue;
            };
            for session in sessions {
                let cwd = session["cwd"].as_str().unwrap_or_default().to_owned();
                // A catalog workspace at the deepest path wins over a surrounding plain folder.
                let project = self
                    .catalog
                    .iter()
                    .filter(|p| {
                        p.path == cwd
                            || p.common.as_deref() == Some(&cwd)
                            || p.workspaces.iter().any(|w| tree::contains(&cwd, &w.path))
                            || (p.common.is_some() && tree::contains(&cwd, &p.path))
                    })
                    .max_by_key(|p| {
                        p.workspaces
                            .iter()
                            .filter(|w| tree::contains(&cwd, &w.path))
                            .map(|w| w.path.len())
                            .max()
                            .unwrap_or(p.path.len())
                    });
                if let Some(project) = project {
                    session["project_id"] = Value::String(project.id.clone());
                    session["project"] = Value::String(project.path.clone());
                    session["workspace"] = Value::String(
                        project
                            .workspaces
                            .iter()
                            .filter(|w| !w.bare && tree::contains(&cwd, &w.path))
                            .max_by_key(|w| w.path.len())
                            .map(|w| w.path.clone())
                            .unwrap_or(cwd.clone()),
                    );
                    session["folder_missing"] = Value::Bool(!Path::new(&cwd).exists());
                }
                if let Some(log) = session["chair_log"].as_str().map(str::to_owned) {
                    let modified = std::fs::metadata(&log)
                        .and_then(|m| m.modified())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
                        .ok();
                    if let Some(modified) = modified {
                        session["chair_log_activity"] = Value::from(modified.as_secs());
                    }
                    let modified = modified.map(|m| m.as_nanos());
                    let id = session["id"].as_str().unwrap_or_default().to_owned();
                    // A chat with no title yet reads its log on each change; the agent can retitle a
                    // titled one, so that is read again at most every 30 s while the log grows.
                    if !self
                        .titles
                        .get(&id)
                        .is_some_and(|(path, title, last, seen)| {
                            *path == log
                                && (*seen == modified || (title.is_some() && at - last < 30))
                        })
                    {
                        self.titles
                            .insert(id.clone(), (log.clone(), read_title(&log), at, modified));
                    }
                    if let Some((_, Some(title), _, _)) = self.titles.get(&id) {
                        session["title"] = Value::String(title.clone());
                    }
                }
            }
        }
    }

    /// Reserve before a read, including failures. The coordinator owns the cache.
    pub fn refresh_fields(&mut self, tree: &tree::Tree, owner: &OwnerChoices, at: i64) -> bool {
        self.refresh_fields_with(
            tree,
            owner,
            at,
            |path| {
                run("git", &["-C", path, "status", "--porcelain=v1", "-z"], 5)
                    .map(|b| dirty_count(&b))
            },
            super::github::lookup,
        )
    }

    pub fn refresh_fields_with(
        &mut self,
        tree: &tree::Tree,
        owner: &OwnerChoices,
        at: i64,
        mut inspect: impl FnMut(&str) -> Option<usize>,
        mut lookup: impl FnMut(&str) -> Option<Value>,
    ) -> bool {
        let dirty: BTreeSet<_> = fields::requested_paths("dirty", tree, &owner.fields)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let github: BTreeSet<_> = fields::requested_paths("pr", tree, &owner.fields)
            .into_iter()
            .chain(fields::requested_paths("ci", tree, &owner.fields))
            .map(str::to_owned)
            .collect();
        let previous = self.fields.clone();
        for path in dirty.union(&github) {
            let read_dirty =
                dirty.contains(path) && self.dirty_at.get(path).is_none_or(|last| at - last >= 10);
            let read_github = github.contains(path)
                && self.github_at.get(path).is_none_or(|last| at - last >= 120);
            if read_dirty {
                self.dirty_at.insert(path.clone(), at);
            }
            if read_github {
                self.github_at.insert(path.clone(), at);
            }
            if read_dirty {
                self.fields.entry(path.clone()).or_default().dirty_count = inspect(path);
            }
            if read_github {
                let request = lookup(path);
                let fields = request
                    .as_ref()
                    .map(fields::github_fields)
                    .unwrap_or_default();
                let current = self.fields.entry(path.clone()).or_default();
                current.pr = fields.pr;
                current.ci = fields.ci;
            }
        }
        previous != self.fields
    }
}
pub fn dirty_count(bytes: &[u8]) -> usize {
    let mut paths = BTreeSet::new();
    let mut chunks = bytes.split(|b| *b == 0);
    while let Some(line) = chunks.next() {
        if line.len() < 4 {
            continue;
        }
        paths.insert(line[3..].to_vec());
        if line[0] == b'R' || line[1] == b'R' || line[0] == b'C' || line[1] == b'C' {
            chunks.next();
        }
    }
    paths.len()
}
