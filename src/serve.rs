use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Query, Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, post},
};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    convert::Infallible,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::watch,
};
use tokio_stream::Stream;

const QUEUE_LIMIT: usize = 1024;
type ApiError = (StatusCode, Json<Value>);
type App = Arc<Server>;

fn error(status: StatusCode, message: &str) -> ApiError {
    (status, Json(json!({"error": message})))
}

struct Server {
    root: PathBuf,
    tool: PathBuf,
    port: u16,
    token: String,
    assets: Option<PathBuf>,
    dev_origin: Option<String>,
    hub: Mutex<Hub>,
    directories: Mutex<HashMap<String, (Option<String>, String)>>,
    stopped: AtomicBool,
}

struct Hub {
    version: u64,
    agents: Value,
    streams: HashMap<String, Client>,
    followers: HashMap<String, Follower>,
    retiring: HashMap<String, watch::Receiver<bool>>,
}

struct Client {
    queue: VecDeque<Frame>,
    waker: Option<Waker>,
    keys: HashMap<String, u64>,
}

#[derive(Clone)]
struct Frame {
    name: &'static str,
    data: String,
    key: Option<String>,
}

impl Frame {
    fn new(name: &'static str, data: Value, key: Option<&str>) -> Self {
        Self {
            name,
            data: data.to_string(),
            key: key.map(str::to_owned),
        }
    }
}

impl Client {
    fn push(&mut self, mut frame: Frame) {
        if frame.name == "snapshot" {
            self.queue.retain(|queued| queued.name != "snapshot");
        }
        if self.queue.len() == QUEUE_LIMIT {
            let keys: HashSet<_> = self
                .queue
                .iter()
                .filter_map(|queued| queued.key.clone())
                .collect();
            self.queue
                .retain(|queued| queued.key.is_none() && queued.name != "ping");
            for key in keys {
                self.queue
                    .push_back(Frame::new("reset", json!({"key": key}), Some(&key)));
            }
            if let Some(key) = frame.key.as_deref() {
                self.queue
                    .retain(|queued| queued.key.as_deref() != Some(key));
                frame = Frame::new("reset", json!({"key": key}), Some(key));
            }
        }
        self.queue.push_back(frame);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

struct Follower {
    generation: String,
    from: u64,
    stop: watch::Sender<bool>,
    done: watch::Receiver<bool>,
}

impl Hub {
    fn bump(&mut self) {
        self.version += 1;
        let frame = Frame::new("snapshot", json!({"version": self.version}), None);
        for stream in self.streams.values_mut() {
            stream.push(frame.clone());
        }
    }

    fn prune(&mut self) {
        self.retiring
            .retain(|_, done| !*done.borrow() && done.has_changed().is_ok());
        let unused: Vec<_> = self
            .followers
            .keys()
            .filter(|key| {
                !self
                    .streams
                    .values()
                    .any(|stream| stream.keys.contains_key(*key))
            })
            .cloned()
            .collect();
        for key in unused {
            let follower = self.followers.remove(&key).unwrap();
            let _ = follower.stop.send(true);
            self.retiring.insert(key, follower.done);
        }
    }
}

pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut port = 0;
    let mut assets = None;
    let mut dev_origin = None;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or("usage: swarm serve [--port N] [--assets DIR] [--dev-origin ORIGIN]")?;
        match flag.as_str() {
            "--port" => port = value.parse::<u16>()?,
            "--assets" => assets = Some(std::fs::canonicalize(value)?),
            "--dev-origin" => dev_origin = Some(value.clone()),
            _ => {
                return Err(
                    "usage: swarm serve [--port N] [--assets DIR] [--dev-origin ORIGIN]".into(),
                );
            }
        }
    }
    let root = swarm::paths::root_dir()?;
    drop(swarm::store::open(&root.join("swarm.db"))?);
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let tool = std::env::var_os("SWARM_TRANSCRIPT_TOOL")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/transcript/zig-out/bin/transcript")
        });
    let app = Arc::new(Server {
        root,
        tool,
        port,
        token,
        assets,
        dev_origin,
        hub: Mutex::new(Hub {
            version: 1,
            agents: json!({}),
            streams: HashMap::new(),
            followers: HashMap::new(),
            retiring: HashMap::new(),
        }),
        directories: Mutex::new(HashMap::new()),
        stopped: AtomicBool::new(false),
    });
    start_coordinator(&app);
    println!("http://127.0.0.1:{port}/#token={}", app.token);
    std::io::stdout().flush()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::from_std(listener)?;
        let ping_app = Arc::downgrade(&app);
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(15));
            timer.tick().await;
            loop {
                timer.tick().await;
                let Some(app) = ping_app.upgrade() else {
                    break;
                };
                for stream in app.hub.lock().unwrap().streams.values_mut() {
                    stream.push(Frame::new("ping", json!({}), None));
                }
            }
        });
        axum::serve(listener, router(app.clone())).await
    })?;
    app.stopped.store(true, Ordering::Relaxed);
    Ok(())
}

