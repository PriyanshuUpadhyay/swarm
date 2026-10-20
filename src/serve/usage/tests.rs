//! Tests for server usage.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn usage_reads_cache_and_reserves_provider_refresh_even_on_failure() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$SWARM_HOME/actions"
if [ -f "$SWARM_HOME/fail" ] && [ "$2" = --refresh ]; then printf 'refresh refused' >&2; exit 2; fi
printf '{"schema":"swarm.usage.v1","meters":[]}\n'
"#,
    )
    .unwrap();
    assert_eq!(
        usage_read(State(fixture.app.clone())).await.unwrap().0["schema"],
        "swarm.usage.v1"
    );
    let request = |provider: &str| {
        usage_refresh(
            State(fixture.app.clone()),
            Ok(Json(UsageRefresh {
                provider: provider.into(),
            })),
        )
    };
    let (one, two) = tokio::join!(request("claude"), request("claude"));
    assert_eq!(one.unwrap().0["refresh"]["state"], "refreshed");
    assert_eq!(two.unwrap().0["refresh"]["state"], "cooldown");
    assert_eq!(
        action_calls(&fixture)
            .matches("usage --refresh --provider claude --json")
            .count(),
        1
    );
    std::fs::write(fixture.app.root.parent().unwrap().join("fail"), "fail").unwrap();
    let first = request("codex").await.unwrap_err();
    let second = request("codex").await.unwrap_err();
    assert_eq!(first.1.0, second.1.0);
    assert_eq!(
        action_calls(&fixture)
            .matches("usage --refresh --provider codex --json")
            .count(),
        1
    );
    std::fs::remove_file(fixture.app.root.parent().unwrap().join("fail")).unwrap();
    fixture
        .app
        .usage_refresh
        .lock()
        .unwrap()
        .get_mut("codex")
        .unwrap()
        .0 = tokio::time::Instant::now() - Duration::from_secs(300);
    assert_eq!(
        request("codex").await.unwrap().0["refresh"]["state"],
        "refreshed"
    );
    assert_eq!(request("agy").await.unwrap_err().0, StatusCode::BAD_REQUEST);
    assert!(
        Action::UsageRefresh(UsageRefresh {
            provider: "claude".into()
        })
        .lane()
        .is_none()
    );
}
