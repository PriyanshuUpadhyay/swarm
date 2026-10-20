//! Server usage.

use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UsageRefresh {
    pub(super) provider: String,
}
pub(super) async fn usage_read(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    read_usage(&app, None).await.map(Json)
}
pub(super) async fn usage_refresh(
    State(app): State<App>,
    body: Result<Json<UsageRefresh>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| error(e.status(), &e.body_text()))?;
    if !["claude", "codex"].contains(&body.provider.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Usage refresh supports claude or codex",
        ));
    }
    enqueue_action(&app, Action::UsageRefresh(body))
        .await
        .map(Json)
}
pub(super) async fn read_usage(app: &App, refresh: Option<&str>) -> Result<Value, ApiError> {
    let mut args = vec!["usage", "--json"];
    if let Some(provider) = refresh {
        args = vec!["usage", "--refresh", "--provider", provider, "--json"];
    }
    let output = run_cli(
        app,
        CliCall {
            args: args.into_iter().map(String::from).collect(),
            cwd: app.root.clone(),
            session: None,
            adapter: String::new(),
        },
        None,
    )
    .await?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "swarm returned invalid usage JSON"))?;
    if !value.is_object() {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "swarm returned invalid usage data",
        ));
    }
    Ok(value)
}
pub(super) async fn refresh_usage(app: &App, provider: &str) -> Result<Value, ApiError> {
    let (remaining, failure) = {
        let mut refresh = app.usage_refresh.lock().unwrap();
        let now = tokio::time::Instant::now();
        let remaining = refresh
            .get(provider)
            .map(|(last, _)| Duration::from_secs(300).saturating_sub(now.duration_since(*last)))
            .unwrap_or_default();
        if remaining.is_zero() {
            refresh.insert(provider.into(), (now, None));
        }
        (
            remaining,
            refresh
                .get(provider)
                .and_then(|(_, failure)| failure.clone()),
        )
    };
    if let Some(failure) = failure {
        return Err(failure);
    }
    let reading = read_usage(app, remaining.is_zero().then_some(provider)).await;
    if remaining.is_zero()
        && let Err(err) = &reading
    {
        app.usage_refresh
            .lock()
            .unwrap()
            .get_mut(provider)
            .unwrap()
            .1 = Some(err.clone());
    }
    let mut value = reading?;
    value["refresh"] = json!({"provider":provider,"state":if remaining.is_zero() {"refreshed"} else {"cooldown"},
        "next_in_seconds":if remaining.is_zero() {300} else {remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)}});
    Ok(value)
}

#[cfg(test)]
#[path = "usage/tests.rs"]
mod tests;