fn router(app: App) -> Router {
    Router::new()
        .route("/api/snapshot", get(snapshot))
        .route("/api/transcript", get(transcript))
        .route("/api/stream", get(stream))
        .route("/api/subscribe", post(subscribe))
        .fallback(static_file)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}

async fn guard(State(app): State<App>, request: Request, next: Next) -> Response {
    if !request.uri().path().starts_with("/api/") {
        return next.run(request).await;
    }
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let expected = format!("Bearer {}", app.token);
    if authorization != Some(expected.as_str()) {
        let mut response = error(StatusCode::UNAUTHORIZED, "bearer token required").into_response();
        let challenge = if authorization.is_none() {
            "Bearer realm=\"swarm\""
        } else {
            "Bearer realm=\"swarm\", error=\"invalid_token\""
        };
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static(challenge),
        );
        return response;
    }
    let hosts = [
        format!("127.0.0.1:{}", app.port),
        format!("localhost:{}", app.port),
    ];
    if !request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| hosts.iter().any(|allowed| allowed == host))
    {
        return error(StatusCode::FORBIDDEN, "host refused").into_response();
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let valid = origin.to_str().ok().is_some_and(|origin| {
            hosts.iter().any(|host| origin == format!("http://{host}"))
                || app.dev_origin.as_deref() == Some(origin)
        });
        if !valid {
            return error(StatusCode::FORBIDDEN, "origin refused").into_response();
        }
    }
    if request.method() == Method::POST
        && !request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
            })
    {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "JSON content type required",
        )
        .into_response();
    }
    next.run(request).await
}

