//! Tests for server cli process.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn action_timeout_kills_the_process_group_without_retry() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
printf 'start\n' >> "$SWARM_HOME/actions"
sleep 30 &
printf '%s\n' "$!" > "$SWARM_HOME/sleeper"
wait
"#,
    )
    .unwrap();
    let call = action_session(&fixture.app, &fixture.session)
        .unwrap()
        .args(&["interrupt", "worker"]);
    // The limit must outlast the stand-in's start on a loaded Mac, so its sleeper exists.
    let result = run_cli_with_timeout(&fixture.app, call, None, Duration::from_secs(5)).await;
    assert_eq!(result.unwrap_err().0, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(action_calls(&fixture), "start\n");
    let pid = std::fs::read_to_string(fixture.app.root.parent().unwrap().join("sleeper")).unwrap();
    until_dead(pid.trim()).await;
}
