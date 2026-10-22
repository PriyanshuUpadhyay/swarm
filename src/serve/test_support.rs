//! Shared server test support.

use super::*;

pub(super) fn client(keys: &[(&str, u64)]) -> Client {
    Client {
        queue: VecDeque::new(),
        waker: None,
        row_keys: HashSet::new(),
        keys: keys
            .iter()
            .map(|(key, from)| (key.to_string(), *from))
            .collect(),
    }
}

pub(super) struct Fixture {
    pub(super) app: App,
    pub(super) dir: PathBuf,
    pub(super) session: String,
    pub(super) log: PathBuf,
}

impl Fixture {
    /// A new chat in `src`, a folder of the workspace that the owner archived (the crate root).
    pub(super) fn chat_in_archived_workspace(&self) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let chat = swarm::store::create_session(
            &swarm::store::open(&self.app.root.join("swarm.db")).unwrap(),
            "lane",
            &root.join("src"),
            Some(("claude", "inner")),
            Some("tmux-swarm"),
        )
        .unwrap();
        choices_store::update(&self.app.root, None, |owner, _| {
            owner.archived.insert(root.to_string_lossy().into_owned());
            Ok(())
        })
        .unwrap();
        chat
    }
    pub(super) fn new() -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tmp/web-pilot")
            .join(format!("serve-unit-{}", uuid::Uuid::now_v7()));
        Self::at(dir)
    }
    pub(super) fn at(dir: PathBuf) -> Self {
        let root = dir.join("home/.swarm");
        std::fs::create_dir_all(&root).unwrap();
        let log = dir.join("chair.jsonl");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("packages/transcript/src/fixtures/claude-chair-run.jsonl"),
            &log,
        )
        .unwrap();
        std::fs::write(root.join("swarm-home"), "swarm\n").unwrap();
        let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let session = swarm::store::create_session(
            &connection,
            "lane",
            Path::new(env!("CARGO_MANIFEST_DIR")),
            Some(("claude", "synthetic")),
            Some("tmux-swarm"),
        )
        .unwrap();
        swarm::store::set_chair_log(&connection, &session, &log).unwrap();
        swarm::store::add_agent(&connection, &session, "worker", "chat").unwrap();
        let (actions, action_queue) = mpsc::channel(64);
        let app = Arc::new(Server {
            root,
            tool: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("packages/transcript/zig-out/bin/transcript"),
            port: 1,
            token: "test".into(),
            assets: None,
            dev_origin: None,
            hub: Mutex::new(Hub {
                version: 1,
                agents: json!({}),
                streams: HashMap::new(),
                followers: HashMap::new(),
                retiring: HashMap::new(),
                row_cache: VecDeque::new(),
            }),
            directories: Mutex::new(HashMap::new()),
            stopped: AtomicBool::new(false),
            cli: dir.join("action-cli"),
            actions,
            tree_cache: Mutex::new(TreeCache::default()),
            handoffs: Mutex::new(HashMap::new()),
            chat_create: tokio::sync::Mutex::new(()),
            usage_refresh: Mutex::new(HashMap::new()),
            commands: Mutex::new(HashMap::new()),
        });
        // Most row tests need no action worker and may not run inside a Tokio runtime.
        drop(action_queue);
        Self {
            app,
            dir,
            session,
            log,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.app.stopped.store(true, Ordering::Relaxed);
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

pub(super) fn action_fixture() -> Fixture {
    action_fixture_at(Fixture::new())
}

pub(super) fn action_fixture_at(mut fixture: Fixture) -> Fixture {
    use std::os::unix::fs::PermissionsExt;
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET adapter = 'tmux-swarm' WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    swarm::store::add_agent(&connection, &fixture.session, "orchestrator", "chat").unwrap();
    let cli = &fixture.app.cli;
    let new_id = uuid::Uuid::now_v7();
    std::fs::write(cli, format!(r#"#!/bin/sh
printf '%s\n' "$SWARM_ADAPTER|${{SWARM_SESSION_ID-}}|${{SWARM_AGENT_ID-}}|$PWD|$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
  'session new') printf '%s\n' '{new_id}' ;;
  'launch orchestrator') printf 'fixture-pane\n' ;;
  'sessions --json') printf '%s\n' '{{"sessions":[{{"id":"archived","archivedAt":42}},{{"id":"active"}}]}}' ;;
  'doctor --json') printf '%s\n' '{{"schema":"swarm.doctor.v1","items":[]}}' ;;
  'setup '*) case "$3 $4" in
    '--plan --json') printf '%s\n' '{{"digest":"ab12","files":[],"conflicts":[],"skipped":[]}}' ;;
    '--digest dead') echo 'swarm: a file changed after the plan; check the plan again' >&2; exit 1 ;;
    '--digest beef') echo 'swarm: cannot write ~/.codex/config.toml' >&2; exit 1 ;;
    '--digest f00d') echo 'swarm: ~/.claude/skills/x changed after the link plan; swarm leaves it' >&2; exit 1 ;;
    '--digest cafe') echo 'swarm: ~/.codex/config.toml changed while swarm edited it; run the command again' >&2; exit 1 ;;
  esac ;;
  'import conductor') case "$3 $4" in
    '--json ') printf '%s\n' '{{"digest":"ab12","chats":[],"skipped":[]}}' ;;
    '--digest dead') echo 'swarm: Conductor or swarm changed after the plan; check the plan again' >&2; exit 1 ;;
    '--digest beef') echo 'swarm: no Conductor database at /x' >&2; exit 1 ;;
  esac ;;
  *) if [ "$1" = type ]; then cat > "$SWARM_HOME/text"; fi ;;
