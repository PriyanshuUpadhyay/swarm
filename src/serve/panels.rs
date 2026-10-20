//! Server panels.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectPath {
    pub(super) project: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkspacePath {
    pub(super) workspace: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PanelQuery {
    pub(super) workspace: String,
    #[serde(default)]
    pub(super) mode: changes::Mode,
    pub(super) base: Option<String>,
    pub(super) path: Option<String>,
    pub(super) layer: Option<String>,
}
pub(super) async fn panel_read(
    State(app): State<App>,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (tree, _) = fresh_lifecycle_tree(&app)?;
        let value = serde_json::to_value(query).unwrap();
        if uri.path() == "/api/issues" {
            let query: ProjectPath = serde_json::from_value(value)
                .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
            let project = lifecycle::project(&tree, &query.project).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &query.project, true).map_err(lifecycle_error)?;
            let common = lifecycle::common(project)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Issues need a Git project"))?;
            lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            return workspace_panel::issues(common)
                .map(Json)
                .map_err(lifecycle_error);
        }
        let query: PanelQuery = serde_json::from_value(value)
            .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
        lifecycle::workspace(&tree, &query.workspace).map_err(lifecycle_error)?;
        lifecycle::checked_path(&app.root, &query.workspace, true).map_err(lifecycle_error)?;
        let path = || {
            query
                .path
                .as_deref()
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Select a file path"))
        };
        let result = match uri.path() {
            "/api/changes" => changes::list(&query.workspace, &query.mode, query.base.as_deref()),
            "/api/changes/diff" => changes::patch(
                &query.workspace,
                path()?,
                &query.mode,
                query.base.as_deref(),
                query.layer.as_deref(),
            ),
            "/api/changes/file" => changes::file(&query.workspace, path()?),
            _ => workspace_panel::pr(&query.workspace),
        };
        result.map(Json).map_err(lifecycle_error)
    })
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "workspace read task failed",
        )
    })?
}

#[cfg(test)]
#[path = "panels/tests.rs"]
mod tests;