fn read_connection(root: &Path) -> rusqlite::Result<rusqlite::Connection> {
    let connection = rusqlite::Connection::open_with_flags(
        root.join("swarm.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    connection.busy_timeout(Duration::from_secs(5))?;
    Ok(connection)
}

async fn snapshot(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        snapshot_value(&app).map(Json).map_err(|err| {
            eprintln!("swarm serve: {err}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "snapshot failed")
        })
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "snapshot task failed"))?
}

fn snapshot_value(app: &Server) -> Result<Value, Box<dyn std::error::Error>> {
    let connection = read_connection(&app.root)?;
    let mut sessions = Vec::new();
    for row in swarm::store::sessions(&connection)? {
        let cwd = row.cwd.clone();
        let session = swarm::bus::Session {
            id: row.id,
            talk_mode: row.talk_mode,
            adapter: row.adapter,
            cwd: row.cwd,
            created_at: row.created_at,
            chair_provider: row.chair_provider,
            chair_id: row.chair_id,
            chair_log: row.chair_log,
            continuation_of: row.continuation_of,
            agents: row.agents,
            messages: row.messages,
            last_message_at: row.last_message_at,
            archived_at_s: row.archived_at,
        };
        let mut session = serde_json::to_value(session)?;
        let mut directories = app.directories.lock().unwrap();
        let (project, workspace) = directories.entry(cwd.clone()).or_insert_with(|| {
            let git = |flag: &str| {
                std::process::Command::new("git")
                    .args(["-C", &cwd, "rev-parse", "--path-format=absolute", flag])
                    .output()
                    .ok()
                    .filter(|output| output.status.success())
                    .and_then(|output| String::from_utf8(output.stdout).ok())
                    .map(|text| text.trim().to_owned())
                    .filter(|text| !text.is_empty())
            };
            let project = git("--git-common-dir").and_then(|path| {
                Path::new(&path)
                    .parent()
                    .map(|parent| parent.to_string_lossy().into_owned())
            });
            (project, git("--show-toplevel").unwrap_or(cwd))
        });
        session["project"] = json!(project);
        session["workspace"] = json!(workspace);
        sessions.push(session);
    }
    let hub = app.hub.lock().unwrap();
    Ok(json!({"version": hub.version, "sessions": sessions, "agents": hub.agents}))
}

fn start_coordinator(app: &App) {
    let weak = Arc::downgrade(app);
    std::thread::spawn(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let connection = swarm::store::open(&app.root.join("swarm.db"));
        let Ok(mut connection) = connection else {
            eprintln!("swarm serve: coordinator database open failed");
            return;
        };
        drop(app);
        loop {
            let Some(app) = weak.upgrade() else {
                break;
            };
            if app.stopped.load(Ordering::Relaxed) {
                break;
            }
            let started = std::time::Instant::now();
            match super::all_agent_listings(&mut connection, &app.root, Duration::from_millis(900))
            {
                Ok(listings) => {
                    let mut agents = BTreeMap::new();
                    // Sessions without a pane adapter still expose their stored agents.
                    if let Ok(sessions) = swarm::store::sessions(&connection) {
                        for session in sessions {
                            let rows = listings
                                .get(&session.id)
                                .map(|listing| {
                                    serde_json::to_value(&listing.agents)
                                        .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)
                                })
                                .unwrap_or_else(|| stored_agents(&connection, &session.id));
                            if let Ok(rows) = rows {
                                agents.insert(session.id, rows);
                            }
                        }
                    }
                    let value = json!(agents);
                    let mut hub = app.hub.lock().unwrap();
                    if hub.agents != value {
                        hub.agents = value;
                        hub.bump();
                    }
                }
                Err(err) => eprintln!("swarm serve: coordinator: {err}"),
            }
            // Notices stay unposted: `take_notices` marks rows posted, and only the native shell
            // that shows them may take them.
            drop(app);
            std::thread::sleep(Duration::from_secs(1).saturating_sub(started.elapsed()));
        }
    });
    let weak = Arc::downgrade(app);
    std::thread::spawn(move || {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let Ok(connection) = read_connection(&app.root) else {
            eprintln!("swarm serve: version database open failed");
            return;
        };
        drop(app);
        let mut last = None;
        loop {
            let Some(app) = weak.upgrade() else {
                break;
            };
            if app.stopped.load(Ordering::Relaxed) {
                break;
            }
            match connection.query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0)) {
                Ok(version) => {
                    if last.is_some_and(|old| old != version) {
                        app.hub.lock().unwrap().bump();
                    }
                    last = Some(version);
                }
                Err(err) => eprintln!("swarm serve: data_version: {err}"),
            }
            drop(app);
            std::thread::sleep(Duration::from_millis(200));
        }
    });
}

fn stored_agents(
    connection: &rusqlite::Connection,
    session: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let rows = swarm::store::agents(connection, session)?;
    let agents: Vec<_> = rows
        .into_iter()
        .map(|row| swarm::bus::Agent {
            id: row.id,
            role: row.role,
            pane: row.pane,
            provider: row.provider,
            created_at: row.created_at,
            alive: None,
            state: row.state,
            state_at_s: row.state_at,
            state_source: row.state_source,
            state_detail: row.state_detail,
            state_kind: row.state_kind,
            log: row.log,
            prompt: None,
            profile: row.profile,
            runner: row.runner,
            model: row.model,
            effort: row.effort,
            account: row.account,
            cost_usd: row.cost_usd,
            tokens: row.tokens,
        })
        .collect();
    Ok(serde_json::to_value(agents)?)
}

#[derive(Deserialize)]
struct TranscriptQuery {
    session: String,
    agent: String,
    tail: u16,
    before: Option<u64>,
}

struct Log {
    path: PathBuf,
    format: String,
}

fn resolve_log(root: &Path, session: &str, agent: &str) -> Result<Log, ApiError> {
    let connection = read_connection(root)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?;
    let row = connection
        .query_row(
            "SELECT chair_log, chair_provider FROM session WHERE id = ?1",
            [session],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "session read failed"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown session"))?;
    let (path, format) = if agent == "chair" {
        row
    } else {
        let row = swarm::store::agents(&connection, session)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "agent read failed"))?
            .into_iter()
            .find(|row| row.id == agent)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown agent"))?;
        (row.log, row.provider)
    };
    let path = path
        .filter(|path| !path.is_empty())
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "no transcript log"))?;
    let format = format
        .filter(|format| matches!(format.as_str(), "claude" | "codex" | "agy"))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "no transcript format"))?;
    if !Path::new(&path).is_file() {
        return Err(error(StatusCode::NOT_FOUND, "transcript log unavailable"));
    }
    Ok(Log {
        path: path.into(),
        format,
    })
}

