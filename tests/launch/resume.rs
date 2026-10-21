// A chair resumed in its chat on a new run, and a new chair in an old chat (ADR 0108).

use super::*;

/// A model switch resumes the chair's own session on the new model and effort (ADR 0108).
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
            "pi:",
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
        // `session new` stores PI_SESSION_ID, or any `--chair pi:<id>`, as pi's chair id.
        let chair = if chair == "pi:" { "pi:p1" } else { chair };
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
