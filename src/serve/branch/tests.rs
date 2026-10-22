use super::*;

fn lines(text: &str) -> Vec<Value> {
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn a_claude_cut_moves_every_record_to_the_new_session() {
    let prefix = concat!(
        r#"{"type":"user","sessionId":"old","uuid":"u1","parentUuid":null,"message":{"role":"user","content":"hi"}}"#,
        "\n",
        r#"{"type":"assistant","sessionId":"older","uuid":"a1","parentUuid":"u1","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
        "\n",
        r#"{"type":"user","sessionId":"older","uuid":"r1","parentUuid":"a1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#,
        "\n",
        r#"{"type":"system","subtype":"compact_boundary","sessionId":"old","uuid":"c1","parentUuid":null}"#,
        "\n",
        r#"{"type":"file-history-snapshot","messageId":"m"}"#,
        "\n",
    );
    let cut = lines(&cut_claude(prefix, "new").unwrap());
    assert_eq!(cut.len(), 5);
    assert!(cut[..4].iter().all(|record| record["sessionId"] == "new"));
    assert_eq!(cut[1]["uuid"], "a1");
    assert_eq!(
        cut[4],
        json!({"type":"file-history-snapshot","messageId":"m"})
    );
}

#[test]
fn a_claude_cut_inside_a_tool_call_is_refused() {
    let prefix = concat!(
        r#"{"type":"assistant","sessionId":"old","uuid":"a1","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
        "\n",
    );
    assert!(cut_claude(prefix, "new").is_err());
    assert!(cut_claude("", "new").is_err());
}

#[test]
fn a_pi_cut_keeps_the_path_to_the_message_and_links_its_parent_file() {
    let text = concat!(
        r#"{"type":"session","version":3,"id":"old","timestamp":"t0","cwd":"/w"}"#,
        "\n",
        r#"{"type":"model_change","id":"m1","parentId":null}"#,
        "\n",
        r#"{"type":"message","id":"u1","parentId":"m1","message":{"role":"user","content":"one"}}"#,
        "\n",
        r#"{"type":"message","id":"a1","parentId":"u1","message":{"role":"assistant","content":[]}}"#,
        "\n",
        // A turn the owner left with /tree: not on the path to the cut.
        r#"{"type":"message","id":"x1","parentId":"u1","message":{"role":"assistant","content":[]}}"#,
        "\n",
        r#"{"type":"label","id":"l1","parentId":"a1","targetId":"u1","label":"start"}"#,
        "\n",
        r#"{"type":"compaction","id":"k1","parentId":"l1","firstKeptEntryId":"l1","summary":"s"}"#,
        "\n",
    );
    let user =
        r#"{"type":"message","id":"u2","parentId":"k1","message":{"role":"user","content":"two"}}"#;
    let file = format!("{text}{user}\n");
    let cut = lines(&cut_pi(&file, text.len(), "S-orchestrator", "/logs/old.jsonl").unwrap());
    assert_eq!(cut[0]["type"], "session");
    assert_eq!(cut[0]["id"], "S-orchestrator");
    assert_eq!(cut[0]["cwd"], "/w");
    assert_eq!(cut[0]["parentSession"], "/logs/old.jsonl");
    let path: Vec<_> = cut[1..]
        .iter()
        .map(|e| (e["id"].clone(), e["parentId"].clone()))
        .collect();
    assert_eq!(
        path,
        [
            (json!("m1"), json!(null)),
            (json!("u1"), json!("m1")),
            (json!("a1"), json!("u1")),
            (json!("k1"), json!("a1")),
        ]
    );
    // The label went, so the compaction keeps from the entry that took its place.
    assert_eq!(cut[4]["firstKeptEntryId"], "k1");
    assert!(cut_pi(&file, 0, "S-orchestrator", "/logs/old.jsonl").is_err());
}

mod route {
    use super::*;
    use crate::serve::test_support::*;

    async fn branch(fixture: &Fixture, session: &str, row: &str) -> Result<Value, ApiError> {
        let body = serde_json::from_value(json!({"session": session, "row": row})).unwrap();
        branch_chat(State(fixture.app.clone()), Ok(Json(body)))
            .await
            .map(|Json(value)| value)
    }

    #[tokio::test]
    async fn a_branch_cuts_the_chair_log_and_resumes_it_in_a_new_chat() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = action_fixture();
        let cli = &fixture.app.cli;
        std::fs::write(
            cli,
            r#"#!/bin/sh
printf '%s\n' "${SWARM_SESSION_ID-}|$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
  'agents --json') printf '%s\n' '{"agents":[{"id":"orchestrator","model":"opus","account":"work"}]}' ;;
  'session new') printf '%s\n' 01a1650a-0000-7000-8000-000000000001 ;;
  'launch orchestrator') printf 'fixture-pane\n' ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut log = std::fs::OpenOptions::new()
            .append(true)
            .open(&fixture.log)
            .unwrap();
        std::io::Write::write_all(
            &mut log,
            br#"{"type":"user","sessionId":"synthetic","uuid":"u9","parentUuid":null,"timestamp":"2026-10-22T10:00:00Z","message":{"role":"user","content":"Now tidy the tests"}}
"#,
        )
        .unwrap();
        let rows = handoff_rows(&fixture.app, &fixture.session).await.unwrap();
        let users: Vec<_> = rows
            .iter()
            .filter(|r| r.kind == view::RowKind::User)
            .collect();
        let (first, second) = (users[0], users[1]);
        // The page sends the id the rows stream gives, which ends with `@<anchor>`.
        let page_id = |row: &view::Row| format!("{}@{}", row.id, row.anchor_source_id);
        let new = "01a1650a-0000-7000-8000-000000000001";

        let response = branch(&fixture, &fixture.session, &page_id(second))
            .await
            .unwrap();
        assert_eq!(response["session"], new);
        assert_eq!(response["text"], second.text.as_str());
        let calls = action_calls(&fixture);
        let calls: Vec<_> = calls.lines().collect();
        let chair = calls[2].rsplit_once(' ').unwrap().1;
        assert_eq!(calls[0], format!("{}|agents --json", fixture.session));
        assert_eq!(calls[1], "|session new lane");
        assert_eq!(
            calls[2],
            format!("{new}|session branch {new} {} {chair}", fixture.session)
        );
        assert_eq!(
            calls[3],
            format!("{new}|launch orchestrator chat --resume --model opus --account work")
        );
        let id = chair.strip_prefix("claude:").unwrap();
        let cut =
            std::fs::read_to_string(fixture.log.with_file_name(format!("{id}.jsonl"))).unwrap();
        let log = std::fs::read_to_string(&fixture.log).unwrap();
        let offset = line_offset(&second.anchor_source_id).unwrap();
        assert_eq!(cut.lines().count(), log[..offset].lines().count());
        assert!(
            lines(&cut)
                .iter()
                .all(|r| r.get("sessionId").is_none_or(|s| s == id))
        );

        // The first message has nothing before it, so its branch starts new.
        std::fs::remove_file(fixture.app.root.parent().unwrap().join("actions")).unwrap();
        branch(&fixture, &fixture.session, &page_id(first))
            .await
            .unwrap();
        let calls = action_calls(&fixture);
        assert!(calls.contains(&format!("{new}|session branch {new} {}\n", fixture.session)));
        assert!(calls.contains(&format!(
            "{new}|launch orchestrator chat --provider claude --model opus --account work\n"
        )));

        let unknown = branch(&fixture, &fixture.session, "user@raw-999999-0").await;
        assert_eq!(unknown.unwrap_err().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_chat_with_no_adapter_still_reads_its_chair_to_branch() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = action_fixture();
        // An imported chat has no adapter until its first pane (new_chat.rs resume_chat).
        swarm::store::open(&fixture.app.root.join("swarm.db"))
            .unwrap()
            .execute(
                "UPDATE session SET adapter = NULL WHERE id = ?1",
                [&fixture.session],
            )
            .unwrap();
        let cli = &fixture.app.cli;
        std::fs::write(
            cli,
            r#"#!/bin/sh
printf '%s\n' "${SWARM_ADAPTER-}|$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
  'agents --json') printf '%s\n' '{"agents":[{"id":"orchestrator"}]}' ;;
  'session new') printf '%s\n' 01a1650a-0000-7000-8000-000000000001 ;;
  'launch orchestrator') printf 'fixture-pane\n' ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let rows = handoff_rows(&fixture.app, &fixture.session).await.unwrap();
        let first = rows.iter().find(|r| r.kind == view::RowKind::User).unwrap();
        let page_id = format!("{}@{}", first.id, first.anchor_source_id);
        branch(&fixture, &fixture.session, &page_id).await.unwrap();
        assert_eq!(
            action_calls(&fixture).lines().next(),
            Some("tmux-swarm|agents --json")
        );
    }

    #[tokio::test]
    async fn a_chat_on_another_adapter_reads_its_chair_there() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = action_fixture();
        // The owner's own tmux: another server than the app's `tmux-swarm`, with other pane ids.
        swarm::store::open(&fixture.app.root.join("swarm.db"))
            .unwrap()
            .execute(
                "UPDATE session SET adapter = 'tmux' WHERE id = ?1",
                [&fixture.session],
            )
            .unwrap();
        let cli = &fixture.app.cli;
        std::fs::write(
            cli,
            r#"#!/bin/sh
printf '%s\n' "${SWARM_ADAPTER-}|$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
  'agents --json') printf '%s\n' '{"agents":[{"id":"orchestrator"}]}' ;;
  'session new') printf '%s\n' 01a1650a-0000-7000-8000-000000000001 ;;
  'launch orchestrator') printf 'fixture-pane\n' ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let rows = handoff_rows(&fixture.app, &fixture.session).await.unwrap();
        let first = rows.iter().find(|r| r.kind == view::RowKind::User).unwrap();
        let page_id = format!("{}@{}", first.id, first.anchor_source_id);
        branch(&fixture, &fixture.session, &page_id).await.unwrap();
        assert_eq!(
            action_calls(&fixture).lines().next(),
            Some("tmux|agents --json")
        );
    }

    /// A stand-in CLI that logs every call and lets a test change what two verbs do.
    fn stand_in(fixture: &Fixture, branch: &str, launch: &str) {
        use std::os::unix::fs::PermissionsExt;
        let cli = &fixture.app.cli;
        std::fs::write(
            cli,
            format!(
                r#"#!/bin/sh
printf '%s\n' "${{SWARM_ADAPTER-}}|$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
  'agents --json') printf '%s\n' '{{"agents":[{{"id":"orchestrator"}}]}}' ;;
  'session new') printf '%s\n' 01a1650a-0000-7000-8000-000000000001 ;;
  'session branch') {branch} ;;
  'launch orchestrator') {launch} ;;