async fn transcript(
    State(app): State<App>,
    Query(query): Query<TranscriptQuery>,
) -> Result<Json<Value>, ApiError> {
    if !(1..=2000).contains(&query.tail) {
        return Err(error(StatusCode::BAD_REQUEST, "tail must be 1..2000"));
    }
    tokio::task::spawn_blocking(move || {
        let log = resolve_log(&app.root, &query.session, &query.agent)?;
        let mut command = std::process::Command::new(&app.tool);
        command.args(["--format", &log.format, "--tail", &query.tail.to_string()]);
        if let Some(before) = query.before {
            command.args(["--before", &before.to_string()]);
        }
        let output = command.arg(log.path).output().map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transcript tool unavailable",
            )
        })?;
        if !output.status.success() {
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transcript tool failed",
            ));
        }
        let mut events = Vec::new();
        let mut page = None;
        for line in output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let mut value: Value = serde_json::from_slice(line).map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "invalid transcript output",
                )
            })?;
            if value["type"] == "page" {
                value.as_object_mut().unwrap().remove("type");
                page = Some(value);
            } else {
                events.push(value);
            }
        }
        let page = page
            .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript page missing"))?;
        Ok(Json(json!({"page": page, "events": events})))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript task failed"))?
}

struct Events {
    app: App,
    id: String,
}

impl Stream for Events {
    type Item = Result<Event, Infallible>;
    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut hub = self.app.hub.lock().unwrap();
        let Some(client) = hub.streams.get_mut(&self.id) else {
            return Poll::Ready(None);
        };
        if let Some(frame) = client.queue.pop_front() {
            Poll::Ready(Some(Ok(Event::default()
                .event(frame.name)
                .data(frame.data))))
        } else {
            client.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        let mut hub = self.app.hub.lock().unwrap();
        hub.streams.remove(&self.id);
        hub.prune();
    }
}

async fn stream(State(app): State<App>) -> Sse<Events> {
    let id = uuid::Uuid::now_v7().to_string();
    let mut client = Client {
        queue: VecDeque::new(),
        waker: None,
        keys: HashMap::new(),
    };
    client.push(Frame::new("hello", json!({"stream": id}), None));
    app.hub.lock().unwrap().streams.insert(id.clone(), client);
    Sse::new(Events { app, id })
}

#[derive(Deserialize)]
struct Subscription {
    stream: String,
    keys: Vec<SubscriptionKey>,
}
#[derive(Deserialize)]
struct SubscriptionKey {
    key: String,
    from: u64,
}

async fn subscribe(
    State(app): State<App>,
    Json(body): Json<Subscription>,
) -> Result<Json<Value>, ApiError> {
    if body.keys.len() > 32 {
        return Err(error(StatusCode::BAD_REQUEST, "at most 32 keys"));
    }
    let root = app.root.clone();
    let keys = tokio::task::spawn_blocking(move || {
        let mut unique = HashSet::new();
        let mut keys = Vec::new();
        for mut entry in body.keys {
            let (session, agent) = entry
                .key
                .split_once(':')
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "invalid transcript key"))?;
            if !unique.insert(entry.key.clone()) {
                return Err(error(StatusCode::BAD_REQUEST, "duplicate transcript key"));
            }
            let log = resolve_log(&root, session, agent)?;
            entry.from = entry.from.min(
                std::fs::metadata(&log.path)
                    .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?
                    .len(),
            );
            keys.push((entry, log));
        }
        Ok::<_, ApiError>((body.stream, keys))
    })
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "subscription task failed",
        )
    })??;
    let (id, keys) = keys;
    let mut launch = Vec::new();
    let mut wait = Vec::new();
    {
        let mut hub = app.hub.lock().unwrap();
        let client = hub
            .streams
            .get_mut(&id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown stream"))?;
        client.keys = keys
            .iter()
            .map(|(entry, _)| (entry.key.clone(), entry.from))
            .collect();
        client.queue.retain(|frame| frame.key.is_none());
        hub.prune();
        for (entry, log) in keys {
            if let Some(done) = hub.retiring.remove(&entry.key) {
                wait.push(done);
            }
            if hub
                .followers
                .get(&entry.key)
                .is_some_and(|follower| follower.from <= entry.from)
            {
                continue;
            }
            if let Some(old) = hub.followers.remove(&entry.key) {
                let _ = old.stop.send(true);
                wait.push(old.done);
            }
            let (stop, receiver) = watch::channel(false);
            let (done_sender, done) = watch::channel(false);
            let generation = uuid::Uuid::now_v7().to_string();
            hub.followers.insert(
                entry.key.clone(),
                Follower {
                    generation: generation.clone(),
                    from: entry.from,
                    stop,
                    done,
                },
            );
            launch.push((entry, log, generation, receiver, done_sender));
        }
    }
    for mut done in wait {
        let _ = done.wait_for(|finished| *finished).await;
    }
    for (entry, log, generation, stop, done) in launch {
        tokio::spawn(follow(
            Arc::downgrade(&app),
            app.tool.clone(),
            entry,
            log,
            generation,
            stop,
            done,
        ));
    }
    Ok(Json(json!({})))
}

