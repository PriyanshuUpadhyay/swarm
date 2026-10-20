//! Server lifecycle routes.

use super::*;
#[derive(Deserialize)]
pub(super) enum LifecycleAction {
    Options(ProjectPath),
    Impact(WorkspacePath),
    New(lifecycle::NewWorkspace),
    Archive(WorkspacePath),
    Restore(WorkspacePath),
    Delete(WorkspacePath),
    Prune(WorkspacePath),
    Create(lifecycle::CreateProject),
    Import(lifecycle::ImportProject),
    Remove(ProjectPath),
}
pub(super) fn lifecycle_error(err: lifecycle::Error) -> ApiError {
    let status = match err {
        lifecycle::Error::Forbidden => StatusCode::FORBIDDEN,
        lifecycle::Error::Invalid(_) => StatusCode::BAD_REQUEST,
        lifecycle::Error::Failed(_) => StatusCode::CONFLICT,
    };
    error(status, &err.to_string())
}
pub(super) async fn lifecycle_read(
    State(app): State<App>,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let value = serde_json::to_value(query).unwrap();
    let action = match uri.path() {
        "/api/workspace/new-options" => LifecycleAction::Options(
            serde_json::from_value(value)
                .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?,
        ),
        _ => LifecycleAction::Impact(
            serde_json::from_value(value)
                .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?,
        ),
    };
    enqueue_action(&app, Action::Lifecycle(action))
        .await
        .map(Json)
}
pub(super) async fn lifecycle_write(
    State(app): State<App>,
    uri: Uri,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                e.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    let kind = match uri.path() {
        "/api/workspace/new" => "New",
        "/api/workspace/archive" => "Archive",
        "/api/workspace/restore" => "Restore",
        "/api/workspace/delete" => "Delete",
        "/api/workspace/prune" => "Prune",
        "/api/project/create" => "Create",
        "/api/project/import" => "Import",
        _ => "Remove",
    };
    let action = serde_json::from_value(json!({kind:body}))
        .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
    enqueue_action(&app, Action::Lifecycle(action))
        .await
        .map(Json)
}
pub(super) fn fresh_lifecycle_tree(
    app: &App,
) -> Result<(tree::Tree, choices_store::Saved), ApiError> {
    let saved = choices_store::load(&app.root, true).map_err(choice_error)?;
    let mut snapshot = snapshot_value(app)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let mut discovery = app.tree_cache.lock().unwrap().discovery.clone();
    discovery.forget_catalog();
    discovery.augment(&mut snapshot, &saved.owner, discovery::now());
    let tree = tree::build(&snapshot, &saved.owner);
    let mut cache = app.tree_cache.lock().unwrap();
    cache.discovery = discovery;
    cache.snapshot = snapshot;
    cache.saved = Some(saved.clone());
    Ok((tree, saved))
}
pub(super) async fn workspace_live_targets(
    app: &App,
    workspace: &tree::Workspace,
) -> Result<Vec<(String, String)>, ApiError> {
    let sessions: Vec<_> = workspace
        .chats
        .iter()
        .chain(&workspace.unlisted)
        .flat_map(|c| c.sessions.iter().cloned())
        .collect();
    if sessions.is_empty() {
        return Ok(Vec::new());
    }
    let call = CliCall {
        args: vec!["agents".into(), "--json".into(), "--all".into()],
        cwd: app.root.clone(),
        session: None,
        adapter: "tmux-swarm".into(),
    };
    let batch = run_cli(app, call, None)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    let mut listing = batch.as_ref().and_then(Value::as_object).map(|entries| {
        entries
            .iter()
            .filter_map(|(id, v)| v["agents"].as_array().map(|a| (id.clone(), json!(a))))
            .collect::<serde_json::Map<_, _>>()
    });
    let mut singles = HashMap::new();
    for session in &sessions {
        let id = session["id"].as_str().unwrap_or_default();
        if listing.as_ref().is_some_and(|l| l.contains_key(id))
            || session["adapter"]
                .as_str()
                .is_none_or(|a| a.trim().is_empty())
            || (listing.is_some() && session["agents"].as_u64() == Some(0))
        {
            continue;
        }
        let call = action_session(app, id)?.args(&["agents", "--json"]);
        let result = run_cli(app, call, None)
            .await
            .map_err(|e| e.1.0.to_string())
            .and_then(|s| {
                serde_json::from_str::<Value>(&s)
                    .map_err(|e| e.to_string())
                    .and_then(|v| {
                        v["agents"]
                            .as_array()
                            .cloned()
                            .ok_or_else(|| "invalid agents".into())
                    })
            });
        singles.insert(id.to_owned(), result);
    }
    let listing = listing.take().map(Value::Object);
    lifecycle::live_targets(&sessions, listing.as_ref(), |session| {
        singles
            .remove(session["id"].as_str().unwrap_or_default())
            .unwrap_or_else(|| Err("missing listing".into()))
    })
    .map_err(lifecycle_error)
}
pub(super) async fn execute_lifecycle(
    app: &App,
    action: LifecycleAction,
) -> Result<Value, ApiError> {
    let cloned = app.clone();
    let (tree, saved) = tokio::task::spawn_blocking(move || fresh_lifecycle_tree(&cloned))
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "lifecycle tree task failed",
            )
        })??;
    let target = match &action {
        LifecycleAction::Impact(p) | LifecycleAction::Archive(p) | LifecycleAction::Delete(p) => {
            let (project, workspace) =
                lifecycle::workspace(&tree, &p.workspace).map_err(lifecycle_error)?;
            lifecycle::checked_path(
                &app.root,
                &p.workspace,
                !workspace.missing && !workspace.removed,
            )
            .map_err(lifecycle_error)?;
            if let Some(common) = lifecycle::common(project) {
                lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            }
            if matches!(action, LifecycleAction::Delete(_)) {
                if !lifecycle::can_delete(project, workspace) {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "This workspace cannot be deleted",
                    ));
                }
                let path = p.workspace.clone();
                let blocker =
                    tokio::task::spawn_blocking(move || lifecycle::removal_blocker(&path))
                        .await
                        .map_err(|_| {
                            error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "delete inspection failed",
                            )
                        })?
                        .map_err(lifecycle_error)?;
                if let Some(blocker) = blocker {
                    return Err(error(StatusCode::CONFLICT, &blocker));
                }
            }
            Some(workspace.clone())
        }
        _ => None,
    };
    let targets = if let Some(workspace) = &target {
        workspace_live_targets(app, workspace).await?
    } else {
        Vec::new()
    };
    if matches!(
        action,
        LifecycleAction::Archive(_) | LifecycleAction::Delete(_)
    ) {
        for (session, agent) in &targets {
            let (call, agent) = action_target(app, session, agent)?;
            run_cli(app, call.args(&["close", &agent]), None).await?;
        }
    }
    let setup = if let LifecycleAction::New(request) = &action {
        saved
            .owner
            .project_defaults
            .get(&request.project)
            .and_then(|s| s["setup_script"].as_str())
            .filter(|s| !s.trim().is_empty())
            .map(String::from)
    } else {
        None
    };
    let cloned = app.clone();
    let mut result = tokio::task::spawn_blocking(move || {
        apply_lifecycle(&cloned, action, &tree, &saved, targets.len())
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "lifecycle task failed"))??;
    if let Some(script) = setup {
        let path = result["workspace"]
            .as_str()
            .expect("created workspace")
            .to_owned();
        result["setup"] = run_setup(app, &path, &script, Duration::from_secs(600)).await;
    }
    Ok(result)
}

#[cfg(test)]
#[path = "lifecycle_routes/tests.rs"]
mod tests;