esac
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    async fn second_message(fixture: &Fixture) -> String {
        let rows = handoff_rows(&fixture.app, &fixture.session).await.unwrap();
        let mut users = rows.iter().filter(|r| r.kind == view::RowKind::User);
        let row = users.nth(1).unwrap();
        format!("{}@{}", row.id, row.anchor_source_id)
    }

    fn logs_in(fixture: &Fixture) -> usize {
        std::fs::read_dir(fixture.log.parent().unwrap())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "jsonl")
            })
            .count()
    }

    fn with_second_message(fixture: &Fixture) {
        let mut log = std::fs::OpenOptions::new()
            .append(true)
            .open(&fixture.log)
            .unwrap();
        std::io::Write::write_all(
            &mut log,
            br#"{"type":"user","sessionId":"synthetic","uuid":"u9","parentUuid":null,"timestamp":"2026-10-22T10:00:00Z","message":{"role":"user","content":"Now tidy the tests"}}
"#,
        )
        .unwrap();
    }

    #[tokio::test]
    async fn an_archived_workspace_cannot_branch() {
        let fixture = action_fixture();
        stand_in(&fixture, "true", "echo pane");
        with_second_message(&fixture);
        let row = second_message(&fixture).await;
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"))
            .to_string_lossy()
            .into_owned();
        choices_store::update(&fixture.app.root, None, |owner, _| {
            owner.archived.insert(cwd);
            Ok(())
        })
        .unwrap();
        let refused = branch(&fixture, &fixture.session, &row).await.unwrap_err();
        assert_eq!(refused.0, StatusCode::CONFLICT);
        assert!(refused.1.0["error"].as_str().unwrap().contains("read-only"));
        assert!(action_calls(&fixture).is_empty());
    }

    #[tokio::test]
    async fn a_chat_in_a_folder_of_an_archived_workspace_cannot_branch() {
        let fixture = action_fixture();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let inner = swarm::store::create_session(
            &swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap(),
            "lane",
            &root.join("src"),
            Some(("claude", "inner")),
            Some("tmux-swarm"),
        )
        .unwrap();
        choices_store::update(&fixture.app.root, None, |owner, _| {
            owner.archived.insert(root.to_string_lossy().into_owned());
            Ok(())
        })
        .unwrap();
        let refused = branch(&fixture, &inner, "user@raw-0-0").await.unwrap_err();
        assert_eq!(refused.0, StatusCode::CONFLICT);
        assert!(refused.1.0["error"].as_str().unwrap().contains("read-only"));
    }

    #[tokio::test]
    async fn a_failed_step_after_the_cut_removes_the_cut_file() {
        let fixture = action_fixture();
        stand_in(&fixture, "exit 1", "echo pane");
        with_second_message(&fixture);
        let row = second_message(&fixture).await;
        let before = logs_in(&fixture);
        let failed = branch(&fixture, &fixture.session, &row).await;
        assert!(failed.is_err());
        assert_eq!(logs_in(&fixture), before);
    }

    #[tokio::test]
    async fn a_launch_that_prints_no_pane_is_an_error() {
        let fixture = action_fixture();
        stand_in(&fixture, "true", "true");
        with_second_message(&fixture);
        let row = second_message(&fixture).await;
        let before = logs_in(&fixture);
        let failed = branch(&fixture, &fixture.session, &row).await.unwrap_err();
        assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(logs_in(&fixture), before);
    }
}
