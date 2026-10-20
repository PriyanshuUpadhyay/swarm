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
    let result = run_cli_with_timeout(&fixture.app, call, None, Duration::from_secs(1)).await;
    assert_eq!(result.unwrap_err().0, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(action_calls(&fixture), "start\n");
    let pid = std::fs::read_to_string(fixture.app.root.parent().unwrap().join("sleeper")).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        !std::process::Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    );
}
