//! Tests for CLI hooks.

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    #[test]
    fn a_cli_run_inside_the_agents_own_cli_is_nested() {
        let chain = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        // The chair's own hook: one AGY above it.
        assert!(!nested_run(
            &chain(&["sh", "/Users/x/.local/bin/agy", "zsh", "tmux"]),
            "agy"
        ));
        // `agy -p` from the chair's shell tool: a second AGY above the hook.
        assert!(nested_run(
            &chain(&[
                "/Users/x/.local/bin/agy",
                "zsh",
                "/Users/x/.local/bin/agy",
                "tmux"
            ]),
            "agy"
        ));
        // Another CLI in between does not count; the provider guard owns that case.
        assert!(!nested_run(
            &chain(&["codex", "zsh", "pi", "tmux"]),
            "codex"
        ));
    }

    #[test]
    fn hook_stdin_read_ends_at_its_deadline_while_the_writer_holds_the_pipe() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut writer, b"{}").unwrap();
        let started = std::time::Instant::now();
        assert_eq!(
            read_within(reader, std::time::Duration::from_millis(200)),
            None
        );
        assert!(started.elapsed() < std::time::Duration::from_millis(600));
        drop(writer);
        let (reader, mut writer) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut writer, b"{}").unwrap();
        drop(writer);
        assert_eq!(
            read_within(reader, std::time::Duration::from_millis(200)).as_deref(),
            Some("{}")
        );
    }

    #[test]
    fn a_hook_outside_a_swarm_agent_reports_nothing() {
        let claude = ["claude".to_string()];
        let waiting = r#"{"hook_event_name":"PermissionRequest"}"#;
        let session = "0199a000-0000-7000-8000-000000000001";
        let no_env = |_: &str| None;
        assert!(hook_report(&claude, waiting, no_env).unwrap().is_none());
        let session_only = |name: &str| (name == "SWARM_SESSION_ID").then(|| session.to_string());
        assert!(
            hook_report(&claude, waiting, session_only)
                .unwrap()
                .is_none()
        );
        assert!(hook_report(&claude, "not json", no_env).unwrap().is_none());

        let coder = |name: &str| match name {
            "SWARM_SESSION_ID" => Some(session.to_string()),
            "SWARM_AGENT_ID" => Some(CODER.to_string()),
            _ => None,
        };
        let report = |state, log| {
            Some(HookReport {
                session: session.to_string(),
                agent: CODER.to_string(),
                state,
                log,
                usage_provider: None,
            })
        };
        assert_eq!(
            hook_report(&claude, waiting, coder).unwrap(),
            report(
                Some(swarm::host::HookState {
                    state: "waiting",
                    detail: None,
                    kind: Some("permission"),
                }),
                None
            )
        );
        assert!(
            hook_report(&claude, r#"{"hook_event_name":"SessionStart"}"#, coder)
                .unwrap()
                .is_none()
        );
        assert!(hook_report(&claude, "not json", coder).is_err());
        let agy_stop = ["agy".to_string(), "Stop".to_string()];
        assert_eq!(
            hook_report(&agy_stop, r#"{"error":"quota"}"#, coder).unwrap(),
            report(
                Some(swarm::host::HookState {
                    state: "failed",
                    detail: Some("quota".to_string()),
                    kind: None,
                }),
                None
            )
        );
        assert!(hook_report(&["gemini".to_string()], waiting, coder).is_err());
    }

    #[test]
    fn a_hook_reports_the_chat_log_its_payload_names() {
        let dir = std::env::temp_dir().join(format!("swarm-hook-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("chat.jsonl");
        std::fs::write(&log, "").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        let session = "0199a000-0000-7000-8000-000000000001";
        let coder = |name: &str| match name {
            "SWARM_SESSION_ID" => Some(session.to_string()),
            "SWARM_AGENT_ID" => Some(CODER.to_string()),
            _ => None,
        };
        let path = |value: &std::path::Path| serde_json::to_string(value).unwrap();
        let reported = |args: &[&str], payload: String| {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            hook_report(&args, &payload, coder).unwrap()
        };

        // A start event says nothing about state, yet its log path is kept.
        let start = format!(
            r#"{{"hook_event_name":"SessionStart","transcript_path":{}}}"#,
            path(&log)
        );
        let claude = reported(&["claude"], start).unwrap();
        assert_eq!((claude.state, claude.log), (None, Some(log.clone())));
        let codex = format!(
            r#"{{"hook_event_name":"PermissionRequest","transcript_path":{}}}"#,
            path(&log)
        );
        let codex = reported(&["codex"], codex).unwrap();
        assert_eq!(
            (codex.state, codex.log),
            (
                Some(swarm::host::HookState {
                    state: "waiting",
                    detail: None,
                    kind: Some("permission"),
                }),
                Some(log.clone())
            )
        );
        let agy = reported(
            &["agy", "PreInvocation"],
            format!(r#"{{"transcriptPath":{}}}"#, path(&log)),
        )
        .unwrap();
        assert_eq!(agy.log, Some(log.clone()));

        // Codex may send null; a relative, missing, or non-jsonl path is not a chat log.
        for bad in [
            "null".to_string(),
            r#""chat.jsonl""#.to_string(),
            path(&dir.join("missing.jsonl")),
            path(&dir.join("notes.txt")),
            path(&dir),
        ] {
            let payload =
                format!(r#"{{"hook_event_name":"SessionStart","transcript_path":{bad}}}"#);
            assert!(reported(&["claude"], payload).is_none(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A failed capture hides only the screen. The store still holds a turn-start hook or a read
    /// after the ring, so a late pass stores that proof, not unconfirmed (ADR 0041).
    #[test]
    fn a_failed_capture_still_finds_the_hook_or_the_read_in_the_store() {
        let proofs = [
            (
                "hook",
                "UPDATE agent SET state = 'working', state_source = 'hook', state_at = unixepoch()",
            ),
            ("seen", "UPDATE message SET seen_at = unixepoch()"),
        ];
        for (proof, update) in proofs {
            let (root, mut connection, session) =
                ring_session("capture-fails-proof", Some("claude"), "ring = true", "");
            let ask = swarm::store::send_message(
                &mut connection,
                &root,
                &session,
                ORCHESTRATOR,
                CODER,
                "ask",
                "task",
            )
            .unwrap();
            connection
                .execute(
                    "UPDATE message SET rung_at = unixepoch() - ?1, rings = 1",
                    [RING_TIMEOUT.as_secs() + 1],
                )
                .unwrap();
            connection.execute(update, []).unwrap();
            std::fs::remove_file(root.join("screen")).unwrap();
            let adapter = swarm::adapter::load(&root, "fake").unwrap();
            settle_rings(
                &mut connection,
                &root,
                &adapter,
                &session,
                Proof::Wait,
                &mut Vec::new(),
            )
            .unwrap();
            assert_eq!(
                delivery_of(&connection, &session, ask).as_deref(),
                Some(proof)
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn stop_hook_claude_replaces_cumulative_cost_and_model() {
        let log = format!(
            "{}\n{}\n{}\n{}\n",
            include_str!("../../../packages/transcript/src/fixtures/claude-chair-run.jsonl"),
            r#"{"type":"cost-state","totalCostUSD":1.25,"hasUnknownModelCost":false}"#,
            r#"{"type":"assistant","message":{"model":"claude-new"}}"#,
            r#"{"type":"cost-state","totalCostUSD":2.5,"hasUnknownModelCost":false}"#
        );
        let (home, connection, session) = stop_fixture("claude", Some(&log));
        for _ in 0..2 {
            run_stop_hook(&home, &session, "claude", "Stop", "{}");
            let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
            assert_eq!(agent.cost_usd, Some(2.5));
            assert_eq!(agent.model.as_deref(), Some("claude-new"));
            assert_eq!(agent.tokens, None);
        }
        std::fs::write(
            home.join("chat.jsonl"),
            r#"{"type":"cost-state","totalCostUSD":0,"hasUnknownModelCost":false}"#,
        )
        .unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.cost_usd, Some(0.0));
        assert_eq!(agent.model.as_deref(), Some("claude-new"));
    }

    #[test]
    fn a_hook_from_another_cli_than_the_agents_changes_nothing() {
        let (home, connection, session) = stop_fixture("other-cli", Some("{}\n"));
        swarm::store::set_provider(&connection, &session, CODER, "claude").unwrap();
        let stray = home.join("stray.jsonl");
        std::fs::write(&stray, "").unwrap();
        let payload = format!(
            r#"{{"transcriptPath":{}}}"#,
            serde_json::to_string(&stray).unwrap()
        );
        run_stop_hook(&home, &session, "agy", "Stop", &payload);
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(
            agent.log.as_deref(),
            Some(home.join("chat.jsonl").to_str().unwrap())
        );
        assert_eq!(agent.state, None);
        // The agent's own CLI still reports.
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        assert_eq!(
            swarm::store::agents(&connection, &session).unwrap()[0]
                .state
                .as_deref(),
            Some("done")
        );
    }

    #[test]
    fn stop_hook_commits_done_state_before_usage() {
        let log = r#"{"type":"assistant","message":{"id":"m1","model":"claude","usage":{"input_tokens":1,"output_tokens":2}}}"#;
        let (home, connection, session) = stop_fixture("state-before-usage", Some(log));
        swarm::store::set_state(
            &connection,
            &session,
            CODER,
            "working",
            "hook",
            None,
            None,
            1,
        )
        .unwrap();
        // Reject a usage write unless the done state is already stored in the same row.
        connection.execute_batch("CREATE TRIGGER usage_requires_done BEFORE UPDATE OF usage_state ON agent WHEN NEW.state IS NOT 'done' BEGIN SELECT RAISE(ABORT, 'usage before done'); END;").unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.state.as_deref(), Some("done"));
        assert_eq!(agent.tokens, Some(3));
    }

    #[test]
    fn stop_hook_claude_adds_only_new_messages_in_a_growing_fixture() {
        use std::io::Write;
        let first = r#"{"type":"assistant","message":{"id":"m1","model":"claude","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":4}}}"#;
        let second = first.replace("m1", "m2");
        let log = format!(
            "{}\n{first}\n{first}\n",
            include_str!("../../../packages/transcript/src/fixtures/claude-chair-run.jsonl")
        );
        let (home, connection, session) = stop_fixture("claude-growing", Some(&log));
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        assert_eq!(
            swarm::store::agents(&connection, &session).unwrap()[0].tokens,
            Some(64)
        );
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(home.join("chat.jsonl"))
            .unwrap();
        writeln!(file, "{second}\n{first}\n{second}").unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.tokens, Some(128));
        assert_eq!(agent.cost_usd, None);
        assert_eq!(agent.state.as_deref(), Some("done"));
    }

    #[test]
    fn stop_hook_codex_uses_total_tokens_and_latest_model() {
        let log = format!(
            "{}\n{}\n{}\n{}\n",
            include_str!("../../../packages/transcript/src/fixtures/codex-chair-run.jsonl"),
            r#"{"type":"turn_context","payload":{"model":"gpt-old"}}"#,
            r#"{"type":"turn_context","payload":{"model":"gpt-new"}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":12},"total_token_usage":{"input_tokens":90,"output_tokens":10,"total_tokens":100}}}}"#
        );
        let (home, connection, session) = stop_fixture("codex", Some(&log));
        run_stop_hook(&home, &session, "codex", "Stop", "{}");
        let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
        assert_eq!(agent.tokens, Some(100));
        assert_eq!(agent.model.as_deref(), Some("gpt-new"));
        assert_eq!(agent.cost_usd, None);
    }

    #[test]
    fn stop_hook_claude_reads_past_a_mebibyte_and_keeps_cost_without_a_complete_record() {
        let snapshot = r#"{"type":"cost-state","totalCostUSD":4,"hasUnknownModelCost":false}"#;
        let log = format!(
            "{snapshot}\n{}\n{snapshot}\n",
            "x".repeat(1024 * 1024 + 100)
        );
        let (home, connection, session) = stop_fixture("tail", Some(&log));
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        assert_eq!(
            swarm::store::agents(&connection, &session).unwrap()[0].cost_usd,
            Some(4.0)
        );
        // A cost record without a completeness flag does not replace a reported cost.
        std::fs::write(
            home.join("chat.jsonl"),
            format!(
                "{{\"type\":\"cost-state\",\"totalCostUSD\":9}}\n{}",
                "x".repeat(1024 * 1024 + 100)
            ),
        )
        .unwrap();
        run_stop_hook(&home, &session, "claude", "Stop", "{}");
        assert_eq!(
            swarm::store::agents(&connection, &session).unwrap()[0].cost_usd,
            Some(4.0)
        );
    }

    #[test]
    fn stop_hook_missing_unreadable_or_snapshot_free_log_keeps_values() {
        let (home, connection, session) = stop_fixture("no-snapshot", Some("not json\n{}\n"));
        connection
            .execute(
                "UPDATE agent SET model = 'kept', cost_usd = 3, tokens = 5 WHERE session_id = ?1 AND id = ?2",
                (&session, CODER),
            )
            .unwrap();
        for mode in ["no-snapshot", "missing", "unreadable"] {
            if mode == "missing" {
                std::fs::remove_file(home.join("chat.jsonl")).unwrap();
            } else if mode == "unreadable" {
                use std::os::unix::fs::PermissionsExt;
                std::fs::write(
                    home.join("chat.jsonl"),
                    r#"{"type":"cost-state","totalCostUSD":9}"#,
                )
                .unwrap();
                std::fs::set_permissions(
                    home.join("chat.jsonl"),
                    std::fs::Permissions::from_mode(0o000),
                )
                .unwrap();
                assert!(std::fs::File::open(home.join("chat.jsonl")).is_err());
            }
            run_stop_hook(&home, &session, "claude", "Stop", "{}");
            let agent = &swarm::store::agents(&connection, &session).unwrap()[0];
            assert_eq!(agent.model.as_deref(), Some("kept"));
            assert_eq!(agent.cost_usd, Some(3.0));
            assert_eq!(agent.tokens, Some(5));
            assert_eq!(agent.state.as_deref(), Some("done"));
        }
    }

    #[test]
    fn stop_hook_other_provider_interrupt_and_subagent_keep_usage() {
        let log = r#"{"type":"cost-state","totalCostUSD":9}"#;
        let (home, connection, session) = stop_fixture("other-provider", Some(log));
        connection
            .execute(
                "UPDATE agent SET cost_usd = 3 WHERE session_id = ?1 AND id = ?2",
                (&session, CODER),
            )
            .unwrap();
        for (provider, event, payload) in [
            ("agy", "Stop", "{}"),
            ("codex", "Interrupt", "{}"),
            ("claude", "Stop", r#"{"agent_id":"subagent"}"#),
        ] {
            run_stop_hook(&home, &session, provider, event, payload);
            assert_eq!(
                swarm::store::agents(&connection, &session).unwrap()[0].cost_usd,
                Some(3.0)
            );
        }
    }
}
