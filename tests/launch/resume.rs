// A chair resumed in its chat on a new run, and a new chair in an old chat (ADR 0110).

use super::*;

/// A model switch resumes the chair's own session on the new model and effort (ADR 0110).
#[test]
fn a_resume_runs_the_chosen_model_and_effort_in_the_same_session() {
    let home = scratch("resume-switch");
    tool(&home, "claude", "true");
    tool(&home, "pi", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    for (chair, model, effort, want) in [
        (
            "claude:abc",
            "sonnet",
            "low",
            vec!["'--model' 'sonnet' '--effort' 'low'", "'--resume' 'abc'"],
        ),
        (
            "pi:p1",
            "claude-bridge/claude-sonnet-5-5",
            "minimal",
            vec![
                "'--model' 'claude-bridge/claude-sonnet-5-5' '--thinking' 'minimal'",
                // The stored id, not `<session>-orchestrator`, or pi opens a new empty session.
                "'--session-id' 'p1'",
            ],
        ),
    ] {
        let fake = [("SWARM_ADAPTER", "fake")];
        // `session new` stores any `--chair pi:<id>` as pi's chair id.
        let session = swarm(&home, &fake, &["session", "new", "lane", "--chair", chair]);
        assert!(session.status.success(), "{}", stderr(&session));
        let session = String::from_utf8(session.stdout)
            .unwrap()
            .trim()
            .to_string();
        let resumed = swarm(
            &home,
            &[
                ("SWARM_ADAPTER", "fake"),
                ("SWARM_SESSION_ID", session.as_str()),
            ],
            &[
                "launch",
                "orchestrator",
                "chat",
                "--resume",
                "--model",
                model,
                "--effort",
                effort,
                "--cwd",
                &cwd,
            ],
        );
        assert!(resumed.status.success(), "{}", stderr(&resumed));
        let command =
            std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
                .unwrap();
        for part in want {
            assert!(command.contains(part), "{part} not in {command}");
        }
    }
    // Claude has no `ultra` level, so the switch is refused before any pane starts.
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(
        &home,
        &fake,
        &["session", "new", "lane", "--chair", "claude:abc"],
    );
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let bad = swarm(
        &home,
        &[
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", session.as_str()),
        ],
        &[
            "launch",
            "orchestrator",
            "chat",
            "--resume",
            "--effort",
            "ultra",
            "--cwd",
            &cwd,
        ],
    );
    assert!(
        stderr(&bad).contains("not a level of this CLI"),
        "{}",
        stderr(&bad)
    );
}

/// A new AGY chair in a chat drops the old conversation id, so the new chair's log is taken.
#[test]
fn a_new_agy_chair_forgets_the_old_conversation() {
    let home = scratch("agy-new-chair");
    tool(&home, "agy", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(&home, &fake, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    {
        let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
        swarm::store::set_chair_provider(&connection, &session, "agy").unwrap();
        let old = "/h/.gemini/antigravity-cli/brain/90f7dfab-e45b-43ed-87d0-1b6497513ac7/x.jsonl";
        swarm::store::set_chair_log(&connection, &session, std::path::Path::new(old)).unwrap();
    }
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
    ];
    let launched = swarm(
        &home,
        &env,
        &[
            "launch",
            "orchestrator",
            "chat",
            "--provider",
            "agy",
            "--model",
            "gemini-3.8-flash",
            "--effort",
            "high",
            "--cwd",
            &cwd,
        ],
    );
    assert!(launched.status.success(), "{}", stderr(&launched));
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    let chair: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT chair_provider, chair_id FROM session WHERE id = ?1",
            [&session],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(chair, (Some("agy".into()), None));
}

/// The plain Resume of an ended Pi or AGY chat runs its stored model, not the chat profile,
/// which has no Pi or AGY runner.
#[test]
fn a_plain_resume_runs_the_chairs_stored_model() {
    let home = scratch("resume-plain");
    tool(&home, "pi", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(&home, &fake, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
    ];
    let model = "claude-bridge/claude-sonnet-5-5";
    for args in [
        vec!["--provider", "pi", "--model", model, "--effort", "high"],
        vec!["--resume"],
    ] {
        let mut all = vec!["launch", "orchestrator", "chat"];
        all.extend(args);
        all.extend(["--cwd", &cwd]);
        let launched = swarm(&home, &env, &all);
        assert!(launched.status.success(), "{}", stderr(&launched));
        // The pane ends, so the chat offers Resume.
        swarm::store::open(&home.join(".swarm/swarm.db"))
            .unwrap()
            .execute("UPDATE agent SET pane_id = NULL", [])
            .unwrap();
    }
    let command =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
            .unwrap();
    for part in [
        format!("'--model' '{model}' '--thinking' 'high'"),
        format!("'--session-id' '{session}-orchestrator'"),
    ] {
        assert!(command.contains(&part), "{part} not in {command}");
    }
}

/// A plain Resume of a chat whose profile has a runner for its CLI runs that runner, so a stored
/// transcript id (no `[1m]`) does not replace it, and a non-chat role resumes too (ADR 0107).
#[test]
fn a_plain_resume_runs_the_profile_runner_when_it_has_one() {
    let home = scratch("resume-profile");
    tool(&home, "claude", "true");
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    for role in ["chat", "review.deep"] {
        let fake = [("SWARM_ADAPTER", "fake")];
        let session = swarm(&home, &fake, &["session", "new", "lane"]);
        assert!(session.status.success(), "{}", stderr(&session));
        let session = String::from_utf8(session.stdout)
            .unwrap()
            .trim()
            .to_string();
        let env = [
            ("SWARM_ADAPTER", "fake"),
            ("SWARM_SESSION_ID", session.as_str()),
        ];
        let first = swarm(
            &home,
            &env,
            &[
                "launch",
                "orchestrator",
                role,
                "--provider",
                "claude",
                "--cwd",
                &cwd,
            ],
        );
        assert!(first.status.success(), "{}", stderr(&first));
        // The chair ran one turn, so the store holds its transcript model and session id.
        let db = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
        db.execute(
            "UPDATE agent SET pane_id = NULL, model = 'claude-opus-9-9' WHERE session_id = ?1",
            [&session],
        )
        .unwrap();
        db.execute(
            "UPDATE session SET chair_provider = 'claude', chair_id = 'abc' WHERE id = ?1",
            [&session],
        )
        .unwrap();
        let resumed = swarm(
            &home,
            &env,
            &["launch", "orchestrator", role, "--resume", "--cwd", &cwd],
        );
        assert!(resumed.status.success(), "{role}: {}", stderr(&resumed));
        let command =
            std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
                .unwrap();
        assert!(!command.contains("claude-opus-9-9"), "{role}: {command}");
        assert!(command.contains("'--resume' 'abc'"), "{role}: {command}");
    }
}