fn flush_transcript(
    app: &Weak<Server>,
    key: &str,
    generation: &str,
    batches: &mut Vec<(u64, Vec<Value>)>,
) {
    if batches.is_empty() {
        return;
    }
    if let Some(app) = app.upgrade() {
        let mut hub = app.hub.lock().unwrap();
        if hub
            .followers
            .get(key)
            .is_none_or(|follower| follower.generation != generation)
        {
            batches.clear();
            return;
        }
        for client in hub.streams.values_mut() {
            let Some(cursor) = client.keys.get_mut(key) else {
                continue;
            };
            let mut events = Vec::new();
            let mut end_offset = *cursor;
            for (end, batch) in batches.iter() {
                if *end > end_offset {
                    events.extend(batch.iter().cloned());
                    end_offset = *end;
                }
            }
            if end_offset != *cursor {
                *cursor = end_offset;
                client.push(Frame::new(
                    "transcript",
                    json!({"key": key, "events": events, "end_offset": end_offset}),
                    Some(key),
                ));
            }
        }
        if let Some(follower) = hub.followers.get_mut(key) {
            follower.from = batches.last().unwrap().0;
        }
    }
    batches.clear();
}

async fn follow(
    app: Weak<Server>,
    tool: PathBuf,
    entry: SubscriptionKey,
    log: Log,
    generation: String,
    mut stop: watch::Receiver<bool>,
    done: watch::Sender<bool>,
) {
    let result = async {
        if *stop.borrow() { return Ok(()); }
        let mut child = tokio::process::Command::new(tool)
            .args(["--follow", "--from", &entry.from.to_string(), "--format", &log.format])
            .arg(log.path).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).kill_on_drop(true).spawn()?;
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut events = Vec::new();
        let mut batches = Vec::new();
        let mut first = true;
        let mut timer = tokio::time::interval(Duration::from_millis(16));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let result = async { loop {
            tokio::select! {
                biased;
                _ = stop.changed() => break Ok::<_, std::io::Error>(()),
                _ = timer.tick() => {
                    flush_transcript(&app, &entry.key, &generation, &mut batches);
                    timer.reset();
                }
                line = lines.next_line() => {
                    let Some(line) = line? else { break Err(std::io::Error::other("transcript follower ended")); };
                    let value: Value = serde_json::from_str(&line)?;
                    if value["type"] == "page" {
                        if first { first = false; continue; }
                        let end = value["end_offset"].as_u64().ok_or_else(|| std::io::Error::other("transcript offset missing"))?;
                        batches.push((end, std::mem::take(&mut events)));
                    } else { events.push(value); }
                }
            }
        } }.await;
        let killed = child.kill().await;
        result.and(killed)
    }.await;
    if let Err(err) = result {
        eprintln!("swarm serve: follower {}: {err}", entry.key);
        if let Some(app) = app.upgrade() {
            let mut hub = app.hub.lock().unwrap();
            if hub
                .followers
                .get(&entry.key)
                .is_some_and(|follower| follower.generation == generation)
            {
                hub.followers.remove(&entry.key);
                for client in hub
                    .streams
                    .values_mut()
                    .filter(|client| client.keys.contains_key(&entry.key))
                {
                    client.push(Frame::new(
                        "reset",
                        json!({"key": entry.key}),
                        Some(&entry.key),
                    ));
                }
            }
        }
    }
    let _ = done.send(true);
}

