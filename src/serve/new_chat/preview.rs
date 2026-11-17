//! The new chat composer's preview card (ADR 0147).

use super::*;

/// `GET /api/chat/new-preview?cwd=&prompt=`: what a new chat in `cwd` with place `new` and this
/// prompt does, for the composer's card. It writes nothing. A chat that starts in `cwd` itself
/// is `{place: "folder"}`; one that gets its own worktree names its branch, folder, base, the
/// last origin fetch and the setup script's first line (ADR 0147).
pub(in crate::serve) async fn new_chat_preview(
    State(app): State<App>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let cwd = query.get("cwd").cloned().unwrap_or_default();
    let prompt = query.get("prompt").cloned().unwrap_or_default();
    if cwd.contains('\0') || !Path::new(&cwd).is_absolute() {
        return Err(error(StatusCode::BAD_REQUEST, "absolute cwd required"));
    }
    tokio::task::spawn_blocking(move || {
        let target = chat_target(&app, &cwd)?;
        let project = lifecycle::project(&target.tree, &target.project).map_err(lifecycle_error)?;
        let Some(common) = lifecycle::common(project).filter(|_| target.hub_main) else {
            return Ok(Json(json!({"place": "folder"})));
        };
        let branch = swarm::task::chat_preview(Path::new(&cwd), &prompt);
        let name = branch
            .as_deref()
            .map(|branch| branch.trim_start_matches(swarm::task::PREFIX).to_owned());
        let (folder, _, _) = lifecycle::defaults(project, &target.saved.owner);
        let base = lifecycle::references(common)
            .map_err(lifecycle_error)?
            .bases
            .into_iter()
            .next();
        let fetched = std::fs::metadata(Path::new(common).join("FETCH_HEAD"))
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|age| age.as_secs());
        let setup = target
            .saved
            .owner
            .project_defaults
            .get(&target.project)
            .and_then(|s| s["setup_script"].as_str())
            .and_then(|script| script.lines().map(str::trim).find(|line| !line.is_empty()))
            .map(str::to_owned);
        Ok(Json(json!({
            "place": "worktree",
            "branch": branch,
            "folder": name.map(|name| format!("{}/{}", folder.trim_end_matches('/'), lifecycle::slug(&name))),
            "base": base,
            "fetchedUnixS": fetched,
            "setup": setup,
        })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "preview task failed"))?
}
