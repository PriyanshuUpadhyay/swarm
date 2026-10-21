//! Tests for server setup.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn hud_routes_run_swarm_hud_and_refuse_a_bad_reply() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$SWARM_HOME/actions"
if [ -f "$SWARM_HOME/bad" ]; then printf '{}\n'; exit 0; fi
printf '{"schema":"swarm.hud.v1","installed":true,"running":true,"app":"/a/SwarmHUD.app"}\n'
"#,
    )
    .unwrap();
    assert_eq!(
        hud_read(State(fixture.app.clone())).await.unwrap().0["installed"],
        true
    );
    for on in [true, false] {
        let reply = hud_write(State(fixture.app.clone()), Ok(Json(HudWrite { on })))
            .await
            .unwrap();
        assert_eq!(reply.0["schema"], "swarm.hud.v1");
    }
    let calls = action_calls(&fixture);
    for call in [
        "hud status --json",
        "setup hud --yes",
        "hud uninstall --json",
    ] {
        assert!(calls.contains(call), "{calls}");
    }
    std::fs::write(fixture.app.root.parent().unwrap().join("bad"), "").unwrap();
    assert_eq!(
        hud_read(State(fixture.app.clone())).await.unwrap_err().0,
        StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn setup_routes_run_the_cli_from_the_home_and_refuse_bad_input_first() {
    let fixture = action_fixture();
    let doctor = doctor_read(State(fixture.app.clone())).await.unwrap().0;
    assert_eq!(doctor["schema"], "swarm.doctor.v1");
    let plan = |only: Option<&str>| {
        setup_plan(
            State(fixture.app.clone()),
            Query(SetupPlanQuery {
                only: only.map(str::to_owned),
            }),
        )
    };
    assert_eq!(
        plan(Some("hooks,plugin")).await.unwrap().0["digest"],
        "ab12"
    );
    assert_eq!(plan(None).await.unwrap().0["digest"], "ab12");
    for bad in ["hooks;rm", "skills"] {
        assert_eq!(
            plan(Some(bad)).await.unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }
    let write = |body: Value| {
        setup_write(
            State(fixture.app.clone()),
            Ok(Json(serde_json::from_value(body).unwrap())),
        )
    };
    for body in [
        json!({"only": "hooks", "digest": ""}),
        json!({"only": "hooks", "digest": "ab 12"}),
        json!({"only": "skills", "digest": "ab12"}),
        json!({"only": "all", "digest": "ab12"}),
    ] {
        assert_eq!(write(body).await.unwrap_err().0, StatusCode::BAD_REQUEST);
    }
    for body in [
        json!({"only": "hooks,plugin", "digest": "ab12"}),
        json!({"only": "hud", "digest": "ab12"}),
    ] {
        assert_eq!(write(body).await.unwrap().0["ok"], true);
    }
    let calls = action_calls(&fixture);
    let root = fixture.app.root.to_string_lossy();
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        [
            format!("|||{root}|doctor --json"),
            format!("|||{root}|setup hooks,plugin --plan --json"),
            format!("|||{root}|setup all --plan --json"),
            format!("|||{root}|setup hooks,plugin --digest ab12"),
            format!("|||{root}|setup hud --digest ab12"),
        ]
    );
    // Only a file that changed under the plan is a conflict; any other failed setup is the request's own failure.
    let stale = write(json!({"only": "hooks", "digest": "dead"}))
        .await
        .unwrap_err();
    assert_eq!(stale.0, StatusCode::CONFLICT);
    let failed = write(json!({"only": "hooks", "digest": "beef"}))
        .await
        .unwrap_err();
    assert_eq!(failed.0, StatusCode::BAD_REQUEST);
    assert!(
        failed.1.0["error"]
            .as_str()
            .unwrap()
            .contains("cannot write")
    );
    let raced = write(json!({"only": "hooks", "digest": "cafe"}))
        .await
        .unwrap_err();
    assert_eq!(raced.0, StatusCode::CONFLICT);
    let link = write(json!({"only": "plugin", "digest": "f00d"}))
        .await
        .unwrap_err();
    assert_eq!(link.0, StatusCode::CONFLICT);
}

#[test]
fn one_notices_fix_runs_at_a_time() {
    let first = NoticesFix::start().unwrap();
    assert!(NoticesFix::start().is_err());
    drop(first);
    assert!(NoticesFix::start().is_ok());
}
