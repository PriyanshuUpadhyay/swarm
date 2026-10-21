use super::*;

fn session(id: &str, provider_id: Option<&str>, messages: i64) -> Session {
    Session {
        id: id.into(),
        agent_type: "codex".into(),
        provider_id: provider_id.map(Into::into),
        title: format!("chat {id}"),
        created_at: 1_782_116_698,
        updated_at: 1_782_120_000,
        cwd: Some("/w".into()),
        messages,
        ..Session::default()
    }
}

fn reasons(plan: &Plan) -> Vec<(&str, &str)> {
    plan.skipped
        .iter()
        .map(|s| (s.conductor_id.as_str(), s.reason.as_str()))
        .collect()
}

#[test]
fn plan_skips_empty_known_and_duplicate_sessions() {
    let known = Known {
        provider_ids: ["in-swarm".to_owned()].into(),
        logs: ["/rebuilt/copied.jsonl".to_owned()].into(),
    };
    let sessions = vec![
        session("empty", None, 0),
        session("fewer", Some("thread"), 3),
        session("more", Some("thread"), 9),
        session("known", Some("in-swarm"), 5),
        session("copied", None, 2),
        Session {
            agent_type: "amp".into(),
            ..session("amp", None, 4)
        },
        Session {
            cwd: None,
            ..session("nowhere", None, 4)
        },
        session("new", None, 1),
    ];
    let plan = plan(
        sessions,
        &known,
        |_| None,
        |session, _| PathBuf::from(format!("/rebuilt/{}.jsonl", session.id)),
    );
    assert_eq!(
        reasons(&plan),
        [
            ("empty", "no messages"),
            ("fewer", "the same chat as Conductor session more"),
            ("known", "already in swarm"),
            ("copied", "already in swarm"),
            ("amp", "a amp chat has no log swarm reads"),
            ("nowhere", "no workspace folder"),
        ]
    );
    let ids: Vec<_> = plan.chats.iter().map(|c| c.conductor_id.as_str()).collect();
    assert_eq!(ids, ["more", "new"]);
    let new = &plan.chats[1];
    assert_eq!(new.log, "/rebuilt/new.jsonl");
    assert_eq!(
        new.provider_id.as_deref(),
        Some(new.id.as_str()),
        "a rebuilt session takes the swarm id"
    );
}

#[test]
fn plan_keeps_a_native_log_and_its_provider() {
    let hidden = Session {
        hidden: true,
        selected: true,
        ..session("closed", Some("thread"), 4)
    };
    let plan = plan(
        vec![hidden],
        &Known::default(),
        |_| Some(PathBuf::from("/codex/rollout-thread.jsonl")),
        |_, _| unreachable!(),
    );
    let chat = &plan.chats[0];
    assert_eq!(
        (chat.provider.as_str(), chat.log.as_str(), chat.native_log),
        ("codex", "/codex/rollout-thread.jsonl", true)
    );
    assert_eq!(chat.provider_id.as_deref(), Some("thread"));
    assert_eq!(
        (chat.open, chat.selected, chat.closed_at),
        (false, false, Some(1_782_120_000))
    );
}

#[test]
fn swarm_id_is_stable_and_older_than_a_new_session() {
    let old = session("30daa5bb-8470-417e-8047-0f615852baf6", None, 1);
    assert_eq!(swarm_id(&old), swarm_id(&old));
    let id = uuid::Uuid::parse_str(&swarm_id(&old)).unwrap();
    assert_eq!(id.get_version_num(), 7);
    // `continue_session` links a newer session only when the old id sorts first.
    assert!(swarm_id(&old) < uuid::Uuid::now_v7().to_string());
}

/// A Conductor database with one open chat that has messages and one closed one, in `dir`.
fn conductor_db(dir: &Path) -> PathBuf {
    let path = dir.join("conductor.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(&format!(
        "CREATE TABLE workspaces (id TEXT, workspace_path TEXT, active_session_id TEXT);
         CREATE TABLE sessions (id TEXT, agent_type TEXT, claude_session_id TEXT, title TEXT,
                                is_hidden INTEGER, created_at TEXT, updated_at TEXT,
                                workspace_id TEXT);
         CREATE TABLE session_messages (id TEXT, session_id TEXT, role TEXT, content TEXT,
                                        created_at TEXT, cancelled_at TEXT);
         INSERT INTO workspaces VALUES ('w', '{ws}', 'open');
         INSERT INTO sessions VALUES
           ('open', 'claude', 'claude-id', 'Plan sync', 0, '2026-06-22T08:24:58.551Z',
            '2026-06-22 11:00:15', 'w'),
           ('closed', 'codex', '', 'Untitled', 1, '2026-06-23T08:00:00.000Z',
            '2026-06-23 09:00:00', 'w'),
           ('empty', 'codex', '', 'Untitled', 0, '2026-06-24T08:00:00.000Z',
            '2026-06-24 09:00:00', 'w');
         INSERT INTO session_messages VALUES
           ('m1', 'open', 'user', 'plan it', '2026-06-22T08:25:00.000Z', NULL),
           ('m2', 'open', 'assistant',
            '{{\"type\":\"assistant\",\"session_id\":\"claude-id\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"done\"}}]}}}}',
            '2026-06-22T08:26:00.000Z', NULL),
           ('m3', 'open', 'assistant', '{{\"type\":\"result\"}}', '2026-06-22T08:27:00.000Z', NULL),
           ('m4', 'closed', 'user', 'hi', '2026-06-23T08:01:00.000Z', NULL);",
        ws = dir.display()
    ))
    .unwrap();
    path
}

