//! The new chat composer's preview card (ADR 0147).

use super::*;

/// `GET /api/chat/new-preview?cwd=&prompt=`: what a new chat in `cwd` with place `new` and this
/// prompt does, for the composer's card. It makes and saves nothing; like each lifecycle read,
/// it refreshes the server's tree cache, and the page reads it on each pause in typing. A plain folder is
/// `{place: "folder"}`; a git project names the new worktree's branch, folder, base, the last
/// origin fetch and the setup script's first line (ADR 0147).
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
        let Some(common) = lifecycle::common(project) else {
            return Ok(Json(json!({"place": "folder"})));
        };

        let (branch, folder) = prompt_worktree(&target, project, &cwd, &prompt);
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
            .and_then(|script| script.lines().map(str::trim).find(|line| !line.is_empty()))
            .map(str::to_owned);

        Ok(Json(json!({
            "place": "worktree",
            "branch": branch,
            "folder": folder,
            "base": base,
            "fetchedUnixS": fetched,
            "setup": setup,
        })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "preview task failed"))?
}

/// The branch and folder that a new worktree named from `prompt` gets: the first name whose
/// branch and folder are both free, with the folder create_workspace makes for it.
pub(super) fn prompt_worktree(
    target: &Target,
    project: &tree::Project,
    cwd: &str,
    prompt: &str,
) -> (Option<String>, Option<PathBuf>) {
    let (setting, _, _) = lifecycle::defaults(project, &target.saved.owner);
    let parent = lifecycle::worktree_folder(project, &setting).ok();
    let folder_of = |branch: &str| {
        let name = branch.trim_start_matches(swarm::task::PREFIX);
        parent
            .as_ref()
            .map(|parent| parent.join(lifecycle::slug(name)))
    };
    let branch = swarm::task::chat_preview(Path::new(cwd), prompt, |branch| {
        folder_of(branch).is_none_or(|folder| !folder.exists())
    });
    let folder = branch.as_deref().and_then(folder_of);
    (branch, folder)
}
