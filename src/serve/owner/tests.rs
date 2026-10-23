//! Tests for server owner.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn profile_and_account_writes_run_their_owner_and_only_a_stale_revision_is_409() {
    let fixture = action_fixture();
    std::fs::write(
        &fixture.app.cli,
        r#"#!/bin/sh
printf '%s|%s|%s\n' "$SWARM_ADAPTER" "$PWD" "$*" >> "$SWARM_HOME/actions"
case " $* " in
  *" --revision stale "*) printf 'swarm: profiles changed on disk; reload and try again' >&2; exit 1 ;;
  *refused*) printf 'swarm: Invalid account name' >&2; exit 1 ;;
esac
printf '{"revision":"next"}\n'
"#,
    )
    .unwrap();
    let write = |route: &'static str, body: Value| {
        owner_write(
            State(fixture.app.clone()),
            Uri::from_static(route),
            Ok(Json(body)),
        )
    };
    let last_call = || action_calls(&fixture).lines().last().unwrap().to_owned();
    let root = fixture.app.root.to_str().unwrap().to_owned();
    for (body, args) in [
        (
            json!({"revision":"r1","op":"save","profile":{"name":"chat","runners":[]}}),
            r#"roles save --revision r1 {"name":"chat","runners":[]}"#,
        ),
        (
            json!({"revision":"r1","op":"new","name":"review"}),
            "roles new review --revision r1",
        ),
        (
            json!({"revision":"r1","op":"rename","name":"review","to":"check"}),
            "roles rename review check --revision r1",
        ),
        (
            json!({"revision":"r1","op":"copy","name":"check","to":"check2"}),
            "roles copy check check2 --revision r1",
        ),
        (
            json!({"revision":"r1","op":"delete","name":"check2"}),
            "roles delete check2 --revision r1",
        ),
        (
            json!({"revision":"r1","op":"reset"}),
            "roles reset --revision r1",
        ),
        (
            json!({"revision":"r1","op":"min_usage","pct":7}),
            "roles set-min-usage 7 --revision r1",
        ),
    ] {
        assert_eq!(
            write("/api/roles", body).await.unwrap().0["revision"],
            "next"
        );
        assert_eq!(last_call(), format!("tmux-swarm|{root}|{args}"));
    }
    let home = std::env::var("HOME").unwrap();
    let repo = env!("CARGO_MANIFEST_DIR");
    for (workspace, cwd) in [
        (json!(repo), repo),
        (json!("/not/listed"), &home),
        (Value::Null, &home),
    ] {
        let body = json!({"provider":"claude","name":"work","revision":"r2","workspace":workspace});
        assert_eq!(
            write("/api/accounts/login", body).await.unwrap().0["revision"],
            "next"
        );
        assert_eq!(
            last_call(),
            format!(
                "tmux-swarm|{root}|accounts login --provider claude --name work --revision r2 --json --cwd {cwd}"
            )
        );
    }
    assert_eq!(
        write("/api/accounts/reset", json!({"revision":"r3"}))
            .await
            .unwrap()
            .0["revision"],
        "next"
    );
    assert!(last_call().ends_with("|accounts reset --revision r3 --json"));
    let status = |result: Result<Json<Value>, ApiError>| result.unwrap_err().0;
    assert_eq!(
        status(write("/api/roles", json!({"revision":"stale","op":"reset"})).await),
        StatusCode::CONFLICT
    );
    let refused = json!({"provider":"codex","name":"refused","revision":"r2"});
    assert_eq!(
        status(write("/api/accounts/login", refused).await),
        StatusCode::BAD_REQUEST
    );
    let calls = action_calls(&fixture).lines().count();
    for (route, body) in [
        (
            "/api/accounts/login",
            json!({"provider":"agy","name":"a","revision":"r"}),
        ),
        (
            "/api/roles",
            json!({"revision":"r","op":"min_usage","pct":300}),
        ),
        ("/api/accounts/reset", json!({})),
    ] {
        assert_eq!(status(write(route, body).await), StatusCode::BAD_REQUEST);
    }
    assert_eq!(action_calls(&fixture).lines().count(), calls);
}
