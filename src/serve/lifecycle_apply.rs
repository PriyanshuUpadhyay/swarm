//! Server lifecycle apply.

use super::*;
pub(super) fn apply_lifecycle(
    app: &App,
    action: LifecycleAction,
    tree: &tree::Tree,
    saved: &choices_store::Saved,
    live: usize,
) -> Result<Value, ApiError> {
    let archive = matches!(action, LifecycleAction::Archive(_));
    let mut result = json!({"ok":true});
    match action {
        LifecycleAction::Options(p) => {
            let project = lifecycle::project(tree, &p.project).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.project, true).map_err(lifecycle_error)?;
            if let Some(common) = lifecycle::common(project) {
                lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            }
            return lifecycle::options(project, &saved.owner).map_err(lifecycle_error);
        }
        LifecycleAction::Impact(p) => {
            let (project, workspace) =
                lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            return lifecycle::impact(project, workspace, live).map_err(lifecycle_error);
        }
        LifecycleAction::New(mut request) => {
            let project = lifecycle::project(tree, &request.project).map_err(lifecycle_error)?;
            if matches!(request.mode, lifecycle::Mode::Issue) {
                lifecycle::checked_path(&app.root, &request.project, true)
                    .map_err(lifecycle_error)?;
                let common = lifecycle::common(project)
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Issues need a Git project"))?;
                lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
                let number = request
                    .issue
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Select an issue"))?;
                let issue = workspace_panel::issue(common, number).map_err(lifecycle_error)?;
                request.name = format!("{number}-{}", issue["title"].as_str().unwrap());
                result["issue"] = issue;
            }
            let (path, settings) =
                lifecycle::create_workspace(&app.root, project, &saved.owner, &request)
                    .map_err(lifecycle_error)?;
            result["workspace"] = path.clone().into();
            let recorded = choices_store::update(
                &app.root,
                None,
                |owner: &mut choices::OwnerChoices, state: &mut choices::ViewState| {
                    lifecycle::remember(owner, &path, &project.path);
                    owner
                        .project_defaults
                        .insert(project.path.clone(), settings);
                    owner.names.insert(path.clone(), request.name.trim().into());
                    state.selected_workspace = Some(path.clone());
                    state.selected_child = None;
                    Ok(())
                },
            )
            .map_err(choice_error);
            if let Err(err) = recorded {
                result["notice"] = format!("The workspace at {path} exists, so do not create it again. Swarm could not record it. {}",err.1.0).into();
            }
        }
        LifecycleAction::Archive(p) | LifecycleAction::Restore(p) => {
            lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.workspace, false).map_err(lifecycle_error)?;
            choices_store::update(&app.root, None, |owner, state| {
                if archive {
                    owner.archived.insert(p.workspace.clone());
                    lifecycle::clear_selection(state, &p.workspace);
                } else {
                    owner.archived.remove(&p.workspace);
                }
                Ok(())
            })
            .map_err(choice_error)?;
            // An archived workspace is read-only, so its shells end with it.
            if archive {
                shells::close_all(&p.workspace);
            }
        }
        LifecycleAction::Delete(p) => {
            let (project, workspace) =
                lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.workspace, true).map_err(lifecycle_error)?;
            if !lifecycle::can_delete(project, workspace) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "This workspace cannot be deleted",
                ));
            }
            if let Some(blocker) =
                lifecycle::removal_blocker(&p.workspace).map_err(lifecycle_error)?
            {
                return Err(error(StatusCode::CONFLICT, &blocker));
            }
            let common = lifecycle::common(project)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "not a Git project"))?;
            lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            shells::close_all(&p.workspace);
            lifecycle::check_git(common, &["worktree", "remove", "--", &p.workspace])
                .map_err(lifecycle_error)?;
            choices_store::update(&app.root, None, |owner, state| {
                owner.pinned.remove(&p.workspace);
                owner.archived.remove(&p.workspace);
                owner.names.remove(&p.workspace);
                owner.tabs.remove(&p.workspace);
                owner.project_paths.retain(|path| path != &p.workspace);
                if !owner.project_paths.contains(&project.path) {
                    owner.project_paths.push(project.path.clone());
                }
                if let Some(order) = owner.workspace_order.get_mut(&project.path) {
                    order.retain(|path| path != &p.workspace);
                }
                lifecycle::clear_selection(state, &p.workspace);
                Ok(())
            })
            .map_err(choice_error)?;
        }
        LifecycleAction::Prune(p) => {
            let (project, workspace) =
                lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            if !workspace.missing || workspace.removed {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "Only a missing worktree can be pruned",
                ));
            }
            lifecycle::checked_path(&app.root, &p.workspace, false).map_err(lifecycle_error)?;
            let common = lifecycle::common(project)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "not a Git project"))?;
            lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            lifecycle::check_git(common, &["worktree", "prune"]).map_err(lifecycle_error)?;
        }
        LifecycleAction::Create(request) => {
            if request.name.trim().is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "Enter a project name"));
            }
            let path = lifecycle::checked_path(&app.root, &request.path, false)
                .map_err(lifecycle_error)?;
            // 409 only for a path that exists, so the page can offer Import in its place.
            let exists = || {
                error(
                    StatusCode::CONFLICT,
                    "A file or folder already exists at this path",
                )
            };
            if std::fs::symlink_metadata(&path).is_ok() {
                return Err(exists());
            }
            std::fs::create_dir(&path).map_err(|e| match e.kind() {
                std::io::ErrorKind::AlreadyExists => exists(),
                _ => error(StatusCode::BAD_REQUEST, &e.to_string()),
            })?;
            let path = path.to_string_lossy().into_owned();
            let existed = saved.owner.project_paths.contains(&path);
            let created = (|| {
                choices_store::update(&app.root, None, |owner, _| {
                    lifecycle::remember(owner, &path, &path);
                    if request.name.trim() != tree::basename(&path) {
                        owner
                            .project_names
                            .insert(path.clone(), request.name.trim().into());
                    }
                    Ok(())
                })
                .map_err(choice_error)?;
                lifecycle::initialize_project(&path).map_err(lifecycle_error)
            })();
            let first_commit = match created {
                Ok(commit) => commit,
                Err(err) => {
                    if std::fs::read_dir(&path)
                        .ok()
                        .is_some_and(|mut entries| entries.next().is_none())
                        && std::fs::remove_dir(&path).is_ok()
                    {
                        let _ = choices_store::update(&app.root, None, |owner, _| {
                            if !existed {
                                owner.project_paths.retain(|p| p != &path);
                            }
                            if let Some(name) = saved.owner.project_names.get(&path) {
                                owner.project_names.insert(path.clone(), name.clone());
                            } else {
                                owner.project_names.remove(&path);
                            }
                            Ok(())
                        })
                        .map_err(choice_error);
                    }
                    return Err(err);
                }
            };
            result["path"] = path.into();
            if let Some(notice) = first_commit.notice() {
                result["notice"] = notice.into();
            }
            result["firstCommit"] = serde_json::to_value(first_commit).unwrap();
        }
        LifecycleAction::Import(request) => {
            let path = lifecycle::checked_path(&app.root, &request.path, true)
                .map_err(lifecycle_error)?
                .to_string_lossy()
                .into_owned();
            let repository = lifecycle::is_repository(&path).map_err(lifecycle_error)?;
            if !repository && request.kind.is_none() && !saved.owner.plain_folders.contains(&path) {
                return Ok(
                    json!({"ok":true,"needsChoice":true,"path":path,"choices":["git","folder"]}),
                );
            }
            if !repository && matches!(request.kind, Some(lifecycle::ImportAs::Git)) {
                lifecycle::check_git(&path, &["init", "-q"]).map_err(lifecycle_error)?;
            }
            let project = lifecycle::project_path(&path);
            choices_store::update(&app.root, None, |owner, _| {
                lifecycle::remember(owner, &path, &project);
                if !repository && !matches!(request.kind, Some(lifecycle::ImportAs::Git)) {
                    owner.plain_folders.insert(path.clone());
                } else {
                    owner.plain_folders.remove(&path);
                }
                Ok(())
            })
            .map_err(choice_error)?;
            result["path"] = path.into();
            result["needsChoice"] = false.into();
        }
        LifecycleAction::Remove(p) => {
            let project = lifecycle::project(tree, &p.project).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.project, false).map_err(lifecycle_error)?;
            let paths: Vec<_> = saved
                .owner
                .project_paths
                .iter()
                .filter(|p| *p == &project.path || lifecycle::project_path(p) == project.path)
                .cloned()
                .collect();
            choices_store::update(&app.root, None, |owner, state| {
                lifecycle::remove_project(owner, state, project, &paths);
                Ok(())
            })
            .map_err(choice_error)?;
        }
    }
    app.directories.lock().unwrap().clear();
    app.tree_cache.lock().unwrap().discovery.forget_catalog();
    app.hub.lock().unwrap().bump();
    if let Err(err) = refresh_tree(app) {
        result["notice"] = format!(
            "Saved, but the workspace list could not refresh. {}",
            err.1.0
        )
        .into();
    }
    if let Ok(saved) = choices_store::load(&app.root, false) {
        result["revision"] = saved.revision.into();
    }
    result["version"] = app.hub.lock().unwrap().version.into();
    Ok(result)
}
