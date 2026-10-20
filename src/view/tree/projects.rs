// Build projects and workspaces from snapshot facts.
use super::*;

pub fn build(snapshot: &Value, owner: &OwnerChoices) -> Tree {
    let sessions: Vec<Value> = snapshot["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["archivedAt"].is_null() && s["archived_at_s"].is_null())
        .cloned()
        .collect();
    let mut facts: Vec<ProjectFact> =
        serde_json::from_value(snapshot["projects"].clone()).unwrap_or_default();
    // Fixtures and older snapshots can provide just the project/workspace facts on sessions.
    if facts.is_empty() {
        for session in &sessions {
            let cwd = session["cwd"].as_str().unwrap_or_default();
            let path = session["project"].as_str().unwrap_or(cwd);
            let common = session["common"].as_str().map(str::to_owned);
            let id = common
                .as_ref()
                .map_or_else(|| format!("folder:{path}"), |c| format!("repository:{c}"));
            let index = facts.iter().position(|p| p.id == id).unwrap_or_else(|| {
                facts.push(ProjectFact {
                    id,
                    path: path.into(),
                    launch_directory: path.into(),
                    common,
                    workspaces: Vec::new(),
                });
                facts.len() - 1
            });
            let path = session["workspace"].as_str().unwrap_or(cwd);
            if !facts[index].workspaces.iter().any(|w| w.path == path) {
                facts[index].workspaces.push(WorkspaceFact {
                    path: path.into(),
                    branch: session["branch"].as_str().map(str::to_owned),
                    ..Default::default()
                });
            }
        }
        for path in &owner.project_paths {
            if !facts
                .iter()
                .any(|p| p.path == *path || p.workspaces.iter().any(|w| w.path == *path))
            {
                facts.push(ProjectFact {
                    id: format!("folder:{path}"),
                    path: path.clone(),
                    launch_directory: path.clone(),
                    common: None,
                    workspaces: vec![WorkspaceFact {
                        path: path.clone(),
                        ..Default::default()
                    }],
                });
            }
        }
    }
    let mut projects = Vec::new();
    let mut importable = Vec::new();
    for fact in facts {
        if owner.removed_projects.contains(&fact.path) {
            continue;
        }
        let project_sessions: Vec<_> = sessions
            .iter()
            .filter(|s| {
                s["project_id"] == fact.id
                    || (s["project_id"].is_null()
                        && s["project"]
                            .as_str()
                            .unwrap_or(s["cwd"].as_str().unwrap_or_default())
                            == fact.path)
            })
            .cloned()
            .collect();
        let opened = owner
            .project_paths
            .iter()
            .any(|p| *p == fact.path || fact.workspaces.iter().any(|w| w.path == *p));
        let mut workspaces = Vec::new();
        let mut matched = BTreeSet::new();
        for ws in fact.workspaces.iter().filter(|w| !w.bare) {
            let matching: Vec<_> = project_sessions
                .iter()
                .filter(|s| {
                    if s["workspace"].is_string() {
                        s["workspace"] == ws.path
                    } else {
                        fact.workspaces
                            .iter()
                            .filter(|w| {
                                !w.bare && contains(s["cwd"].as_str().unwrap_or_default(), &w.path)
                            })
                            .max_by_key(|w| w.path.len())
                            .is_some_and(|w| w.path == ws.path)
                    }
                })
                .cloned()
                .collect();
            matched.extend(
                matching
                    .iter()
                    .filter_map(|s| s["id"].as_str())
                    .map(str::to_owned),
            );
            if matching.is_empty() && !ws.missing && !opened {
                continue;
            }
            let (listed, unlisted) = chats(matching, snapshot, owner);
            workspaces.push(Workspace {
                path: ws.path.clone(),
                name: ws.branch.clone().unwrap_or_else(|| basename(&ws.path)),
                branch: ws.branch.clone(),
                missing: ws.missing,
                mark: if ws.locked {
                    Some("locked".into())
                } else if ws.detached {
                    Some("detached".into())
                } else {
                    None
                },
                removed: false,
                chats: listed,
                unlisted,
                archived: archived_chats(snapshot, &ws.path, owner),
            });
        }
        let unmatched: Vec<_> = project_sessions
            .into_iter()
            .filter(|s| !matched.contains(s["id"].as_str().unwrap_or_default()))
            .collect();
        let (removed, hub): (Vec<_>, Vec<_>) = unmatched.into_iter().partition(|s| {
            s["folder_missing"] == true && s["cwd"] != fact.path && s["cwd"] != json!(fact.common)
        });
        let hub_is_workspace = workspaces.iter().any(|w| w.path == fact.path);
        for (removed, sessions) in [(false, hub), (true, removed)] {
            if !sessions.is_empty() {
                let (listed, unlisted) = chats(sessions, snapshot, owner);
                workspaces.push(Workspace {
                    path: if removed {
                        format!("{}#removed", fact.path)
                    } else {
                        fact.path.clone()
                    },
                    name: if removed {
                        "Removed worktrees".into()
                    } else {
                        basename(&fact.path)
                    },
                    branch: None,
                    missing: false,
                    mark: None,
                    removed,
                    chats: listed,
                    unlisted,
                    // A removed worktree's chats keep their own gone folder, so none matches.
                    archived: if removed || hub_is_workspace {
                        Vec::new()
                    } else {
                        archived_chats(snapshot, &fact.path, owner)
                    },
                });
            }
        }
        for w in &workspaces {
            importable.extend(
                w.unlisted
                    .iter()
                    .filter(|c| c.importable)
                    .map(|c| (w.path.clone(), c.clone())),
            );
        }
        if workspaces.is_empty() && !opened {
            continue;
        }
        if !opened && workspaces.iter().all(|w| w.chats.is_empty()) {
            continue;
        }
        let main_path = if fact
            .common
            .as_ref()
            .is_some_and(|c| *c != format!("{}/.git", fact.path))
        {
            workspaces
                .iter()
                .find(|w| w.branch.as_deref() == Some("main"))
                .or_else(|| {
                    workspaces
                        .iter()
                        .find(|w| w.branch.as_deref() == Some("master"))
                })
                .map(|w| w.path.clone())
        } else {
            Some(fact.path.clone())
        };
        order_workspaces(
            &mut workspaces,
            owner
                .workspace_order
                .get(&fact.path)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            main_path.as_deref(),
            &fact.path,
        );
        projects.push(Project {
            id: fact.id,
            name: basename(&fact.path),
            path: fact.path,
            main_path,
            launch_directory: fact.launch_directory,
            workspaces,
        });
    }
    projects.sort_by(|a, b| natural_cmp(&a.path, &b.path));
    Tree {
        projects,
        importable,
    }
}
