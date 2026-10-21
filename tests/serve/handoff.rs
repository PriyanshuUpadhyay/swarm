// Tests for handoff.

use super::*;

#[test]
fn handoff_http_progress_delivery_link_and_cancel() {
    use std::os::unix::fs::PermissionsExt;
    for cancel in [false, true] {
        let server = Server::with_actions(true);
        let home = server.dir.join("home");
        let root = home.join(".swarm");
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join("bin/claude"), "#!/bin/sh\nif [ \"$*\" = 'auth status' ]; then printf '%s\\n' '{\"loggedIn\":true}'; fi\n").unwrap();
        fs::set_permissions(home.join("bin/claude"), fs::Permissions::from_mode(0o755)).unwrap();
        let summary = [
            json!({"type":"user","uuid":"summary-request","message":{"role":"user","content":swarm::view::handoff::REQUEST}}),
            json!({"type":"assistant","uuid":"summary-answer","message":{"role":"assistant","content":[{"type":"text","text":"Keep the files and check the menu"}]}}),
            json!({"type":"system","subtype":"turn_duration","uuid":"summary-end","durationMs":1}),
        ].iter().map(|e| format!("{e}\n")).collect::<String>();
        fs::write(home.join("summary-turn.jsonl"), summary).unwrap();
        fs::write(
            home.join("handoff-ring"),
            format!(
                r#"#!/bin/sh
printf 'ring %s\n' "$SWARM_PANE" >> "$HOME/actions"
printf '%s' "$SWARM_TEXT" > "$HOME/text"
if [ "$SWARM_PANE" = fixture-chair ] && [ ! -f "$HOME/no-summary" ]; then
  cat "$HOME/summary-turn.jsonl" >> '{}'
fi
"#,
                server.log.display().to_string().replace('\'', "'\"'\"'")
            ),
        )
        .unwrap();
        fs::set_permissions(home.join("handoff-ring"), fs::Permissions::from_mode(0o755)).unwrap();
        for adapter in ["fixture", "tmux-swarm"] {
            let file = root.join(format!("adapters/{adapter}.conf"));
            let text = fs::read_to_string(&file).unwrap();
            let text = text
                .lines()
                .map(|line| {
                    if line.starts_with("ring =") {
                        "ring = \"$HOME/handoff-ring\""
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(file, format!("{text}\n")).unwrap();
        }
        if cancel {
            fs::write(home.join("no-summary"), "").unwrap();
        }
        let mut stream = server.sse();
        stream
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        assert_eq!(event(&mut stream).0, "hello");
        std::thread::scope(|scope| {
            let server_ref = &server;
            let request=scope.spawn(move || server_ref.request("POST","/api/chat/continue","Content-Type: application/json\r\n",&json!({"old_session":server_ref.session,"provider":"claude","model":"sonnet","account":"default"}).to_string(),true));
            let mut phases = Vec::new();
            loop {
                let progress = named_event(&mut stream, "handoff");
                assert_eq!(progress["old_session"], server.session);
                let phase = progress["phase"].as_str().unwrap().to_owned();
                phases.push(phase.clone());
                if cancel && phase == "summarizing" {
                    let (status, _, body) = server.request(
                        "POST",
                        "/api/chat/continue/cancel",
                        "Content-Type: application/json\r\n",
                        &json!({"old_session":server.session}).to_string(),
                        true,
                    );
                    assert_eq!(status, 200, "{body}");
                }
                if !cancel && phase == "waiting" {
                    assert_eq!(progress["canCancel"], false);
                    let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
                    let new_id: String = connection
                        .query_row(
                            "SELECT id FROM session WHERE id != ?1 ORDER BY id DESC LIMIT 1",
                            [&server.session],
                            |r| r.get(0),
                        )
                        .unwrap();
                    swarm::store::set_chair(
                        &connection,
                        &new_id,
                        Some(("claude", "stand-in-ready")),
                    )
                    .unwrap();
                }
                if matches!(phase.as_str(), "completed" | "cancelled" | "failed") {
                    break;
                }
            }
            let (status, _, body) = request.join().unwrap();
            let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
            if cancel {
                assert_eq!(status, 409, "{body}");
                assert_eq!(phases, ["preparing", "summarizing", "cancelled"]);
                let count: i64 = connection
                    .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(count, 1);
                let actions = fs::read_to_string(home.join("actions")).unwrap_or_default();
                assert!(!actions.contains("spawn"));
                assert!(!actions.contains("interrupt"));
            } else {
                assert_eq!(status, 200, "{body}");
                assert_eq!(
                    phases,
                    [
                        "preparing",
                        "summarizing",
                        "starting",
                        "waiting",
                        "delivering",
                        "completed"
                    ]
                );
                let response: Value = serde_json::from_str(&body).unwrap();
                let linked: String = connection
                    .query_row(
                        "SELECT continuation_of FROM session WHERE id = ?1",
                        [response["session"].as_str().unwrap()],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(linked, server.session);
                let delivered = fs::read_to_string(home.join("text")).unwrap();
                assert!(delivered.contains("compact handoff summary:"));
                assert!(delivered.contains("Keep the files and check the menu"));
            }
        });
    }
}

#[test]
fn menu_http_matches_cli_json_and_keeps_bearer_guards() {
    let server = Server::with_actions(true);
    let home = server.dir.join("home");
    for (route, args) in [
        ("/api/roles", vec!["roles", "--json"]),
        ("/api/providers", vec!["providers", "--json"]),
        (
            "/api/models?provider=claude",
            vec!["models", "--provider", "claude", "--json"],
        ),
        (
            "/api/accounts?provider=agy",
            vec!["accounts", "--provider", "agy", "--json"],
        ),
    ] {
        let (status, _, body) = server.request("GET", route, "", "", true);
        assert_eq!(status, 200, "{body}");
        let cli = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &home)
            .env("SWARM_HOME", &home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .args(args)
            .output()
            .unwrap();
        assert!(
            cli.status.success(),
            "{}",
            String::from_utf8_lossy(&cli.stderr)
        );
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            serde_json::from_slice::<Value>(&cli.stdout).unwrap()
        );
        assert_eq!(server.request("GET", route, "", "", false).0, 401);
    }
    for route in ["/api/models", "/api/accounts?provider=unknown"] {
        assert_eq!(server.request("GET", route, "", "", true).0, 400);
    }
    for route in ["/api/chat/continue", "/api/chat/continue/cancel"] {
        assert_eq!(
            server
                .request(
                    "POST",
                    route,
                    "Content-Type: application/json\r\n",
                    "{}",
                    false
                )
                .0,
            401
        );
        assert_eq!(
            server
                .request(
                    "POST",
                    route,
                    "Content-Type: application/json\r\n",
                    "{}",
                    true
                )
                .0,
            400
        );
    }
}

/// A same-CLI switch keeps the chat: it closes the chair and resumes its own session on the new
/// run. Start clean makes a linked chat with nothing typed into it (ADR 0108).
#[test]
fn a_switch_resumes_the_same_chat_or_starts_clean() {
    use std::os::unix::fs::PermissionsExt;
    for carry in [
        "resume",
        "clean",
        "other-cli",
        "resume-no-log",
        "clean-no-log",
        "bad-effort",
        "no-id",
    ] {
        let server = Server::with_actions(true);
        // A new chat has no log before its first turn; nothing it needs comes from the log.
        if carry.ends_with("no-log") {
            fs::remove_file(server.dir.join("chair.jsonl")).unwrap();
        }
        let home = server.dir.join("home");
        let root = home.join(".swarm");
        if carry == "no-id" {
            // A Codex or AGY chat learns its session id from its first log.
            swarm::store::open(&root.join("swarm.db"))
                .unwrap()
                .execute("UPDATE session SET chair_id = NULL", [])
                .unwrap();
        }
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join("bin/claude"), "#!/bin/sh\nif [ \"$*\" = 'auth status' ]; then printf '%s\\n' '{\"loggedIn\":true}'; fi\n").unwrap();
        fs::set_permissions(home.join("bin/claude"), fs::Permissions::from_mode(0o755)).unwrap();
        let (provider, sent) = match carry {
            "other-cli" => ("codex", "resume"),
            "resume-no-log" | "bad-effort" | "no-id" => ("claude", "resume"),
            "clean-no-log" => ("claude", "clean"),
            other => ("claude", other),
        };
        let effort = if carry == "bad-effort" {
            "bogus"
        } else {
            "low"
        };
        let body = json!({"old_session":server.session,"provider":provider,"model":"sonnet","effort":effort,"carry":sent});
        let (status, _, reply) = server.request(
            "POST",
            "/api/chat/continue",
            "Content-Type: application/json\r\n",
            &body.to_string(),
            true,
        );
        let actions = fs::read_to_string(home.join("actions")).unwrap_or_default();
        let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let sessions: i64 = connection
            .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
            .unwrap();
        match carry {
            "resume-no-log" | "clean-no-log" => assert_eq!(status, 200, "{carry}: {reply}"),
            "no-id" => {
                // Nothing could start again, so the chair keeps running.
                assert_eq!(status, 409, "{reply}");
                assert!(reply.contains("no session to resume"), "{reply}");
                assert!(!actions.contains("close"), "{actions}");
            }
            "bad-effort" => {
                // The refused launch comes after the close, so the old run starts again.
                assert_eq!(status, 409, "{reply}");
                assert!(reply.contains("not a level"), "{reply}");
                assert!(actions.contains("close fixture-chair"), "{actions}");
                assert!(
                    actions.contains("spawn"),
                    "the chair starts again: {actions}"
                );
            }
            "resume" => {
                assert_eq!(status, 200, "{reply}");
                let reply: Value = serde_json::from_str(&reply).unwrap();
                assert_eq!(reply["session"], server.session);
                assert_eq!(sessions, 1);
                assert!(actions.contains("close fixture-chair"), "{actions}");
                let script = fs::read_to_string(
                    root.join(format!("runs/{}/orchestrator.sh", server.session)),
                )
                .unwrap();
                for part in [
                    "'--model' 'sonnet' '--effort' 'low'",
                    "'--resume' 'synthetic'",
                ] {
                    assert!(script.contains(part), "{part} not in {script}");
                }
            }
            "clean" => {
                assert_eq!(status, 200, "{reply}");
                let reply: Value = serde_json::from_str(&reply).unwrap();
                let linked: String = connection
                    .query_row(
                        "SELECT continuation_of FROM session WHERE id = ?1",
                        [reply["session"].as_str().unwrap()],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(linked, server.session);
                // A launch may ring its own pane; no summary or recent message goes in.
                let typed = fs::read_to_string(home.join("text")).unwrap_or_default();
                assert!(!typed.contains("prior"), "nothing is carried: {typed}");
            }
            _ => {
                assert_eq!(status, 409, "{reply}");
                assert!(reply.contains("Start clean"), "{reply}");
                assert!(!actions.contains("close"), "{actions}");
            }
        }
    }
}
