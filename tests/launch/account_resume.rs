// The account a chat runs on: pi's own logins, and a Codex chat moved to another account (ADR 0122).

use super::*;

/// pi's `default` launches as the model pi's settings name, so the chat records which login it
/// runs on and its run chip can offer that login's accounts.
#[test]
fn a_pi_default_launch_records_the_model_pi_settings_name() {
    let home = scratch("pi-default");
    tool(&home, "pi", "true");
    std::fs::create_dir_all(home.join(".pi/agent")).unwrap();
    std::fs::write(
        home.join(".pi/agent/settings.json"),
        r#"{"defaultProvider": "openai-codex", "defaultModel": "gpt-6-sol"}"#,
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let session = swarm(
        &home,
        &[("SWARM_ADAPTER", "fake")],
        &["session", "new", "lane"],
    );
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
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
            "pi",
            "--model",
            "default",
            "--cwd",
            &cwd,
        ],
    );
    assert!(launched.status.success(), "{}", stderr(&launched));
    let command =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
            .unwrap();
    assert!(
        command.contains("'--model' 'openai-codex/gpt-6-sol'"),
        "{command}"
    );
    let model: String = swarm::store::open(&home.join(".swarm/swarm.db"))
        .unwrap()
        .query_row(
            "SELECT model FROM agent WHERE id = 'orchestrator'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(model, "openai-codex/gpt-6-sol");
}

/// pi on a GPT model runs on the Codex account the launch picked, through `PI_CODEX_HOME`, and
/// never on a Codex home the app's own shell set.
#[test]
fn a_pi_launch_on_openai_codex_gets_the_codex_account() {
    let home = scratch("pi-codex-account");
    tool(&home, "pi", "true");
    tool(
        &home,
        "codex",
        &format!(
            "if [ \"$*\" = 'app-server --listen stdio://' ]; then\n{}\nfi",
            include_str!("../fixtures/accounts/work-app-server.sh")
        ),
    );
    std::fs::create_dir_all(home.join(".codex-work")).unwrap();
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let session = swarm(
        &home,
        &[("SWARM_ADAPTER", "fake")],
        &["session", "new", "lane"],
    );
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
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
            "pi",
            "--model",
            "openai-codex/gpt-6-sol",
            "--account",
            "work",
            "--cwd",
            &cwd,
        ],
    );
    assert!(launched.status.success(), "{}", stderr(&launched));
    let command =
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
            .unwrap();
    let codex_home = std::fs::canonicalize(home.join(".codex-work")).unwrap();
    for part in [
        "'-u' 'PI_CODEX_HOME'".to_string(),
        format!("'PI_CODEX_HOME={}'", codex_home.display()),
        "'PI_CODEX_PROFILE=work'".into(),
    ] {
        assert!(command.contains(&part), "{part} not in {command}");
    }
    assert!(!command.contains("'CODEX_HOME="), "{command}");
}

/// A Codex chat switched to another account takes its rollout along, so the new account resumes
/// the same session. Auto then stays on the account that ran it last, and a move back replaces
/// the old copy (probe 2026-10-25: Codex resumed a copied rollout on another account).
#[test]
fn a_codex_resume_on_another_account_moves_the_session() {
    let home = scratch("codex-move");
    tool(
        &home,
        "codex",
        &format!(
            "if [ \"$*\" = 'app-server --listen stdio://' ]; then\n{}\nfi",
            include_str!("../fixtures/accounts/work-app-server.sh")
        ),
    );
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        "self = printf chair\nspawn = printf pane\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let id = "0199a000-0000-7000-8000-0000000000aa";
    let relative = format!("sessions/2026/10/01/rollout-2026-10-01T10-00-00-{id}.jsonl");
    let (work, other) = (home.join(".codex-work"), home.join(".codex-other"));
    std::fs::create_dir_all(work.join("sessions/2026/10/01")).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(work.join(&relative), "{\"type\":\"session_meta\"}\n").unwrap();
    let cwd = home.join("repo");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let fake = [("SWARM_ADAPTER", "fake")];
    let chair = format!("codex:{id}");
    let session = swarm(&home, &fake, &["session", "new", "lane", "--chair", &chair]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let env = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
    ];
    let resume = |account: &str| {
        let launched = swarm(
            &home,
            &env,
            &[
                "launch",
                "orchestrator",
                "chat",
                "--resume",
                "--account",
                account,
                "--cwd",
                &cwd,
            ],
        );
        assert!(launched.status.success(), "{}", stderr(&launched));
        swarm::store::open(&home.join(".swarm/swarm.db"))
            .unwrap()
            .execute("UPDATE agent SET pane_id = NULL", [])
            .unwrap();
        std::fs::read_to_string(home.join(format!(".swarm/runs/{session}/orchestrator.sh")))
            .unwrap()
    };
    let codex_home = |dir: &Path| format!("'CODEX_HOME={}'", dir.canonicalize().unwrap().display());
    let command = resume("other");
    assert!(command.contains(&codex_home(&other)), "{command}");
    assert!(command.contains(&format!("'resume' '{id}'")), "{command}");
    assert_eq!(
        std::fs::read_to_string(other.join(&relative)).unwrap(),
        "{\"type\":\"session_meta\"}\n"
    );
    let writer: String = swarm::store::open(&home.join(".swarm/swarm.db"))
        .unwrap()
        .query_row("SELECT writer FROM managed_edit", [], |r| r.get(0))
        .unwrap();
    assert_eq!(writer, "launch.codex_move");
    // The chat runs on, so its copy on `other` is the newest.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(
        other.join(&relative),
        "{\"type\":\"session_meta\"}\n{\"turn\":2}\n",
    )
    .unwrap();
    assert!(resume("auto").contains(&codex_home(&other)));
    let command = resume("work");
    assert!(command.contains(&codex_home(&work)), "{command}");
    assert!(
        std::fs::read_to_string(work.join(&relative))
            .unwrap()
            .contains("\"turn\":2")
    );
}
