//! The new chat composer's `/` and `@` menus (ADR 0155).

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::serve) struct NewCommandsQuery {
    cwd: String,
    provider: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::serve) struct NewFilesQuery {
    cwd: String,
}

/// Checks that a new chat may start in the folder a menu will read.
async fn menu_folder(app: &App, cwd: String) -> Result<String, ApiError> {
    if cwd.contains('\0') || !Path::new(&cwd).is_absolute() {
        return Err(error(StatusCode::BAD_REQUEST, "absolute cwd required"));
    }

    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        let target = chat_target(&app, &cwd, false)?;
        if target.worktree_only.is_some() {
            return Err(error(
                StatusCode::CONFLICT,
                "Files and commands are not listed for an archived workspace.",
            ));
        }
        Ok(cwd)
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "cwd check failed"))?
}

/// `GET /api/chat/new-commands?cwd=&provider=`: the `/` menu rows that `provider` offers in
/// `cwd`, before a chat exists. The rows and the cache are those of `/api/chat/commands`. No
/// account is picked yet, so the CLI lists with its default one.
pub(in crate::serve) async fn new_chat_commands(
    State(app): State<App>,
    Query(query): Query<NewCommandsQuery>,
) -> Result<Json<Value>, ApiError> {
    if swarm::providers::Provider::parse(&query.provider).is_none() {
        return Err(error(StatusCode::BAD_REQUEST, "known provider required"));
    }
    let cwd = menu_folder(&app, query.cwd).await?;
    let call = CliCall {
        args: ["commands", "--provider", &query.provider, "--json"]
            .map(String::from)
            .to_vec(),
        cwd: PathBuf::from(&cwd),
        session: None,
        adapter: String::new(),
    };
    Ok(Json(
        commands::cached_commands(&app, (cwd, query.provider), call).await,
    ))
}

/// `GET /api/chat/new-files?cwd=`: the `@` menu's paths for `cwd`, as `/api/chat/files` gives
/// them for a chat's folder.
pub(in crate::serve) async fn new_chat_files(
    State(app): State<App>,
    Query(query): Query<NewFilesQuery>,
) -> Result<Json<Value>, ApiError> {
    let cwd = menu_folder(&app, query.cwd).await?;
    tokio::task::spawn_blocking(move || Json(commands::folder_files(&cwd)))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "file list task failed"))
}
