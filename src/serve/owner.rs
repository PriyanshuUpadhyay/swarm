//! Server owner.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MenuQuery {
    pub(super) provider: Option<String>,
}
pub(super) async fn menu_read(
    State(app): State<App>,
    uri: Uri,
    Query(query): Query<MenuQuery>,
) -> Result<Json<Value>, ApiError> {
    let command = uri.path().trim_start_matches("/api/");
    let mut args = vec![command.to_owned()];
    if matches!(command, "models" | "accounts") {
        let provider = query
            .provider
            .filter(|p| swarm::providers::Provider::parse(p).is_some())
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "known provider required"))?;
        args.extend(["--provider".into(), provider]);
    } else if query.provider.is_some() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "provider is not accepted here",
        ));
    }
    args.push("--json".into());
    let text = run_cli(
        &app,
        CliCall {
            args,
            cwd: app.root.clone(),
            session: None,
            adapter: String::new(),
        },
        None,
    )
    .await?;
    serde_json::from_str(&text)
        .map(Json)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid menu JSON from swarm"))
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ProfileOp {
    Save { profile: Value },
    New { name: String },
    Rename { name: String, to: String },
    Copy { name: String, to: String },
    Delete { name: String },
    Reset,
    MinUsage { pct: u8 },
}
#[derive(Deserialize)]
pub(super) struct ProfileWrite {
    pub(super) revision: String,
    #[serde(flatten)]
    pub(super) op: ProfileOp,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountLogin {
    pub(super) provider: String,
    pub(super) name: String,
    pub(super) revision: String,
    /// The login pane opens here when it is a listed workspace, else in the home folder (Swift).
    pub(super) workspace: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountReset {
    pub(super) revision: String,
}
/// Profile and account writes (S5). Each runs the `swarm roles` or `swarm accounts` owner with the
/// revision the page read.
#[derive(Deserialize)]
pub(super) enum OwnerWrite {
    Profile(ProfileWrite),
    Login(AccountLogin),
    ResetAccounts(AccountReset),
}
pub(super) async fn owner_write(
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
        "/api/roles" => "Profile",
        "/api/accounts/login" => "Login",
        _ => "ResetAccounts",
    };
    let write = serde_json::from_value(json!({ kind: body }))
        .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
    enqueue_action(&app, Action::Owner(write)).await.map(Json)
}
pub(super) async fn run_owner_write(app: &App, write: OwnerWrite) -> Result<Value, ApiError> {
    let flag = |revision: String| ["--revision".to_owned(), revision];
    let args: Vec<String> = match write {
        OwnerWrite::Profile(ProfileWrite { revision, op }) => {
            let mut args = vec!["roles".to_owned()];
            match op {
                ProfileOp::Save { profile } => {
                    args.push("save".into());
                    args.extend(flag(revision));
                    args.push(profile.to_string());
                    return owner_cli(app, args).await;
                }
                ProfileOp::New { name } => args.extend(["new".into(), name]),
                ProfileOp::Rename { name, to } => args.extend(["rename".into(), name, to]),
                ProfileOp::Copy { name, to } => args.extend(["copy".into(), name, to]),
                ProfileOp::Delete { name } => args.extend(["delete".into(), name]),
                ProfileOp::Reset => args.push("reset".into()),
                ProfileOp::MinUsage { pct } => {
                    args.extend(["set-min-usage".into(), pct.to_string()])
                }
            }
            args.extend(flag(revision));
            args
        }
        OwnerWrite::Login(body) => {
            if !["claude", "codex"].contains(&body.provider.as_str()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "Accounts sign in to claude or codex",
                ));
            }
            let listed = match body.workspace {
                Some(path) => {
                    let app = app.clone();
                    tokio::task::spawn_blocking(move || {
                        (loaded_tree(&app)
                            .is_ok_and(|(tree, _, _)| lifecycle::workspace(&tree, &path).is_ok())
                            && Path::new(&path).is_dir())
                        .then_some(path)
                    })
                    .await
                    .ok()
                    .flatten()
                }
                None => None,
            };
            let cwd = listed
                .or_else(|| std::env::var("HOME").ok())
                .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "HOME is not set"))?;
            let mut args = vec![
                "accounts".to_owned(),
                "login".into(),
                "--provider".into(),
                body.provider,
                "--name".into(),
                body.name,
            ];
            args.extend(flag(body.revision));
            args.extend(["--json".into(), "--cwd".into(), cwd]);
            args
        }
        OwnerWrite::ResetAccounts(body) => {
            let mut args = vec!["accounts".to_owned(), "reset".into()];
            args.extend(flag(body.revision));
            args.push("--json".into());
            args
        }
    };
    owner_cli(app, args).await
}
pub(super) async fn owner_cli(app: &App, args: Vec<String>) -> Result<Value, ApiError> {
    // The Swift app's adapter, so the login pane opens where its chats do; a credential never
    // passes through this server.
    let call = CliCall {
        args,
        cwd: app.root.clone(),
        session: None,
        adapter: "tmux-swarm".into(),
    };
    let output = run_cli(app, call, None).await.map_err(|(status, body)| {
        let stale = body.0["error"]
            .as_str()
            .is_some_and(|text| text.contains("changed on disk"));
        // run_cli reports every refusal as 409. Only a stale revision keeps it, so the page
        // reloads on a conflict and shows any other refusal as it is.
        if status == StatusCode::CONFLICT && !stale {
            (StatusCode::BAD_REQUEST, body)
        } else {
            (status, body)
        }
    })?;
    serde_json::from_str(&output)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid JSON from swarm"))
}

#[cfg(test)]
#[path = "owner/tests.rs"]
mod tests;
