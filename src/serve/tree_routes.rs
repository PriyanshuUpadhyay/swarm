//! Server tree routes.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TreeQuery {
    pub(super) view: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TabsQuery {
    pub(super) workspace: String,
}

pub(super) fn choice_error(err: choices_store::StoreError) -> ApiError {
    let status = match err {
        choices_store::StoreError::Stale => StatusCode::CONFLICT,
        choices_store::StoreError::Busy => StatusCode::SERVICE_UNAVAILABLE,
        choices_store::StoreError::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error(status, &err.to_string())
}

pub(super) fn loaded_tree(
    app: &Server,
) -> Result<
    (
        tree::Tree,
        choices_store::Saved,
        BTreeMap<String, fields::WorkspaceFields>,
    ),
    ApiError,
> {
    let loaded = choices_store::load(&app.root, false);
    let mut cache = app.tree_cache.lock().unwrap();
    let mut saved = match loaded {
        Ok(saved) => {
            cache.choices_error = None;
            cache.saved = Some(saved.clone());
            saved
        }
        Err(err) => {
            cache.choices_error = Some(err.to_string());
            cache.saved.clone().unwrap_or_else(|| choices_store::Saved {
                owner: choices::OwnerChoices::default(),
                state: choices::ViewState::default(),
                revision: String::new(),
            })
        }
    };
    let snapshot = if cache.snapshot.is_object() {
        cache.snapshot.clone()
    } else {
        snapshot_value(app).map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    };
    let tree = tree::build(&snapshot, &saved.owner);
    saved.state.record_first_sight(&tree);
    Ok((tree, saved, cache.discovery.fields.clone()))
}

pub(super) async fn tree_view(
    State(app): State<App>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Value>, ApiError> {
    let archive = match query.view.as_deref().unwrap_or("workspaces") {
        "workspaces" => false,
        "archived" => true,
        _ => return Err(error(StatusCode::BAD_REQUEST, "unknown tree view")),
    };
    tokio::task::spawn_blocking(move || {
        let (tree, saved, cache) = loaded_tree(&app)?;
        let sections = tree::sections(
            &tree,
            &saved.owner,
            &saved.state,
            archive,
            discovery::now(),
            &cache,
        );
        // Search and Recent use every chat even when the sidebar is folded or capped.
        let mut catalog_state = saved.state.clone();
        catalog_state.collapsed.clear();
        catalog_state.expanded_lists = tree
            .projects
            .iter()
            .flat_map(|p| &p.workspaces)
            .map(|w| w.path.clone())
            .collect();
        let chats: Vec<_> = tree::sections(
            &tree,
            &saved.owner,
            &catalog_state,
            false,
            discovery::now(),
            &cache,
        )
        .into_iter()
        .flat_map(|s| s.rows)
        .filter(|r| r.kind == "chat")
        .collect();
        let rows: Vec<_> = sections
            .iter()
            .filter(|s| s.expanded)
            .flat_map(|s| s.rows.clone())
            .collect();
        let selected = saved
            .state
            .selected_workspace
            .as_deref()
            .and_then(|w| navigation::selected_chat(&tree, &saved.owner, &saved.state, w));
        let selected_row = navigation::selected_row(
            &rows,
            saved.state.selected_workspace.as_deref(),
            selected,
            saved.state.selected_child.as_deref(),
        );
        let (version, agents) = {
            let hub = app.hub.lock().unwrap();
            (hub.version, hub.agents.clone())
        };
        let archived_status = tree::AgentStatus::aggregate(
            tree.projects
                .iter()
                .flat_map(|p| &p.workspaces)
                .filter(|w| saved.owner.archived.contains(&w.path))
                .flat_map(|w| w.chats.iter().flat_map(|c| &c.sessions))
                .flat_map(|s| {
                    agents[s["id"].as_str().unwrap_or_default()]
                        .as_array()
                        .into_iter()
                        .flatten()
                })
                .map(tree::AgentStatus::agent)
                .filter(|s| *s == tree::AgentStatus::Waiting),
        );
        let choices_error = app.tree_cache.lock().unwrap().choices_error.clone();
        Ok(Json(json!({
            "version": version,
            "revision": saved.revision,
            "view": if archive { "archived" } else { "workspaces" },
            "sections": sections,
            "chats": chats,
            "selectedRow": selected_row,
            "archivedStatus": archived_status,
            "choicesError": choices_error,
            "prefs": saved.owner.prefs,
        })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "tree task failed"))?
}

/// The chats that run on another adapter and have something to show, newest first (ADR 0085).
/// The `import_chat` choice lists one.
pub(super) async fn importable_chats(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (tree, _, _) = loaded_tree(&app)?;
        let mut chats: Vec<_> = tree.importable.iter().collect();
        chats.sort_by_key(|(_, c)| std::cmp::Reverse(c.last_activity));
        let chats: Vec<_> = chats
            .into_iter()
            .map(|(workspace, c)| {
                json!({
                    "id": c.id,
                    "workspace": workspace,
                    "title": c.title,
                    "provider": c.provider,
                    "lastActivity": c.last_activity,
                    "running": c.is_running == Some(true),
                })
            })
            .collect();
        Ok(Json(json!({ "chats": chats })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "importable task failed"))?
}

/// The Dock badge of the shell (ADR 0077): the chats that wait, outside archived workspaces, or 0
/// when the owner turned the badge off.
pub(super) async fn badge_view(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (tree, saved, _) = loaded_tree(&app)?;
        let count = if saved.owner.prefs["notices"]["badge"] == false {
            0
        } else {
            tree.projects
                .iter()
                .flat_map(|p| &p.workspaces)
                .filter(|w| !saved.owner.archived.contains(&w.path))
                .flat_map(|w| &w.chats)
                .filter(|c| c.status == Some(tree::AgentStatus::Waiting))
                .count()
        };
        Ok(Json(json!({ "count": count })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "badge task failed"))?
}

pub(super) async fn tabs_view(
    State(app): State<App>,
    Query(query): Query<TabsQuery>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (tree, saved, cache) = loaded_tree(&app)?;
        let view = tabs::view(
            &tree,
            &saved.owner,
            &saved.state,
            &query.workspace,
            discovery::now(),
            &cache,
        )
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown workspace"))?;
        Ok(Json(json!({
            "version": app.hub.lock().unwrap().version,
            "revision": saved.revision,
            "view": view,
        })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "tabs task failed"))?
}

pub(super) async fn choice_write(
    State(app): State<App>,
    body: Result<Json<choices::ChoiceWrite>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    enqueue_action(&app, Action::Choices(body)).await.map(Json)
}

#[cfg(test)]
#[path = "tree_routes/tests.rs"]
mod tests;