async fn static_file(State(app): State<App>, request: Request) -> Response {
    if request.uri().path().starts_with("/api/") {
        return error(StatusCode::NOT_FOUND, "unknown route").into_response();
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let Some(assets) = &app.assets else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = request.uri().path();
    let relative = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    if relative.contains('%')
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = assets.join(relative);
    let Ok(path) = tokio::fs::canonicalize(path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !path.starts_with(assets) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(metadata) = tokio::fs::metadata(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !metadata.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let content_type = match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let length = bytes.len();
    let mut response = Response::new(if request.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from(bytes)
    });
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(keys: &[(&str, u64)]) -> Client {
        Client {
            queue: VecDeque::new(),
            waker: None,
            keys: keys
                .iter()
                .map(|(key, from)| (key.to_string(), *from))
                .collect(),
        }
    }

    struct Fixture {
        app: App,
        dir: PathBuf,
        session: String,
        log: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tmp/web-pilot")
                .join(format!("serve-unit-{}", uuid::Uuid::now_v7()));
            let root = dir.join("home/.swarm");
            std::fs::create_dir_all(&root).unwrap();
            let log = dir.join("chair.jsonl");
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("packages/transcript/src/fixtures/claude-chair-run.jsonl"),
                &log,
            )
            .unwrap();
            let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
            let session = swarm::store::create_session(
                &connection,
                "lane",
                Path::new(env!("CARGO_MANIFEST_DIR")),
                Some(("claude", "synthetic")),
                None,
            )
            .unwrap();
            swarm::store::set_chair_log(&connection, &session, &log).unwrap();
            swarm::store::add_agent(&connection, &session, "worker", "chat").unwrap();
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
                }),
                directories: Mutex::new(HashMap::new()),
                stopped: AtomicBool::new(false),
            });
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

    #[test]
    fn queue_overflow_resets_every_affected_key_and_stays_bounded() {
        let mut stream = client(&[("s:one", 0), ("s:two", 0)]);
        for index in 0..QUEUE_LIMIT {
            let key = if index % 2 == 0 { "s:one" } else { "s:two" };
            stream.push(Frame::new("transcript", json!({"index": index}), Some(key)));
        }
        stream.push(Frame::new("transcript", json!({}), Some("s:three")));
        assert!(stream.queue.len() <= QUEUE_LIMIT);
        assert_eq!(stream.queue.len(), 3);
        for key in ["s:one", "s:two", "s:three"] {
            assert!(
                stream
                    .queue
                    .iter()
                    .any(|frame| frame.name == "reset" && frame.key.as_deref() == Some(key))
            );
        }
        for version in 0..2000 {
            stream.push(Frame::new("snapshot", json!({"version": version}), None));
        }
        assert_eq!(stream.queue.len(), 4);
        assert_eq!(
            serde_json::from_str::<Value>(&stream.queue.back().unwrap().data).unwrap()["version"],
            1999
        );
    }

    #[tokio::test]
    async fn transcript_and_snapshot_read_real_synthetic_store_without_marking_notices() {
        let fixture = Fixture::new();
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        swarm::store::add_notice(
            &connection,
            Some(&fixture.session),
            None,
            "synthetic",
            "notice",
        )
        .unwrap();
        let Json(snapshot) = snapshot(State(fixture.app.clone())).await.unwrap();
        assert_eq!(snapshot["sessions"][0]["id"], fixture.session);
        assert!(snapshot["sessions"][0]["project"].is_string());
        assert_eq!(
            snapshot["sessions"][0]["workspace"],
            env!("CARGO_MANIFEST_DIR")
        );
        let unmarked: i64 = connection
            .query_row(
                "SELECT count(*) FROM notice WHERE posted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unmarked, 1);
        let Json(page) = transcript(
            State(fixture.app.clone()),
            Query(TranscriptQuery {
                session: fixture.session.clone(),
                agent: "chair".into(),
                tail: 2000,
                before: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(page["page"]["start_offset"], 0);
        assert_eq!(
            page["page"]["end_offset"],
            std::fs::metadata(&fixture.log).unwrap().len()
        );
        assert!(page["events"].as_array().unwrap().len() > 5);
        assert_eq!(
            resolve_log(&fixture.app.root, &fixture.session, "missing")
                .err()
                .unwrap()
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            resolve_log(&fixture.app.root, &fixture.session, "worker")
                .err()
                .unwrap()
                .0,
            StatusCode::NOT_FOUND
        );
        connection
            .execute(
                "UPDATE session SET archived_at = unixepoch() WHERE id = ?1",
                [&fixture.session],
            )
            .unwrap();
        assert!(resolve_log(&fixture.app.root, &fixture.session, "chair").is_ok());
    }

    async fn until_cursor(app: &App, id: &str, key: &str, end: u64) {
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

    #[tokio::test]
    async fn shared_followers_replay_late_subscribers_and_stop_after_the_last_reference() {
        let fixture = Fixture::new();
        let key = format!("{}:chair", fixture.session);
        fixture
            .app
            .hub
            .lock()
            .unwrap()
            .streams
            .insert("one".into(), client(&[]));
        let _ = subscribe(
            State(fixture.app.clone()),
            Json(Subscription {
                stream: "one".into(),
                keys: vec![SubscriptionKey {
                    key: key.clone(),
                    from: 0,
                }],
            }),
        )
        .await
        .unwrap();
        let size = std::fs::metadata(&fixture.log).unwrap().len();
        until_cursor(&fixture.app, "one", &key, size).await;
        let generation = {
            let mut hub = fixture.app.hub.lock().unwrap();
            hub.streams.get_mut("one").unwrap().queue.clear();
            hub.streams.insert("two".into(), client(&[]));
            hub.followers[&key].generation.clone()
        };
        let _ = subscribe(
            State(fixture.app.clone()),
            Json(Subscription {
                stream: "two".into(),
                keys: vec![SubscriptionKey {
                    key: key.clone(),
                    from: 0,
                }],
            }),
        )
        .await
        .unwrap();
        until_cursor(&fixture.app, "two", &key, size).await;
        {
            let mut hub = fixture.app.hub.lock().unwrap();
            assert_eq!(hub.followers.len(), 1);
            assert_ne!(hub.followers[&key].generation, generation);
            assert!(hub.streams["one"].queue.is_empty());
            assert!(
                hub.streams["two"]
                    .queue
                    .iter()
                    .any(|frame| frame.name == "transcript")
            );
            hub.streams.get_mut("two").unwrap().queue.clear();
        }
        let line = "{\"type\":\"user\",\"uuid\":\"synthetic-live\",\"message\":{\"content\":\"live boundary\"}}\n";
        let mut log = std::fs::OpenOptions::new()
            .append(true)
            .open(&fixture.log)
            .unwrap();
        log.write_all(&line.as_bytes()[..line.len() - 1]).unwrap();
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(
            fixture.app.hub.lock().unwrap().streams["one"]
                .queue
                .is_empty()
        );
        let started = std::time::Instant::now();
        log.write_all(b"\n").unwrap();
        until_cursor(&fixture.app, "one", &key, size + line.len() as u64).await;
        until_cursor(&fixture.app, "two", &key, size + line.len() as u64).await;
        assert!(started.elapsed() < Duration::from_secs(1));
        {
            let hub = fixture.app.hub.lock().unwrap();
            let first =
                serde_json::from_str::<Value>(&hub.streams["one"].queue.back().unwrap().data)
                    .unwrap();
            let second =
                serde_json::from_str::<Value>(&hub.streams["two"].queue.back().unwrap().data)
                    .unwrap();
            assert_eq!(first, second);
            assert_eq!(first["events"][0]["text"], "live boundary");
            assert_eq!(first["end_offset"], size + line.len() as u64);
        }
        let mut done = fixture.app.hub.lock().unwrap().followers[&key].done.clone();
        drop(Events {
            app: fixture.app.clone(),
            id: "one".into(),
        });
        assert_eq!(fixture.app.hub.lock().unwrap().followers.len(), 1);
        drop(Events {
            app: fixture.app.clone(),
            id: "two".into(),
        });
        tokio::time::timeout(Duration::from_secs(1), done.wait_for(|value| *value))
            .await
            .unwrap()
            .unwrap();
        assert!(fixture.app.hub.lock().unwrap().followers.is_empty());
    }
}
