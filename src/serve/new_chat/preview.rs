//! The new chat composer's preview card (ADR 0149).

use super::*;

/// `GET /api/chat/new-preview?cwd=`: what a new chat in `cwd` with place `new` does, for the
/// composer's card. It makes and saves nothing; like each lifecycle read, it refreshes the
/// server's tree cache, and the page reads it when the composer's project changes. A plain folder is
/// `{place: "folder"}`; a git project names the new worktree's base, the last origin fetch and
/// the setup script's first line (ADR 0149). The branch is named only at the start, because the
/// model that names it takes seconds (ADR 0158).
pub(in crate::serve) async fn new_chat_preview(
    State(app): State<App>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let cwd = query.get("cwd").cloned().unwrap_or_default();
    if cwd.contains('\0') || !Path::new(&cwd).is_absolute() {
        return Err(error(StatusCode::BAD_REQUEST, "absolute cwd required"));
    }

    tokio::task::spawn_blocking(move || {
        let target = chat_target(&app, &cwd)?;
        let project = target.tree_project()?;
        let Some(common) = lifecycle::common(project) else {
            return Ok(Json(json!({"place": "folder"})));
        };

        // The same base as create_workspace, so the card matches what starts.
        let base = lifecycle::references(common)
            .map_err(lifecycle_error)?
            .bases
            .into_iter()
            .next();

        let fetched = std::fs::metadata(Path::new(common).join("FETCH_HEAD"))
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since_epoch| since_epoch.as_secs());
        let setup = lifecycle::setup_script(&target.saved.owner, &target.project)
            .and_then(first_line)
            .map(str::to_owned);

        Ok(Json(json!({
            "place": "worktree",
            "base": base,
            "fetchedUnixS": fetched,
            "setup": setup,
        })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "preview task failed"))?
}