#[test]
fn apply_adds_chats_tabs_and_sessions_once() {
    let dir = env::temp_dir().join(format!("swarm-conductor-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("swarm-home"), "").unwrap();
    let copy = Copy::of(&conductor_db(&dir)).unwrap();
    let conductor = Connection::open(&copy.db).unwrap();
    let mut store = swarm::store::open(Path::new(":memory:")).unwrap();
    let rebuilt = dir.join("cli");
    let planned = |store: &Connection| {
        plan(
            read_sessions(&conductor).unwrap(),
            &known(store).unwrap(),
            |_| None,
            |session, id| {
                rebuilt
                    .join(&session.agent_type)
                    .join(format!("{id}.jsonl"))
            },
        )
    };
    let first = planned(&store);
    assert_eq!(reasons(&first), [("empty", "no messages")]);
    apply(&dir, &conductor, &mut store, &first).unwrap();

    type Row = (Option<String>, String, Option<String>, Option<i64>);
    let rows: Vec<Row> = store
        .prepare("SELECT adapter, chair_provider, chair_id, archived_at FROM session ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let (open, closed) = (&first.chats[0], &first.chats[1]);
    assert_eq!(
        rows,
        [
            (None, "claude".into(), Some(open.id.clone()), None),
            (
                None,
                "codex".into(),
                Some(closed.id.clone()),
                Some(1_782_205_200)
            ),
        ]
    );
    let agents: i64 = store
        .query_row(
            "SELECT count(*) FROM agent WHERE pane_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(agents, 2);

    let log = std::fs::read_to_string(&open.log).unwrap();
    let lines: Vec<Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "the result row has no place in the log");
    assert_eq!(lines[0]["message"]["content"], "plan it");
    assert_eq!(lines[1]["sessionId"], open.id.as_str());
    assert_eq!(lines[1]["parentUuid"], "m1");
    let rollout = std::fs::read_to_string(&closed.log).unwrap();
    let meta: Value = serde_json::from_str(rollout.lines().next().unwrap()).unwrap();
    assert_eq!(meta["payload"]["id"], closed.id.as_str());
    let modified = std::fs::metadata(&open.log).unwrap().modified().unwrap();
    assert_eq!(
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        1_782_116_820,
        "the log keeps the time of its last message"
    );

    let saved = swarm::view::choices_store::load(&dir, true).unwrap();
    let ws = dir.display().to_string();
    assert_eq!(saved.owner.imported, [open.id.clone()].into());
    assert_eq!(saved.owner.tabs[&ws].open, std::slice::from_ref(&open.id));
    assert_eq!(saved.owner.chat_names[&open.id], "Plan sync");
    assert_eq!(saved.state.selected_chats[&ws], open.id);

    let second = planned(&store);
    assert!(second.chats.is_empty(), "a second import adds nothing");
    assert_eq!(
        reasons(&second),
        [
            ("open", "already in swarm"),
            ("closed", "already in swarm"),
            ("empty", "no messages")
        ]
    );

    // Each rebuilt file is a managed edit, which a revert removes only while it is unchanged.
    let entries = swarm::managed::list(&store, &[]).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|e| e.state == swarm::managed::State::Present
                && e.edit.writer == Writer::ImportConductor)
    );
    std::fs::write(&closed.log, "resumed since\n").unwrap();
    let target = swarm::managed::Target::All;
    let plans = swarm::managed::revert_plan(&store, &[], &target).unwrap();
    assert!(
        swarm::managed::conflicts_text(&plans).is_none(),
        "a changed file is not targeted"
    );
    swarm::managed::revert(&store, &plans).unwrap();
    assert!(!Path::new(&open.log).exists());
    assert_eq!(
        std::fs::read_to_string(&closed.log).unwrap(),
        "resumed since\n"
    );
    let ids = swarm::managed::Target::Ids(vec![entries[1].edit.id(), entries[0].edit.id()]);
    let plans = swarm::managed::revert_plan(&store, &[], &ids).unwrap();
    assert!(
        swarm::managed::conflicts_text(&plans).is_some(),
        "the resumed file is the CLI's"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