esac
"#)).unwrap();
    std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (actions, queue) = mpsc::channel(64);
    Arc::get_mut(&mut fixture.app).unwrap().actions = actions;
    start_action_queue(&fixture.app, queue);
    fixture
}

pub(super) async fn call_action(
    fixture: &Fixture,
    route: &str,
    body: Value,
) -> Result<Json<Value>, ApiError> {
    chat_action(
        State(fixture.app.clone()),
        route.parse().unwrap(),
        Ok(Json(body)),
    )
    .await
}

pub(super) fn action_calls(fixture: &Fixture) -> String {
    std::fs::read_to_string(fixture.app.root.parent().unwrap().join("actions")).unwrap_or_default()
}

pub(super) async fn until_cursor(app: &App, id: &str, key: &str, end: u64) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if app.hub.lock().unwrap().streams[id].keys[key] == end {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
pub(super) async fn row_page(
    fixture: &Fixture,
    tail: u16,
    before: Option<u64>,
) -> Result<Value, ApiError> {
    transcript_rows(
        State(fixture.app.clone()),
        Query(TranscriptQuery {
            session: fixture.session.clone(),
            agent: "chair".into(),
            tail,
            before,
        }),
    )
    .await
    .map(|Json(value)| value)
}
pub(super) fn codex_line(number: usize) -> String {
    json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":format!("message-{number}")}]}}).to_string() + "\n"
}
pub(super) fn codex_log(fixture: &Fixture, count: usize) {
    std::fs::write(&fixture.log, (0..count).map(codex_line).collect::<String>()).unwrap();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET chair_provider = 'codex' WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
}
pub(super) fn row_texts(page: &Value) -> Vec<String> {
    page["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "user")
        .map(|row| row["text"].as_str().unwrap().into())
        .collect()
}
pub(super) fn synthetic_event(number: usize, tools_only: bool) -> Value {
    if tools_only || number % 4 == 2 {
        json!({"type":"tool_call","tool_call_id":format!("tool-{number}"),"name":"Bash","input":{"command":"cargo test"},"status":"in_progress","meta":{"uuid":format!("call-{number}"),"timestamp":"2026-10-13T00:00:00Z"}})
    } else if number % 4 == 3 {
        json!({"type":"tool_call_update","tool_call_id":format!("tool-{}",number-1),"status":"completed","content":"test passed","meta":{"timestamp":"2026-10-13T00:00:01Z"}})
    } else {
        json!({"type":if number.is_multiple_of(4) {"user_message_chunk"} else {"agent_message_chunk"},"text":format!("message-{number}"),"meta":{"uuid":format!("message-{number}")}})
    }
}
pub(super) fn reference_delta(state: &mut LiveRows, key: &str) -> Value {
    let rows = state.rows_full();
    let previous: HashMap<_, _> = state.last_rows.iter().map(|row| (&row.id, row)).collect();
    let upsert: Vec<_> = rows
        .iter()
        .filter(|row| previous.get(&row.id).is_none_or(|old| *old != *row))
        .collect();
    let current: HashSet<_> = rows.iter().map(|row| &row.id).collect();
    let remove: Vec<_> = state
        .last_rows
        .iter()
        .filter(|row| !current.contains(&row.id))
        .map(|row| &row.id)
        .collect();
    let usage = view::usage(&state.records.values().cloned().collect::<Vec<_>>());
    let mut payload = json!({"key":key,"upsert":upsert,"remove":remove,"end_offset":state.end});
    if usage != state.last_usage {
        payload["usage"] = json!(usage);
    }
    state.last_usage = usage;
    state.remember_shown(&rows);
    state.last_rows = rows;
    payload
}
pub(super) fn queue_log(operation: &str, text: &str) -> String {
    json!({"type":"queue-operation","operation":operation,"content":text}).to_string() + "\n"
}
pub(super) fn pull_back_fixture(records: &str, after: &str) -> Fixture {
    let fixture = action_fixture();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET adapter = 'tmux-swarm' WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    std::fs::write(&fixture.log, records).unwrap();
    std::fs::write(fixture.app.root.parent().unwrap().join("after"), after).unwrap();
    std::fs::write(
        &fixture.app.cli,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$SWARM_HOME/actions"
if [ "$3" = Up ]; then cat "$SWARM_HOME/after" >> '{}'; fi
"#,
            fixture.log.display()
        ),
    )
    .unwrap();
    fixture
}
pub(super) async fn life_post(
    fixture: &Fixture,
    route: &str,
    body: Value,
) -> Result<Value, ApiError> {
    lifecycle_write(
        State(fixture.app.clone()),
        route.parse().unwrap(),
        Ok(Json(body)),
    )
    .await
    .map(|v| v.0)
}
