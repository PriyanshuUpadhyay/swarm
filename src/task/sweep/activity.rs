//! A live pane protects its session's cwd and the task paths joined by Task.session.

use super::*;
use std::collections::HashMap;

pub(super) struct Activity {
    paths: Vec<(PathBuf, Result<bool, String>)>,
}

impl Activity {
    pub(super) fn check(&self, path: &Path) -> Result<(), String> {
        for (cwd, live) in &self.paths {
            if cwd.starts_with(path) {
                match live {
                    Ok(true) => return Err("a live agent's session uses this worktree".into()),
                    Err(error) => return Err(format!("agent activity is unknown: {error}")),
                    Ok(false) => {}
                }
            }
        }
        Ok(())
    }
}

pub(super) fn read(
    root: &Path,
    hub: &Hub,
    trees: &[WorkspaceFact],
    deadline: Instant,
) -> Result<Activity, String> {
    let connection = rusqlite::Connection::open_with_flags(
        root.join("swarm.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;
    let sessions =
        crate::store::sessions_with_archived(&connection, true).map_err(|e| e.to_string())?;
    let mut tasks: Vec<super::super::Task> = Vec::new();
    match std::fs::read_dir(root.join("tasks")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
        Ok(files) => {
            for file in files {
                let file = file.map_err(|e| e.to_string())?;
                if file.path().extension().is_some_and(|ext| ext == "json") {
                    let bytes = std::fs::read(file.path()).map_err(|e| e.to_string())?;
                    tasks.push(serde_json::from_slice(&bytes).map_err(|e| e.to_string())?);
                }
            }
        }
    }
    let mut paths = Vec::new();
    let mut lists = HashMap::<String, Result<String, String>>::new();
    for session in sessions {
        let mut uses = vec![resolve(Path::new("/"), Path::new(&session.cwd))];
        uses.extend(
            tasks
                .iter()
                .filter(|t| {
                    t.repo == hub.common.to_string_lossy()
                        && t.session.as_deref() == Some(&session.id)
                })
                .map(|t| resolve(Path::new("/"), Path::new(&t.path))),
        );
        uses.retain(|cwd| {
            trees
                .iter()
                .any(|tree| cwd.starts_with(resolve(Path::new("/"), Path::new(&tree.path))))
        });
        if uses.is_empty() {
            continue;
        }
        let agents = crate::store::agents(&connection, &session.id).map_err(|e| e.to_string())?;
        let panes: Vec<_> = agents.iter().filter_map(|a| a.pane.as_deref()).collect();
        if panes.is_empty() {
            continue;
        }
        let name = session.adapter.as_deref().unwrap_or("");
        let listing = lists.entry(name.to_owned()).or_insert_with(|| {
            let mut adapter = crate::adapter::load(root, name).map_err(|e| e.to_string())?;
            adapter.deadline = Some(deadline);
            adapter.session_id = Some(session.id.clone());
            adapter.run("list", &[]).map_err(|e| e.to_string())
        });
        let live = listing
            .as_ref()
            .map(|list| {
                panes
                    .iter()
                    .any(|pane| crate::adapter::listing_has_pane(list, pane))
            })
            .map_err(Clone::clone);
        paths.extend(uses.into_iter().map(|cwd| (cwd, live.clone())));
    }
    Ok(Activity { paths })
}
