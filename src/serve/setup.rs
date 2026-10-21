//! Server setup.

use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HudWrite {
    pub(super) on: bool,
}
/// ADR 0087: the switch shows the LaunchAgent's state, which only `swarm hud` reads and writes.
pub(super) async fn hud_read(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    hud_cli(&app, "status").await.map(Json)
}
pub(super) async fn hud_write(
    State(app): State<App>,
    body: Result<Json<HudWrite>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| error(e.status(), &e.body_text()))?;
    enqueue_action(&app, Action::Hud(body)).await.map(Json)
}
pub(super) async fn hud_cli(app: &App, verb: &str) -> Result<Value, ApiError> {
    let output = run_cli(
        app,
        CliCall {
            args: vec!["hud".into(), verb.into(), "--json".into()],
            cwd: app.root.clone(),
            session: None,
            adapter: String::new(),
        },
        None,
    )
    .await?;
    serde_json::from_str::<Value>(&output)
        .ok()
        .filter(|value| value["schema"] == swarm::hud::HUD_SCHEMA)
        .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "swarm returned invalid HUD status"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoticesWrite {
    pub(super) action: String,
}
/// ADR 0102: the notice health, which only `swarm notices` reads.
pub(super) async fn notices_read(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    // Above the 20 s worst case: the helper's 5 s and the log's 15 s.
    notices_cli(&app, &["notices", "health", "--json"], 30).await
}
/// Runs the owner's fix: the macOS prompt, a System Settings pane, or a test notice; then the
/// health.
pub(super) async fn notices_write(
    State(app): State<App>,
    body: Result<Json<NoticesWrite>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| error(e.status(), &e.body_text()))?;
    // One fix at a time, so two pages or a double click do not show two macOS prompts.
    let _running = NoticesFix::start()?;
    match body.action.as_str() {
        // Each limit is above the command's worst case: `allow` waits up to 120 s for the owner's
        // answer and `test` up to about 50 s, and each then reads the health for up to 20 s.
        "allow" => notices_cli(&app, &["notices", "allow", "--json"], 150).await,
        "test" => notices_cli(&app, &["notices", "test", "--json"], 75).await,
        pane @ ("notifications" | "focus") => {
            run_cli(&app, home_call(&app, &["notices", "settings", pane]), None).await?;
            notices_read(State(app)).await
        }
        _ => Err(error(StatusCode::BAD_REQUEST, "unknown notices action")),
    }
}
static NOTICES_FIX: AtomicBool = AtomicBool::new(false);

/// The running notice fix; dropping it, also on a cancelled request, frees the slot.
struct NoticesFix;

impl NoticesFix {
    fn start() -> Result<Self, ApiError> {
        if NOTICES_FIX.swap(true, Ordering::SeqCst) {
            return Err(error(
                StatusCode::CONFLICT,
                "a notices fix is already running",
            ));
        }
        Ok(Self)
    }
}

impl Drop for NoticesFix {
    fn drop(&mut self) {
        NOTICES_FIX.store(false, Ordering::SeqCst);
    }
}

async fn notices_cli(app: &App, args: &[&str], seconds: u64) -> Result<Json<Value>, ApiError> {
    let output = run_cli_with_timeout(
        app,
        home_call(app, args),
        None,
        Duration::from_secs(seconds),
    )
    .await?;
    serde_json::from_str::<Value>(&output)
        .ok()
        .filter(|value| value["state"].is_string())
        .map(Json)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_GATEWAY,
                "swarm returned invalid notice health",
            )
        })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SetupWrite {
    /// A comma list of `SETUP_ITEMS`, with the digest of its plan.
    pub(super) only: String,
    pub(super) digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SetupPlanQuery {
    pub(super) only: Option<String>,
}

pub(super) fn setup_groups(only: &str) -> bool {
    only.split(',')
        .all(|group| crate::SETUP_ITEMS.contains(&group))
}
/// A call from the swarm home, so a plan and its apply see the same folder.
pub(super) fn home_call(app: &App, args: &[&str]) -> CliCall {
    CliCall {
        args: args.iter().map(|arg| arg.to_string()).collect(),
        cwd: app.root.clone(),
        session: None,
        adapter: String::new(),
    }
}
pub(super) async fn doctor_read(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    // Above the notices health read's 20 s worst case, as notices_read.
    let output = run_cli_with_timeout(
        &app,
        home_call(&app, &["doctor", "--json"]),
        None,
        Duration::from_secs(30),
    )
    .await?;
    serde_json::from_str::<Value>(&output)
        .ok()
        .filter(|value| value["schema"] == crate::DOCTOR_SCHEMA)
        .map(Json)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_GATEWAY,
                "swarm returned an invalid doctor report",
            )
        })
}
pub(super) async fn setup_plan(
    State(app): State<App>,
    Query(query): Query<SetupPlanQuery>,
) -> Result<Json<Value>, ApiError> {
    let only = query.only.as_deref().unwrap_or("all");
    if only != "all" && !setup_groups(only) {
        return Err(error(StatusCode::BAD_REQUEST, "unknown setup item"));
    }
    let args = ["setup", only, "--plan", "--json"];
    let output = run_cli(&app, home_call(&app, &args), None).await?;
    serde_json::from_str::<Value>(&output)
        .ok()
        .filter(|value| value["digest"].is_string())
        .map(Json)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_GATEWAY,
                "swarm returned an invalid setup plan",
            )
        })
}
/// The owner's press on the `/setup` page is the consent that `swarm setup` asks for. A digest
/// that no longer matches the plan fails in the CLI, which reads as 409.
pub(super) async fn setup_write(
    State(app): State<App>,
    body: Result<Json<SetupWrite>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| error(e.status(), &e.body_text()))?;
    let valid = setup_groups(&body.only)
        && !body.digest.is_empty()
        && body.digest.chars().all(|c| c.is_ascii_hexdigit());
    if !valid {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "setup takes setup items with the digest of their plan",
        ));
    }
    enqueue_action(&app, Action::Setup(body)).await.map(Json)
}
pub(super) async fn run_setup_write(app: &App, body: SetupWrite) -> Result<Value, ApiError> {
    let args = [
        "setup",
        body.only.as_str(),
        "--digest",
        body.digest.as_str(),
    ];
    // The plugin item runs each agent CLI's own plugin install, which can take a while.
    let done = run_cli_with_timeout(app, home_call(app, &args), None, Duration::from_secs(180))
        .await
        .map(|output| json!({"ok": true, "output": output}));
    done.map_err(|(status, error)| {
        let stale = error.0["error"].as_str().is_some_and(|text| {
            // "after the plan", "after the link plan", and the parent's "after the plan".
            text.contains("changed after the") || text.contains(swarm::managed::CHANGED)
        });
        // run_cli reports every failure as 409. Only a file that changed under the plan keeps
        // it, as in owner_cli.
        if status == StatusCode::CONFLICT && !stale {
            (StatusCode::BAD_REQUEST, error)
        } else {
            (status, error)
        }
    })
}

#[cfg(test)]
#[path = "setup/tests.rs"]
mod tests;
