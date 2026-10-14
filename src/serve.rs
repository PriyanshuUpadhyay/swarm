use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Query, Request, State, rejection::JsonRejection},
    http::{HeaderValue, Method, StatusCode, Uri, header},
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
use swarm::view::transcript::{self as view, Record, Row};
use swarm::view::{
    changes, choices, choices_store, claude_text, discovery, fields, handoff, lifecycle,
    navigation, queue, tabs, tree, workspace_panel,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{mpsc, oneshot, watch},
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
    cli: PathBuf,
    actions: mpsc::Sender<ActionJob>,
    tree_cache: Mutex<TreeCache>,
    handoffs: Mutex<HashMap<String, HandoffRun>>,
    chat_create: tokio::sync::Mutex<()>,
    // shortcut: the cooldown resets on server restart; seed from cache if restart bursts matter.
    usage_refresh: Mutex<HashMap<String, (tokio::time::Instant, Option<ApiError>)>>,
}

#[derive(Default)]
struct TreeCache {
    discovery: discovery::Discovery,
    snapshot: Value,
    revision: String,
    saved: Option<choices_store::Saved>,
    choices_error: Option<String>,
}

struct Hub {
    version: u64,
    agents: Value,
    streams: HashMap<String, Client>,
    followers: HashMap<String, Follower>,
    retiring: HashMap<String, watch::Receiver<bool>>,
    row_cache: VecDeque<(String, LiveRows)>,
}

struct Client {
    queue: VecDeque<Frame>,
    waker: Option<Waker>,
    keys: HashMap<String, u64>,
    row_keys: HashSet<String>,
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
    rows: Option<LiveRows>,
    rendering_rows: bool,
}

impl Hub {
    fn cache_rows(&mut self, key: String, rows: LiveRows) {
        self.row_cache.retain(|(cached, _)| cached != &key);
        self.row_cache.push_back((key, rows));
        while self.row_cache.len() > 4 {
            self.row_cache.pop_front();
        }
    }
    fn bump(&mut self) {
        self.version += 1;
        let frame = Frame::new("snapshot", json!({"version": self.version}), None);
        self.publish(frame);
    }
    fn publish(&mut self, frame: Frame) {
        for stream in self.streams.values_mut() {
            stream.push(frame.clone());
        }
    }

    fn prune(&mut self) {
        let rowless: Vec<_> = self
            .followers
            .keys()
            .filter(|key| {
                !self
                    .streams
                    .values()
                    .any(|stream| stream.row_keys.contains(*key))
            })
            .cloned()
            .collect();
        for key in rowless {
            if let Some(rows) = self
                .followers
                .get_mut(&key)
                .and_then(|follower| follower.rows.take())
            {
                self.cache_rows(key, rows);
            }
        }
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
            if let Some(rows) = follower.rows {
                self.cache_rows(key.clone(), rows);
            }
            self.retiring.insert(key, follower.done);
        }
    }
}

/// `web/dist`, embedded by `build.rs` when built with the serve feature (ADR 0079). Empty in a dev
/// build that has no `web/dist`; `--assets DIR` replaces it.
static WEB_ASSETS: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (port, assets, dev_origin) = options(args)?;
    start(port, assets, dev_origin, None)
}

/// `swarm ui`: reuse the `swarm ui` server of this home when its recorded URL still answers with its
/// token, else start one, record its URL in `<root>/ui-url`, and open the page.
pub fn ui(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (port, assets, dev_origin) = options(args)?;
    if assets.is_none() && WEB_ASSETS.is_empty() {
        return Err("swarm ui: this build has no web UI; run `pnpm --dir web build` and rebuild, or pass --assets DIR".into());
    }
    let record = swarm::paths::root_dir()?.join("ui-url");
    if let Ok(url) = std::fs::read_to_string(&record)
        && answers(url.trim())
    {
        println!("{}", url.trim());
        open_page(url.trim());
        return Ok(());
    }
    start(port, assets, dev_origin, Some(record))
}

/// True when the server at `http://127.0.0.1:<port>/#token=<token>` takes that token.
fn answers(url: &str) -> bool {
    let Some((port, token)) = url
        .strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.split_once("/#token="))
    else {
        return false;
    };
    let Ok(port) = port.parse::<u16>() else {
        return false;
    };
    let address = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(&address, Duration::from_millis(500))
    else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let request = format!(
        "GET /api/badge HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    let mut status = [0; 12];
    stream.write_all(request.as_bytes()).is_ok()
        && stream.read_exact(&mut status).is_ok()
        && &status == b"HTTP/1.1 200"
}

fn open_page(url: &str) {
    if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .status();
    }
}

type Options = (u16, Option<PathBuf>, Option<String>);

fn options(args: &[String]) -> Result<Options, Box<dyn std::error::Error>> {
    let mut port = 0;
    let mut assets = None;
    let mut dev_origin = None;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or("usage: swarm serve|ui [--port N] [--assets DIR] [--dev-origin ORIGIN]")?;
        match flag.as_str() {
            "--port" => port = value.parse::<u16>()?,
            "--assets" => assets = Some(std::fs::canonicalize(value)?),
            "--dev-origin" => dev_origin = Some(value.clone()),
            _ => {
                return Err(
                    "usage: swarm serve|ui [--port N] [--assets DIR] [--dev-origin ORIGIN]".into(),
                );
            }
        }
    }
    Ok((port, assets, dev_origin))
}

/// Serves until killed. With `record`, writes the URL there and opens it (`swarm ui`).
fn start(
    port: u16,
    assets: Option<PathBuf>,
    dev_origin: Option<String>,
    record: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
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
        .or_else(|| swarm::paths::bundled("transcript"))
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/transcript/zig-out/bin/transcript")
        });
    let (actions, action_queue) = mpsc::channel(64);
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
            row_cache: VecDeque::new(),
        }),
        directories: Mutex::new(HashMap::new()),
        stopped: AtomicBool::new(false),
        cli: std::env::current_exe()?,
        actions,
        tree_cache: Mutex::new(TreeCache::default()),
        handoffs: Mutex::new(HashMap::new()),
        chat_create: tokio::sync::Mutex::new(()),
        usage_refresh: Mutex::new(HashMap::new()),
    });
    let url = format!("http://127.0.0.1:{port}/#token={}", app.token);
    println!("{url}");
    std::io::stdout().flush()?;
    if let Some(record) = record {
        use std::os::unix::fs::OpenOptionsExt;
        // The token is the API's only key, so only the owner may read the record.
        let _ = std::fs::remove_file(&record);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&record)?
            .write_all(format!("{url}\n").as_bytes())?;
        open_page(&url);
    }
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        start_coordinator(&app, action_queue);
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
        .route("/api/tree", get(tree_view))
        .route("/api/badge", get(badge_view))
        .route("/api/tabs", get(tabs_view))
        .route("/api/choices", post(choice_write))
        .route("/api/changes", get(panel_read))
        .route("/api/changes/diff", get(panel_read))
        .route("/api/changes/file", get(panel_read))
        .route("/api/pr", get(panel_read))
        .route("/api/issues", get(panel_read))
        .route("/api/usage", get(usage_read))
        .route("/api/usage/refresh", post(usage_refresh))
        .route("/api/roles", get(menu_read).post(owner_write))
        .route("/api/providers", get(menu_read))
        .route("/api/models", get(menu_read))
        .route("/api/accounts", get(menu_read))
        .route("/api/accounts/login", post(owner_write))
        .route("/api/accounts/reset", post(owner_write))
        .route("/api/chat/continue", post(continue_chat))
        .route("/api/chat/continue/cancel", post(cancel_continue))
        .route("/api/workspace/new-options", get(lifecycle_read))
        .route("/api/workspace/impact", get(lifecycle_read))
        .route("/api/workspace/new", post(lifecycle_write))
        .route("/api/workspace/archive", post(lifecycle_write))
        .route("/api/workspace/restore", post(lifecycle_write))
        .route("/api/workspace/delete", post(lifecycle_write))
        .route("/api/workspace/prune", post(lifecycle_write))
        .route("/api/project/create", post(lifecycle_write))
        .route("/api/project/import", post(lifecycle_write))
        .route("/api/project/remove", post(lifecycle_write))
        .route("/api/transcript", get(transcript))
        .route("/api/rows", get(transcript_rows))
        .route("/api/tool-output", get(tool_output))
        .route("/api/stream", get(stream))
        .route("/api/subscribe", post(subscribe))
        .route("/api/chat/send", post(chat_action))
        .route("/api/chat/pull-back", post(chat_action))
        .route("/api/chat/interrupt", post(chat_action))
        .route("/api/chat/key", post(chat_action))
        .route("/api/chat/answer", post(chat_action))
        .route("/api/agent/close", post(chat_action))
        .route("/api/sessions/archive", post(chat_action))
        .route("/api/sessions/unarchive", post(chat_action))
        .route("/api/chat/new", post(chat_action))
        .route("/api/sessions/archived", get(archived_sessions))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeQuery {
    view: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TabsQuery {
    workspace: String,
}

fn choice_error(err: choices_store::StoreError) -> ApiError {
    let status = match err {
        choices_store::StoreError::Stale => StatusCode::CONFLICT,
        choices_store::StoreError::Busy => StatusCode::SERVICE_UNAVAILABLE,
        choices_store::StoreError::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error(status, &err.to_string())
}

fn loaded_tree(
    app: &Server,
) -> Result<
    (
        tree::Tree,
        choices_store::Saved,
        BTreeMap<String, fields::WorkspaceFields>,
    ),
    ApiError,
> {
    let loaded = choices_store::load(&app.root, false);
    let mut cache = app.tree_cache.lock().unwrap();
    let mut saved = match loaded {
        Ok(saved) => {
            cache.choices_error = None;
            cache.saved = Some(saved.clone());
            saved
        }
        Err(err) => {
            cache.choices_error = Some(err.to_string());
            cache.saved.clone().unwrap_or_else(|| choices_store::Saved {
                owner: choices::OwnerChoices::default(),
                state: choices::ViewState::default(),
                revision: String::new(),
            })
        }
    };
    let snapshot = if cache.snapshot.is_object() {
        cache.snapshot.clone()
    } else {
        snapshot_value(app).map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    };
    let tree = tree::build(&snapshot, &saved.owner);
    saved.state.record_first_sight(&tree);
    Ok((tree, saved, cache.discovery.fields.clone()))
}

async fn tree_view(
    State(app): State<App>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Value>, ApiError> {
    let archive = match query.view.as_deref().unwrap_or("workspaces") {
        "workspaces" => false,
        "archived" => true,
        _ => return Err(error(StatusCode::BAD_REQUEST, "unknown tree view")),
    };
    tokio::task::spawn_blocking(move || {
        let (tree, saved, cache) = loaded_tree(&app)?;
        let sections = tree::sections(
            &tree,
            &saved.owner,
            &saved.state,
            archive,
            discovery::now(),
            &cache,
        );
        let rows: Vec<_> = sections
            .iter()
            .filter(|s| s.expanded)
            .flat_map(|s| s.rows.clone())
            .collect();
        let selected = saved
            .state
            .selected_workspace
            .as_deref()
            .and_then(|w| navigation::selected_chat(&tree, &saved.owner, &saved.state, w));
        let selected_row = navigation::selected_row(
            &rows,
            saved.state.selected_workspace.as_deref(),
            selected,
            saved.state.selected_child.as_deref(),
        );
        let (version, agents) = {
            let hub = app.hub.lock().unwrap();
            (hub.version, hub.agents.clone())
        };
        let archived_status = tree::AgentStatus::aggregate(
            tree.projects
                .iter()
                .flat_map(|p| &p.workspaces)
                .filter(|w| saved.owner.archived.contains(&w.path))
                .flat_map(|w| w.chats.iter().flat_map(|c| &c.sessions))
                .flat_map(|s| {
                    agents[s["id"].as_str().unwrap_or_default()]
                        .as_array()
                        .into_iter()
                        .flatten()
                })
                .map(tree::AgentStatus::agent)
                .filter(|s| *s == tree::AgentStatus::Waiting),
        );
        let choices_error = app.tree_cache.lock().unwrap().choices_error.clone();
        Ok(Json(json!({
            "version": version,
            "revision": saved.revision,
            "view": if archive { "archived" } else { "workspaces" },
            "sections": sections,
            "selectedRow": selected_row,
            "archivedStatus": archived_status,
            "choicesError": choices_error,
            "prefs": saved.owner.prefs,
        })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "tree task failed"))?
}

/// The Dock badge of the shell (ADR 0077): the chats that wait, outside archived workspaces, or 0
/// when the owner turned the badge off.
async fn badge_view(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (tree, saved, _) = loaded_tree(&app)?;
        let count = if saved.owner.prefs["notices"]["badge"] == false {
            0
        } else {
            tree.projects
                .iter()
                .flat_map(|p| &p.workspaces)
                .filter(|w| !saved.owner.archived.contains(&w.path))
                .flat_map(|w| &w.chats)
                .filter(|c| c.status == Some(tree::AgentStatus::Waiting))
                .count()
        };
        Ok(Json(json!({ "count": count })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "badge task failed"))?
}

async fn tabs_view(
    State(app): State<App>,
    Query(query): Query<TabsQuery>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (tree, saved, cache) = loaded_tree(&app)?;
        let view = tabs::view(
            &tree,
            &saved.owner,
            &saved.state,
            &query.workspace,
            discovery::now(),
            &cache,
        )
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown workspace"))?;
        Ok(Json(json!({
            "version": app.hub.lock().unwrap().version,
            "revision": saved.revision,
            "view": view,
        })))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "tabs task failed"))?
}

async fn choice_write(
    State(app): State<App>,
    body: Result<Json<choices::ChoiceWrite>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    enqueue_action(&app, Action::Choices(body)).await.map(Json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MenuQuery {
    provider: Option<String>,
}
async fn menu_read(
    State(app): State<App>,
    uri: Uri,
    Query(query): Query<MenuQuery>,
) -> Result<Json<Value>, ApiError> {
    let command = uri.path().trim_start_matches("/api/");
    let mut args = vec![command.to_owned()];
    if matches!(command, "models" | "accounts") {
        let provider = query
            .provider
            .filter(|p| matches!(p.as_str(), "claude" | "codex" | "agy"))
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "known provider required"))?;
        args.extend(["--provider".into(), provider]);
    } else if query.provider.is_some() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "provider is not accepted here",
        ));
    }
    args.push("--json".into());
    let text = run_cli(
        &app,
        CliCall {
            args,
            cwd: app.root.clone(),
            session: None,
            adapter: String::new(),
        },
        None,
    )
    .await?;
    serde_json::from_str(&text)
        .map(Json)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid menu JSON from swarm"))
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum ProfileOp {
    Save { profile: Value },
    New { name: String },
    Rename { name: String, to: String },
    Copy { name: String, to: String },
    Delete { name: String },
    Reset,
    MinUsage { pct: u8 },
}
#[derive(Deserialize)]
struct ProfileWrite {
    revision: String,
    #[serde(flatten)]
    op: ProfileOp,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountLogin {
    provider: String,
    name: String,
    revision: String,
    /// The login pane opens here when it is a listed workspace, else in the home folder (Swift).
    workspace: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountReset {
    revision: String,
}
/// Profile and account writes (S5). Each runs the `swarm roles` or `swarm accounts` owner with the
/// revision the page read.
#[derive(Deserialize)]
enum OwnerWrite {
    Profile(ProfileWrite),
    Login(AccountLogin),
    ResetAccounts(AccountReset),
}
async fn owner_write(
    State(app): State<App>,
    uri: Uri,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                e.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    let kind = match uri.path() {
        "/api/roles" => "Profile",
        "/api/accounts/login" => "Login",
        _ => "ResetAccounts",
    };
    let write = serde_json::from_value(json!({ kind: body }))
        .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
    enqueue_action(&app, Action::Owner(write)).await.map(Json)
}
async fn run_owner_write(app: &App, write: OwnerWrite) -> Result<Value, ApiError> {
    let flag = |revision: String| ["--revision".to_owned(), revision];
    let args: Vec<String> = match write {
        OwnerWrite::Profile(ProfileWrite { revision, op }) => {
            let mut args = vec!["roles".to_owned()];
            match op {
                ProfileOp::Save { profile } => {
                    args.push("save".into());
                    args.extend(flag(revision));
                    args.push(profile.to_string());
                    return owner_cli(app, args).await;
                }
                ProfileOp::New { name } => args.extend(["new".into(), name]),
                ProfileOp::Rename { name, to } => args.extend(["rename".into(), name, to]),
                ProfileOp::Copy { name, to } => args.extend(["copy".into(), name, to]),
                ProfileOp::Delete { name } => args.extend(["delete".into(), name]),
                ProfileOp::Reset => args.push("reset".into()),
                ProfileOp::MinUsage { pct } => {
                    args.extend(["set-min-usage".into(), pct.to_string()])
                }
            }
            args.extend(flag(revision));
            args
        }
        OwnerWrite::Login(body) => {
            if !["claude", "codex"].contains(&body.provider.as_str()) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "Accounts sign in to claude or codex",
                ));
            }
            let listed = match body.workspace {
                Some(path) => {
                    let app = app.clone();
                    tokio::task::spawn_blocking(move || {
                        (loaded_tree(&app)
                            .is_ok_and(|(tree, _, _)| lifecycle::workspace(&tree, &path).is_ok())
                            && Path::new(&path).is_dir())
                        .then_some(path)
                    })
                    .await
                    .ok()
                    .flatten()
                }
                None => None,
            };
            let cwd = listed
                .or_else(|| std::env::var("HOME").ok())
                .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "HOME is not set"))?;
            let mut args = vec![
                "accounts".to_owned(),
                "login".into(),
                "--provider".into(),
                body.provider,
                "--name".into(),
                body.name,
            ];
            args.extend(flag(body.revision));
            args.extend(["--json".into(), "--cwd".into(), cwd]);
            args
        }
        OwnerWrite::ResetAccounts(body) => {
            let mut args = vec!["accounts".to_owned(), "reset".into()];
            args.extend(flag(body.revision));
            args.push("--json".into());
            args
        }
    };
    owner_cli(app, args).await
}
async fn owner_cli(app: &App, args: Vec<String>) -> Result<Value, ApiError> {
    // The Swift app's adapter, so the login pane opens where its chats do; a credential never
    // passes through this server.
    let call = CliCall {
        args,
        cwd: app.root.clone(),
        session: None,
        adapter: "tmux-solo".into(),
    };
    let output = run_cli(app, call, None).await.map_err(|(status, body)| {
        let stale = body.0["error"]
            .as_str()
            .is_some_and(|text| text.contains("changed on disk"));
        // run_cli reports every refusal as 409. Only a stale revision keeps it, so the page
        // reloads on a conflict and shows any other refusal as it is.
        if status == StatusCode::CONFLICT && !stale {
            (StatusCode::BAD_REQUEST, body)
        } else {
            (status, body)
        }
    })?;
    serde_json::from_str(&output)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid JSON from swarm"))
}
fn apply_choice(app: &App, body: choices::ChoiceWrite) -> Result<Value, ApiError> {
    let snapshot = {
        let cache = app.tree_cache.lock().unwrap();
        if cache.snapshot.is_object() {
            cache.snapshot.clone()
        } else {
            snapshot_value(app)
                .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        }
    };
    let mut invalid = None;
    let saved = choices_store::update(&app.root, Some(&body.revision), |owner, state| {
        let tree = tree::build(&snapshot, owner);
        match choices::apply(owner, state, &tree, body.operation, discovery::now()) {
            Ok(()) => Ok(()),
            Err(e) => {
                invalid = Some(e.clone());
                Err(e)
            }
        }
    })
    .map_err(|e| {
        if let Some(message) = invalid {
            error(StatusCode::BAD_REQUEST, &message)
        } else {
            choice_error(e)
        }
    })?;
    if saved.revision != body.revision {
        app.hub.lock().unwrap().bump();
    }
    {
        let mut cache = app.tree_cache.lock().unwrap();
        cache.revision = saved.revision.clone();
        cache.saved = Some(saved.clone());
        cache.choices_error = None;
    }
    Ok(json!({"revision":saved.revision,"version":app.hub.lock().unwrap().version}))
}
fn refresh_tree(app: &App) -> Result<Value, ApiError> {
    let saved = choices_store::load(&app.root, false).map_err(choice_error)?;
    let mut snapshot = snapshot_value(app)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    snapshot.as_object_mut().unwrap().remove("version");
    let mut discovery = app.tree_cache.lock().unwrap().discovery.clone();
    discovery.augment(&mut snapshot, &saved.owner, discovery::now());
    let first_tree = tree::build(&snapshot, &saved.owner);
    let mut discovered_owner = saved.owner.clone();
    for project in &first_tree.projects {
        if !discovered_owner.project_paths.contains(&project.path)
            && !discovered_owner
                .project_paths
                .contains(&project.launch_directory)
        {
            discovered_owner
                .project_paths
                .push(project.launch_directory.clone());
        }
    }
    let needs_projects = discovered_owner.project_paths != saved.owner.project_paths;
    let tree = tree::build(&snapshot, &discovered_owner);
    let mut normalized_state = saved.state.clone();
    normalized_state.record_first_sight(&tree);
    let needs_tabs = tree.projects.iter().flat_map(|p| &p.workspaces).any(|w| {
        saved.owner.tabs.get(&w.path).is_none_or(|strip| {
            strip.pruned(&w.chats.iter().map(|c| c.key.clone()).collect()) != *strip
        })
    });
    let saved = if normalized_state != saved.state || needs_tabs || needs_projects {
        choices_store::update(&app.root, Some(&saved.revision), |owner, state| {
            state.record_first_sight(&tree);
            for p in &tree.projects {
                if !owner.project_paths.contains(&p.path)
                    && !owner.project_paths.contains(&p.launch_directory)
                {
                    owner.project_paths.push(p.launch_directory.clone());
                }
                for w in &p.workspaces {
                    let strip = owner
                        .tabs
                        .entry(w.path.clone())
                        .or_insert_with(|| w.seed_tabs());
                    *strip = strip.pruned(&w.chats.iter().map(|c| c.key.clone()).collect());
                }
            }
            Ok(())
        })
        .map_err(choice_error)?
    } else {
        saved
    };
    let fields_changed = discovery.refresh_fields(&tree, &saved.owner, discovery::now());
    let mut cache = app.tree_cache.lock().unwrap();
    cache.discovery = discovery;
    if cache.snapshot != snapshot || cache.revision != saved.revision || fields_changed {
        app.hub.lock().unwrap().bump();
    }
    cache.snapshot = snapshot;
    cache.revision = saved.revision.clone();
    cache.saved = Some(saved);
    Ok(json!({"ok":true}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectPath {
    project: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspacePath {
    workspace: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelQuery {
    workspace: String,
    #[serde(default)]
    mode: changes::Mode,
    base: Option<String>,
    path: Option<String>,
    layer: Option<String>,
}
async fn panel_read(
    State(app): State<App>,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let (tree, _) = fresh_lifecycle_tree(&app)?;
        let value = serde_json::to_value(query).unwrap();
        if uri.path() == "/api/issues" {
            let query: ProjectPath = serde_json::from_value(value)
                .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
            let project = lifecycle::project(&tree, &query.project).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &query.project, true).map_err(lifecycle_error)?;
            let common = lifecycle::common(project)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Issues need a Git project"))?;
            lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            return workspace_panel::issues(common)
                .map(Json)
                .map_err(lifecycle_error);
        }
        let query: PanelQuery = serde_json::from_value(value)
            .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
        lifecycle::workspace(&tree, &query.workspace).map_err(lifecycle_error)?;
        lifecycle::checked_path(&app.root, &query.workspace, true).map_err(lifecycle_error)?;
        let path = || {
            query
                .path
                .as_deref()
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Select a file path"))
        };
        let result = match uri.path() {
            "/api/changes" => changes::list(&query.workspace, &query.mode, query.base.as_deref()),
            "/api/changes/diff" => changes::patch(
                &query.workspace,
                path()?,
                &query.mode,
                query.base.as_deref(),
                query.layer.as_deref(),
            ),
            "/api/changes/file" => changes::file(&query.workspace, path()?),
            _ => workspace_panel::pr(&query.workspace),
        };
        result.map(Json).map_err(lifecycle_error)
    })
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "workspace read task failed",
        )
    })?
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageRefresh {
    provider: String,
}
async fn usage_read(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    read_usage(&app, None).await.map(Json)
}
async fn usage_refresh(
    State(app): State<App>,
    body: Result<Json<UsageRefresh>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| error(e.status(), &e.body_text()))?;
    if !["claude", "codex"].contains(&body.provider.as_str()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Usage refresh supports claude or codex",
        ));
    }
    enqueue_action(&app, Action::UsageRefresh(body))
        .await
        .map(Json)
}
async fn read_usage(app: &App, refresh: Option<&str>) -> Result<Value, ApiError> {
    let mut args = vec!["usage", "--json"];
    if let Some(provider) = refresh {
        args = vec!["usage", "--refresh", "--provider", provider, "--json"];
    }
    let output = run_cli(
        app,
        CliCall {
            args: args.into_iter().map(String::from).collect(),
            cwd: app.root.clone(),
            session: None,
            adapter: String::new(),
        },
        None,
    )
    .await?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|_| error(StatusCode::BAD_GATEWAY, "swarm returned invalid usage JSON"))?;
    if !value.is_object() {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            "swarm returned invalid usage data",
        ));
    }
    Ok(value)
}
async fn refresh_usage(app: &App, provider: &str) -> Result<Value, ApiError> {
    let (remaining, failure) = {
        let mut refresh = app.usage_refresh.lock().unwrap();
        let now = tokio::time::Instant::now();
        let remaining = refresh
            .get(provider)
            .map(|(last, _)| Duration::from_secs(300).saturating_sub(now.duration_since(*last)))
            .unwrap_or_default();
        if remaining.is_zero() {
            refresh.insert(provider.into(), (now, None));
        }
        (
            remaining,
            refresh
                .get(provider)
                .and_then(|(_, failure)| failure.clone()),
        )
    };
    if let Some(failure) = failure {
        return Err(failure);
    }
    let reading = read_usage(app, remaining.is_zero().then_some(provider)).await;
    if remaining.is_zero()
        && let Err(err) = &reading
    {
        app.usage_refresh
            .lock()
            .unwrap()
            .get_mut(provider)
            .unwrap()
            .1 = Some(err.clone());
    }
    let mut value = reading?;
    value["refresh"] = json!({"provider":provider,"state":if remaining.is_zero() {"refreshed"} else {"cooldown"},
        "next_in_seconds":if remaining.is_zero() {300} else {remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)}});
    Ok(value)
}
#[derive(Deserialize)]
enum LifecycleAction {
    Options(ProjectPath),
    Impact(WorkspacePath),
    New(lifecycle::NewWorkspace),
    Archive(WorkspacePath),
    Restore(WorkspacePath),
    Delete(WorkspacePath),
    Prune(WorkspacePath),
    Create(lifecycle::CreateProject),
    Import(lifecycle::ImportProject),
    Remove(ProjectPath),
}
fn lifecycle_error(err: lifecycle::Error) -> ApiError {
    let status = match err {
        lifecycle::Error::Forbidden => StatusCode::FORBIDDEN,
        lifecycle::Error::Invalid(_) => StatusCode::BAD_REQUEST,
        lifecycle::Error::Failed(_) => StatusCode::CONFLICT,
    };
    error(status, &err.to_string())
}
async fn lifecycle_read(
    State(app): State<App>,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let value = serde_json::to_value(query).unwrap();
    let action = match uri.path() {
        "/api/workspace/new-options" => LifecycleAction::Options(
            serde_json::from_value(value)
                .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?,
        ),
        _ => LifecycleAction::Impact(
            serde_json::from_value(value)
                .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?,
        ),
    };
    enqueue_action(&app, Action::Lifecycle(action))
        .await
        .map(Json)
}
async fn lifecycle_write(
    State(app): State<App>,
    uri: Uri,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                e.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    let kind = match uri.path() {
        "/api/workspace/new" => "New",
        "/api/workspace/archive" => "Archive",
        "/api/workspace/restore" => "Restore",
        "/api/workspace/delete" => "Delete",
        "/api/workspace/prune" => "Prune",
        "/api/project/create" => "Create",
        "/api/project/import" => "Import",
        _ => "Remove",
    };
    let action = serde_json::from_value(json!({kind:body}))
        .map_err(|e| error(StatusCode::BAD_REQUEST, &e.to_string()))?;
    enqueue_action(&app, Action::Lifecycle(action))
        .await
        .map(Json)
}
fn fresh_lifecycle_tree(app: &App) -> Result<(tree::Tree, choices_store::Saved), ApiError> {
    let saved = choices_store::load(&app.root, true).map_err(choice_error)?;
    let mut snapshot = snapshot_value(app)
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let mut discovery = app.tree_cache.lock().unwrap().discovery.clone();
    discovery.forget_catalog();
    discovery.augment(&mut snapshot, &saved.owner, discovery::now());
    let tree = tree::build(&snapshot, &saved.owner);
    let mut cache = app.tree_cache.lock().unwrap();
    cache.discovery = discovery;
    cache.snapshot = snapshot;
    cache.saved = Some(saved.clone());
    Ok((tree, saved))
}
async fn workspace_live_targets(
    app: &App,
    workspace: &tree::Workspace,
) -> Result<Vec<(String, String)>, ApiError> {
    let sessions: Vec<_> = workspace
        .chats
        .iter()
        .chain(&workspace.unlisted)
        .flat_map(|c| c.sessions.iter().cloned())
        .collect();
    if sessions.is_empty() {
        return Ok(Vec::new());
    }
    let call = CliCall {
        args: vec!["agents".into(), "--json".into(), "--all".into()],
        cwd: app.root.clone(),
        session: None,
        adapter: "tmux-solo".into(),
    };
    let batch = run_cli(app, call, None)
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    let mut listing = batch.as_ref().and_then(Value::as_object).map(|entries| {
        entries
            .iter()
            .filter_map(|(id, v)| v["agents"].as_array().map(|a| (id.clone(), json!(a))))
            .collect::<serde_json::Map<_, _>>()
    });
    let mut singles = HashMap::new();
    for session in &sessions {
        let id = session["id"].as_str().unwrap_or_default();
        if listing.as_ref().is_some_and(|l| l.contains_key(id))
            || session["adapter"]
                .as_str()
                .is_none_or(|a| a.trim().is_empty())
            || (listing.is_some() && session["agents"].as_u64() == Some(0))
        {
            continue;
        }
        let call = action_session(app, id)?.args(&["agents", "--json"]);
        let result = run_cli(app, call, None)
            .await
            .map_err(|e| e.1.0.to_string())
            .and_then(|s| {
                serde_json::from_str::<Value>(&s)
                    .map_err(|e| e.to_string())
                    .and_then(|v| {
                        v["agents"]
                            .as_array()
                            .cloned()
                            .ok_or_else(|| "invalid agents".into())
                    })
            });
        singles.insert(id.to_owned(), result);
    }
    let listing = listing.take().map(Value::Object);
    lifecycle::live_targets(&sessions, listing.as_ref(), |session| {
        singles
            .remove(session["id"].as_str().unwrap_or_default())
            .unwrap_or_else(|| Err("missing listing".into()))
    })
    .map_err(lifecycle_error)
}
async fn execute_lifecycle(app: &App, action: LifecycleAction) -> Result<Value, ApiError> {
    let cloned = app.clone();
    let (tree, saved) = tokio::task::spawn_blocking(move || fresh_lifecycle_tree(&cloned))
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "lifecycle tree task failed",
            )
        })??;
    let target = match &action {
        LifecycleAction::Impact(p) | LifecycleAction::Archive(p) | LifecycleAction::Delete(p) => {
            let (project, workspace) =
                lifecycle::workspace(&tree, &p.workspace).map_err(lifecycle_error)?;
            lifecycle::checked_path(
                &app.root,
                &p.workspace,
                !workspace.missing && !workspace.removed,
            )
            .map_err(lifecycle_error)?;
            if let Some(common) = lifecycle::common(project) {
                lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            }
            if matches!(action, LifecycleAction::Delete(_)) {
                if !lifecycle::can_delete(project, workspace) {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "This workspace cannot be deleted",
                    ));
                }
                let path = p.workspace.clone();
                let blocker =
                    tokio::task::spawn_blocking(move || lifecycle::removal_blocker(&path))
                        .await
                        .map_err(|_| {
                            error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "delete inspection failed",
                            )
                        })?
                        .map_err(lifecycle_error)?;
                if let Some(blocker) = blocker {
                    return Err(error(StatusCode::CONFLICT, &blocker));
                }
            }
            Some(workspace.clone())
        }
        _ => None,
    };
    let targets = if let Some(workspace) = &target {
        workspace_live_targets(app, workspace).await?
    } else {
        Vec::new()
    };
    if matches!(
        action,
        LifecycleAction::Archive(_) | LifecycleAction::Delete(_)
    ) {
        for (session, agent) in &targets {
            let (call, agent) = action_target(app, session, agent)?;
            run_cli(app, call.args(&["close", &agent]), None).await?;
        }
    }
    let setup = if let LifecycleAction::New(request) = &action {
        saved
            .owner
            .project_defaults
            .get(&request.project)
            .and_then(|s| s["setup_script"].as_str())
            .filter(|s| !s.trim().is_empty())
            .map(String::from)
    } else {
        None
    };
    let cloned = app.clone();
    let mut result = tokio::task::spawn_blocking(move || {
        apply_lifecycle(&cloned, action, &tree, &saved, targets.len())
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "lifecycle task failed"))??;
    if let Some(script) = setup {
        let path = result["workspace"]
            .as_str()
            .expect("created workspace")
            .to_owned();
        result["setup"] = run_setup(app, &path, &script, Duration::from_secs(600)).await;
    }
    Ok(result)
}

fn setup_frame(app: &App, workspace: &str, payload: Value) {
    let mut payload = payload;
    payload["workspace"] = workspace.into();
    app.hub.lock().unwrap().publish(Frame::new(
        "setup",
        payload,
        Some(&format!("setup:{workspace}")),
    ));
}
fn setup_text(pending: &mut Vec<u8>, bytes: &[u8], eof: bool) -> String {
    pending.extend_from_slice(bytes);
    let mut text = String::new();
    let mut consumed = 0;
    while consumed < pending.len() {
        match std::str::from_utf8(&pending[consumed..]) {
            Ok(rest) => {
                text.push_str(rest);
                consumed = pending.len();
            }
            Err(err) => {
                let valid = consumed + err.valid_up_to();
                text.push_str(std::str::from_utf8(&pending[consumed..valid]).unwrap());
                consumed = valid;
                if let Some(size) = err.error_len() {
                    text.push('\u{fffd}');
                    consumed += size;
                } else if eof {
                    text.push('\u{fffd}');
                    consumed = pending.len();
                } else {
                    break;
                }
            }
        }
    }
    pending.drain(..consumed);
    text
}
async fn run_setup(app: &App, workspace: &str, script: &str, timeout: Duration) -> Value {
    use tokio::io::AsyncReadExt;
    setup_frame(app, workspace, json!({"state":"running"}));
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .args(["-c", script])
        .current_dir(workspace)
        .env("PWD", workspace)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    let result = match command.spawn() {
        Err(err) => json!({"state":"failed","exit_code":null,"error":err.to_string()}),
        Ok(mut child) => {
            let mut group = ActionProcessGroup(Some(child.id().expect("running setup") as i32));
            let mut stdout = child.stdout.take().expect("piped setup stdout");
            let mut stderr = child.stderr.take().expect("piped setup stderr");
            let finished = tokio::time::timeout(timeout, async {
                let mut out_buffer = [0; 4096];
                let mut err_buffer = [0; 4096];
                let (mut out_pending, mut err_pending) = (Vec::new(), Vec::new());
                let (mut out_open, mut err_open) = (true, true);
                let mut status = None;
                while out_open || err_open || status.is_none() {
                    tokio::select! {
                        waited = child.wait(), if status.is_none() => {
                            status = Some(waited?);
                            // Close inherited pipes before draining the last output.
                            drop(ActionProcessGroup(group.0.take()));
                        }
                        read = stdout.read(&mut out_buffer), if out_open => {
                            let size = read?;
                            out_open = size > 0;
                            let text = setup_text(&mut out_pending, &out_buffer[..size], !out_open);
                            if !text.is_empty() { setup_frame(app, workspace, json!({"state":"output","channel":"stdout","text":text})); }
                        }
                        read = stderr.read(&mut err_buffer), if err_open => {
                            let size = read?;
                            err_open = size > 0;
                            let text = setup_text(&mut err_pending, &err_buffer[..size], !err_open);
                            if !text.is_empty() { setup_frame(app, workspace, json!({"state":"output","channel":"stderr","text":text})); }
                        }
                    }
                }
                Ok::<_, std::io::Error>(status.expect("setup was reaped"))
            }).await;
            drop(ActionProcessGroup(group.0.take()));
            match finished {
                Ok(Ok(status)) => {
                    json!({"state":if status.success() {"completed"} else {"failed"},"exit_code":status.code()})
                }
                other => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    match other {
                        Err(_) => {
                            json!({"state":"timeout","exit_code":null,"error":"Setup exceeded its time limit"})
                        }
                        Ok(Err(err)) => {
                            json!({"state":"failed","exit_code":null,"error":err.to_string()})
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    };
    setup_frame(app, workspace, result.clone());
    result
}
fn apply_lifecycle(
    app: &App,
    action: LifecycleAction,
    tree: &tree::Tree,
    saved: &choices_store::Saved,
    live: usize,
) -> Result<Value, ApiError> {
    let archive = matches!(action, LifecycleAction::Archive(_));
    let mut result = json!({"ok":true});
    match action {
        LifecycleAction::Options(p) => {
            let project = lifecycle::project(tree, &p.project).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.project, true).map_err(lifecycle_error)?;
            if let Some(common) = lifecycle::common(project) {
                lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            }
            return lifecycle::options(project, &saved.owner).map_err(lifecycle_error);
        }
        LifecycleAction::Impact(p) => {
            let (project, workspace) =
                lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            return lifecycle::impact(project, workspace, live).map_err(lifecycle_error);
        }
        LifecycleAction::New(mut request) => {
            let project = lifecycle::project(tree, &request.project).map_err(lifecycle_error)?;
            if matches!(request.mode, lifecycle::Mode::Issue) {
                lifecycle::checked_path(&app.root, &request.project, true)
                    .map_err(lifecycle_error)?;
                let common = lifecycle::common(project)
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Issues need a Git project"))?;
                lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
                let number = request
                    .issue
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "Select an issue"))?;
                let issue = workspace_panel::issue(common, number).map_err(lifecycle_error)?;
                request.name = format!("{number}-{}", issue["title"].as_str().unwrap());
                result["issue"] = issue;
            }
            let (path, settings) =
                lifecycle::create_workspace(&app.root, project, &saved.owner, &request)
                    .map_err(lifecycle_error)?;
            result["workspace"] = path.clone().into();
            let recorded = choices_store::update(
                &app.root,
                None,
                |owner: &mut choices::OwnerChoices, state: &mut choices::ViewState| {
                    lifecycle::remember(owner, &path, &project.path);
                    owner
                        .project_defaults
                        .insert(project.path.clone(), settings);
                    owner.names.insert(path.clone(), request.name.trim().into());
                    state.selected_workspace = Some(path.clone());
                    state.selected_child = None;
                    Ok(())
                },
            )
            .map_err(choice_error);
            if let Err(err) = recorded {
                result["notice"] = format!("The workspace at {path} exists, so do not create it again. Swarm could not record it. {}",err.1.0).into();
            }
        }
        LifecycleAction::Archive(p) | LifecycleAction::Restore(p) => {
            lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.workspace, false).map_err(lifecycle_error)?;
            choices_store::update(&app.root, None, |owner, state| {
                if archive {
                    owner.archived.insert(p.workspace.clone());
                    lifecycle::clear_selection(state, &p.workspace);
                } else {
                    owner.archived.remove(&p.workspace);
                }
                Ok(())
            })
            .map_err(choice_error)?;
        }
        LifecycleAction::Delete(p) => {
            let (project, workspace) =
                lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.workspace, true).map_err(lifecycle_error)?;
            if !lifecycle::can_delete(project, workspace) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "This workspace cannot be deleted",
                ));
            }
            if let Some(blocker) =
                lifecycle::removal_blocker(&p.workspace).map_err(lifecycle_error)?
            {
                return Err(error(StatusCode::CONFLICT, &blocker));
            }
            let common = lifecycle::common(project)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "not a Git project"))?;
            lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            lifecycle::check_git(common, &["worktree", "remove", "--", &p.workspace])
                .map_err(lifecycle_error)?;
            choices_store::update(&app.root, None, |owner, state| {
                owner.pinned.remove(&p.workspace);
                owner.archived.remove(&p.workspace);
                owner.names.remove(&p.workspace);
                owner.tabs.remove(&p.workspace);
                owner.project_paths.retain(|path| path != &p.workspace);
                if !owner.project_paths.contains(&project.path) {
                    owner.project_paths.push(project.path.clone());
                }
                if let Some(order) = owner.workspace_order.get_mut(&project.path) {
                    order.retain(|path| path != &p.workspace);
                }
                lifecycle::clear_selection(state, &p.workspace);
                Ok(())
            })
            .map_err(choice_error)?;
        }
        LifecycleAction::Prune(p) => {
            let (project, workspace) =
                lifecycle::workspace(tree, &p.workspace).map_err(lifecycle_error)?;
            if !workspace.missing || workspace.removed {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "Only a missing worktree can be pruned",
                ));
            }
            lifecycle::checked_path(&app.root, &p.workspace, false).map_err(lifecycle_error)?;
            let common = lifecycle::common(project)
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "not a Git project"))?;
            lifecycle::checked_path(&app.root, common, true).map_err(lifecycle_error)?;
            lifecycle::check_git(common, &["worktree", "prune"]).map_err(lifecycle_error)?;
        }
        LifecycleAction::Create(request) => {
            if request.name.trim().is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "Enter a project name"));
            }
            let path = lifecycle::checked_path(&app.root, &request.path, false)
                .map_err(lifecycle_error)?;
            if std::fs::symlink_metadata(&path).is_ok() {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "A file or folder already exists at this path",
                ));
            }
            std::fs::create_dir(&path).map_err(|e| error(StatusCode::CONFLICT, &e.to_string()))?;
            let path = path.to_string_lossy().into_owned();
            let existed = saved.owner.project_paths.contains(&path);
            let created = (|| {
                choices_store::update(&app.root, None, |owner, _| {
                    lifecycle::remember(owner, &path, &path);
                    if request.name.trim() != tree::basename(&path) {
                        owner
                            .project_names
                            .insert(path.clone(), request.name.trim().into());
                    }
                    Ok(())
                })
                .map_err(choice_error)?;
                lifecycle::initialize_project(&path).map_err(lifecycle_error)
            })();
            let first_commit = match created {
                Ok(commit) => commit,
                Err(err) => {
                    if std::fs::read_dir(&path)
                        .ok()
                        .is_some_and(|mut entries| entries.next().is_none())
                        && std::fs::remove_dir(&path).is_ok()
                    {
                        let _ = choices_store::update(&app.root, None, |owner, _| {
                            if !existed {
                                owner.project_paths.retain(|p| p != &path);
                            }
                            if let Some(name) = saved.owner.project_names.get(&path) {
                                owner.project_names.insert(path.clone(), name.clone());
                            } else {
                                owner.project_names.remove(&path);
                            }
                            Ok(())
                        })
                        .map_err(choice_error);
                    }
                    return Err(err);
                }
            };
            result["path"] = path.into();
            if let Some(notice) = first_commit.notice() {
                result["notice"] = notice.into();
            }
            result["firstCommit"] = serde_json::to_value(first_commit).unwrap();
        }
        LifecycleAction::Import(request) => {
            let path = lifecycle::checked_path(&app.root, &request.path, true)
                .map_err(lifecycle_error)?
                .to_string_lossy()
                .into_owned();
            let repository = lifecycle::is_repository(&path).map_err(lifecycle_error)?;
            if !repository && request.kind.is_none() && !saved.owner.plain_folders.contains(&path) {
                return Ok(
                    json!({"ok":true,"needsChoice":true,"path":path,"choices":["git","folder"]}),
                );
            }
            if !repository && matches!(request.kind, Some(lifecycle::ImportAs::Git)) {
                lifecycle::check_git(&path, &["init", "-q"]).map_err(lifecycle_error)?;
            }
            let project = lifecycle::project_path(&path);
            choices_store::update(&app.root, None, |owner, _| {
                lifecycle::remember(owner, &path, &project);
                if !repository && !matches!(request.kind, Some(lifecycle::ImportAs::Git)) {
                    owner.plain_folders.insert(path.clone());
                } else {
                    owner.plain_folders.remove(&path);
                }
                Ok(())
            })
            .map_err(choice_error)?;
            result["path"] = path.into();
            result["needsChoice"] = false.into();
        }
        LifecycleAction::Remove(p) => {
            let project = lifecycle::project(tree, &p.project).map_err(lifecycle_error)?;
            lifecycle::checked_path(&app.root, &p.project, false).map_err(lifecycle_error)?;
            let paths: Vec<_> = saved
                .owner
                .project_paths
                .iter()
                .filter(|p| *p == &project.path || lifecycle::project_path(p) == project.path)
                .cloned()
                .collect();
            choices_store::update(&app.root, None, |owner, state| {
                lifecycle::remove_project(owner, state, project, &paths);
                Ok(())
            })
            .map_err(choice_error)?;
        }
    }
    app.directories.lock().unwrap().clear();
    app.tree_cache.lock().unwrap().discovery.forget_catalog();
    app.hub.lock().unwrap().bump();
    if let Err(err) = refresh_tree(app) {
        result["notice"] = format!(
            "Saved, but the workspace list could not refresh. {}",
            err.1.0
        )
        .into();
    }
    if let Ok(saved) = choices_store::load(&app.root, false) {
        result["revision"] = saved.revision.into();
    }
    result["version"] = app.hub.lock().unwrap().version.into();
    Ok(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionTarget {
    session: String,
    agent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendAction {
    session: String,
    agent: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyAction {
    session: String,
    agent: String,
    key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerAction {
    session: String,
    agent: String,
    prompt_id: String,
    choice: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionAction {
    ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewChat {
    cwd: String,
    role: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    account: Option<String>,
}

#[derive(Deserialize)]
enum Action {
    Send(SendAction),
    PullBack(ActionTarget),
    Interrupt(ActionTarget),
    Key(KeyAction),
    Answer(AnswerAction),
    Close(ActionTarget),
    Archive(SessionAction),
    Unarchive(SessionAction),
    New(NewChat),
    Continue(ContinueChat),
    Archived,
    Choices(choices::ChoiceWrite),
    RefreshView,
    Lifecycle(LifecycleAction),
    UsageRefresh(UsageRefresh),
    Owner(OwnerWrite),
}

struct ActionJob {
    action: Action,
    reply: oneshot::Sender<Result<Value, ApiError>>,
}

struct CliCall {
    args: Vec<String>,
    cwd: PathBuf,
    session: Option<String>,
    adapter: String,
}

impl CliCall {
    fn args(&self, args: &[&str]) -> Self {
        Self {
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cwd: self.cwd.clone(),
            session: self.session.clone(),
            adapter: self.adapter.clone(),
        }
    }
}

async fn chat_action(
    State(app): State<App>,
    uri: Uri,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|rejection| {
        let status = if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            StatusCode::PAYLOAD_TOO_LARGE
        } else {
            StatusCode::BAD_REQUEST
        };
        error(status, &rejection.body_text())
    })?;
    if !body.is_object() {
        return Err(error(StatusCode::BAD_REQUEST, "JSON object required"));
    }
    let kind = match uri.path() {
        "/api/chat/send" => "Send",
        "/api/chat/pull-back" => "PullBack",
        "/api/chat/interrupt" => "Interrupt",
        "/api/chat/key" => "Key",
        "/api/chat/answer" => "Answer",
        "/api/agent/close" => "Close",
        "/api/sessions/archive" => "Archive",
        "/api/sessions/unarchive" => "Unarchive",
        "/api/chat/new" => "New",
        _ => return Err(error(StatusCode::NOT_FOUND, "unknown action")),
    };
    let action = serde_json::from_value(json!({kind: body}))
        .map_err(|err| error(StatusCode::BAD_REQUEST, &err.to_string()))?;
    enqueue_action(&app, action).await.map(Json)
}

async fn archived_sessions(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    enqueue_action(&app, Action::Archived).await.map(Json)
}

async fn enqueue_action(app: &App, action: Action) -> Result<Value, ApiError> {
    let (reply, result) = oneshot::channel();
    app.actions
        .send(ActionJob { action, reply })
        .await
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "action coordinator stopped",
            )
        })?;
    result.await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "action coordinator failed",
        )
    })?
}

impl Action {
    fn lane(&self) -> Option<&str> {
        match self {
            Self::Send(body) => Some(&body.session),
            Self::Continue(body) => Some(&body.old_session),
            Self::Interrupt(body) | Self::Close(body) | Self::PullBack(body) => Some(&body.session),
            Self::Key(body) => Some(&body.session),
            Self::Answer(body) => Some(&body.session),
            Self::New(_)
            | Self::Archive(_)
            | Self::Unarchive(_)
            | Self::Archived
            | Self::Choices(_)
            | Self::Lifecycle(_)
            | Self::UsageRefresh(_)
            | Self::Owner(_)
            | Self::RefreshView => None,
        }
    }
}

fn start_action_queue(app: &App, mut queue: mpsc::Receiver<ActionJob>) {
    let weak = Arc::downgrade(app);
    tokio::spawn(async move {
        let mut pending: VecDeque<(Option<String>, ActionJob)> = VecDeque::new();
        let mut active = HashSet::new();
        let mut workers = tokio::task::JoinSet::new();
        let mut accepting = true;
        loop {
            let mut index = 0;
            while index < pending.len() {
                if active.contains(&pending[index].0) {
                    index += 1;
                    continue;
                }
                let (lane, job) = pending.remove(index).unwrap();
                let Some(app) = weak.upgrade() else { return };
                active.insert(lane.clone());
                workers.spawn(async move {
                    // An accepted action continues if its HTTP connection closes.
                    let changed = !matches!(
                        job.action,
                        Action::Archived
                            | Action::RefreshView
                            | Action::Choices(_)
                            | Action::Lifecycle(_)
                            | Action::UsageRefresh(_)
                            | Action::Owner(_)
                    );
                    let result = execute_action(&app, job.action).await;
                    if changed {
                        app.hub.lock().unwrap().bump();
                    }
                    let _ = job.reply.send(result);
                    lane
                });
            }
            if !accepting && workers.is_empty() {
                break;
            }
            tokio::select! {
                // Bound queued and running work; keep only lanes with active jobs in memory.
                job = queue.recv(), if accepting && pending.len() + active.len() < 64 => {
                    match job {
                        Some(job) => pending.push_back((job.action.lane().map(str::to_owned), job)),
                        None => accepting = false,
                    }
                }
                result = workers.join_next(), if !workers.is_empty() => {
                    match result {
                        Some(Ok(lane)) => { active.remove(&lane); }
                        // A failed owner stops all lanes rather than retrying uncertain mutations.
                        _ => break,
                    }
                }
            }
        }
    });
}

fn action_session(app: &Server, session: &str) -> Result<CliCall, ApiError> {
    let connection = read_connection(&app.root)
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let row = connection
        .query_row(
            "SELECT cwd, adapter FROM session WHERE id = ?1",
            [session],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let (cwd, adapter) = row.ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown session"))?;
    Ok(CliCall {
        args: Vec::new(),
        cwd: cwd.into(),
        session: Some(session.to_owned()),
        adapter: adapter.unwrap_or_default(),
    })
}

fn action_target(app: &Server, session: &str, agent: &str) -> Result<(CliCall, String), ApiError> {
    let call = action_session(app, session)?;
    let agent = if agent == "chair" {
        "orchestrator"
    } else {
        agent
    };
    let connection = read_connection(&app.root)
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let exists = connection
        .query_row(
            "SELECT 1 FROM agent WHERE session_id = ?1 AND id = ?2",
            (session, agent),
            |_| Ok(()),
        )
        .optional()
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?
        .is_some();
    if !exists {
        return Err(error(StatusCode::NOT_FOUND, "unknown session or agent"));
    }
    if call.adapter.trim().is_empty() {
        return Err(error(
            StatusCode::CONFLICT,
            "This session does not record an adapter, so Swarm cannot reach its panes.",
        ));
    }
    Ok((call, agent.to_owned()))
}

async fn execute_action(app: &App, action: Action) -> Result<Value, ApiError> {
    let default_call = || CliCall {
        args: Vec::new(),
        cwd: app.root.clone(),
        session: None,
        adapter: "tmux-solo".into(),
    };
    let interrupt = matches!(action, Action::Interrupt(_));
    let archive = matches!(action, Action::Archive(_));
    let (call, text) = match action {
        Action::Send(body) => {
            if body.text.trim().is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "swarm: empty text"));
            }
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            let text = if action_provider(app, &body.session, &agent)?.as_deref() == Some("claude")
            {
                claude_text::cleaned(&body.text)
            } else {
                body.text
            };
            if text.trim().is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "swarm: empty text"));
            }
            (call.args(&["type", &agent]), Some(text))
        }
        Action::Interrupt(body) | Action::Close(body) => {
            let verb = if interrupt { "interrupt" } else { "close" };
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            (call.args(&[verb, &agent]), None)
        }
        Action::Key(body) => {
            if !matches!(body.key.as_str(), "Up" | "C-u") {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "only Up and C-u are allowed",
                ));
            }
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            (call.args(&["key", &agent, &body.key]), None)
        }
        Action::Answer(body) => {
            if body.prompt_id.trim().is_empty() || body.prompt_id.contains('\0') {
                return Err(error(StatusCode::BAD_REQUEST, "prompt_id required"));
            }
            let (call, agent) = action_target(app, &body.session, &body.agent)?;
            (
                call.args(&["answer", &agent, &body.prompt_id, &body.choice.to_string()]),
                None,
            )
        }
        Action::Archive(body) | Action::Unarchive(body) => {
            let verb = if archive { "archive" } else { "unarchive" };
            if body.ids.is_empty() {
                return Err(error(StatusCode::BAD_REQUEST, "session ids required"));
            }
            for id in &body.ids {
                action_session(app, id)?;
            }
            let mut call = default_call().args(&["session", verb]);
            call.args.extend(body.ids);
            (call, None)
        }
        Action::Choices(body) => {
            let app = app.clone();
            return tokio::task::spawn_blocking(move || apply_choice(&app, body))
                .await
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "choice task failed"))?;
        }
        Action::Lifecycle(action) => return execute_lifecycle(app, action).await,
        Action::UsageRefresh(body) => return refresh_usage(app, &body.provider).await,
        Action::Owner(write) => return run_owner_write(app, write).await,
        Action::RefreshView => {
            let app = app.clone();
            return tokio::task::spawn_blocking(move || refresh_tree(&app))
                .await
                .map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "tree refresh task failed",
                    )
                })?;
        }
        Action::PullBack(body) => return pull_back(app, body).await,
        Action::New(body) => return new_chat(app, body).await,
        Action::Continue(body) => return execute_continue(app, body).await,
        Action::Archived => {
            let output = run_cli(
                app,
                default_call().args(&["sessions", "--json", "--archived"]),
                None,
            )
            .await?;
            let mut value: Value = serde_json::from_str(&output).map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "swarm returned invalid JSON",
                )
            })?;
            let sessions = value["sessions"].as_array_mut().ok_or_else(|| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "swarm returned invalid sessions",
                )
            })?;
            sessions.retain(|session| !session["archivedAt"].is_null());
            value["ok"] = json!(true);
            return Ok(value);
        }
    };
    run_cli(app, call, text).await?;
    Ok(json!({"ok": true}))
}

fn action_provider(app: &Server, session: &str, agent: &str) -> Result<Option<String>, ApiError> {
    let connection = read_connection(&app.root)
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let result = if agent == "orchestrator" || agent == "chair" {
        connection.query_row(
            "SELECT chair_provider FROM session WHERE id = ?1",
            [session],
            |row| row.get(0),
        )
    } else {
        connection.query_row(
            "SELECT provider FROM agent WHERE session_id = ?1 AND id = ?2",
            (session, agent),
            |row| row.get(0),
        )
    };
    result.map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))
}

async fn pull_back(app: &App, body: ActionTarget) -> Result<Value, ApiError> {
    let (call, agent) = action_target(app, &body.session, &body.agent)?;
    let provider = action_provider(app, &body.session, &agent)?;
    if !queue::can_pull_back(provider.as_deref(), &call.adapter) {
        return Err(error(
            StatusCode::CONFLICT,
            "Pull-back needs Claude and a tmux adapter.",
        ));
    }
    let log_agent = if agent == "orchestrator" {
        "chair"
    } else {
        &agent
    };
    let log = resolve_log(&app.root, &body.session, log_agent)?;
    let (_, mut end) = row_offsets(&log.path, 1, None)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript read failed"))?;
    let state = read_row_window(&app.tool, &log, 0, end).await?;
    let pending = queue::replay(state.records.values().map(|record| &record.event));
    let Some(last) = pending.iter().rfind(|text| queue::is_owner(text)) else {
        return Ok(json!({"ok":true,"outcome":"alreadySent","text":null}));
    };
    if !pending.iter().all(|text| queue::is_owner(text)) {
        return Err(error(
            StatusCode::CONFLICT,
            "The agent has its own message in the queue. Edit after it is delivered.",
        ));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_millis(1500);
    run_cli(app, call.args(&["key", &agent, "Up"]), None).await?;
    let mut after = Vec::new();
    let decision = loop {
        let (_, next) = row_offsets(&log.path, 1, None)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript read failed"))?;
        if next < end {
            break queue::PullBack::Unconfirmed;
        }
        if next > end {
            match tokio::time::timeout_at(deadline, read_row_window(&app.tool, &log, end, next))
                .await
            {
                Ok(Ok(state)) => {
                    after.extend(state.records.into_values().map(|record| record.event));
                    end = next;
                }
                _ => break queue::PullBack::Unconfirmed,
            }
        }
        let decision =
            queue::PullBack::decide(&pending, &after, tokio::time::Instant::now() >= deadline);
        if decision != queue::PullBack::Waiting {
            break decision;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    for index in 0..decision.clear_presses(last) {
        if index > 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        run_cli(app, call.args(&["key", &agent, "C-u"]), None).await?;
    }
    let (outcome, text) = match decision {
        queue::PullBack::Pulled { owner, .. } => ("pulled", Some(owner.join("\n"))),
        queue::PullBack::AlreadySent { .. } => ("alreadySent", None),
        queue::PullBack::Unconfirmed | queue::PullBack::Waiting => ("unconfirmed", None),
    };
    Ok(json!({"ok":true,"outcome":outcome,"text":text}))
}

async fn new_chat(app: &App, body: NewChat) -> Result<Value, ApiError> {
    let role = body.role.as_deref().unwrap_or("chat");
    let account = body.account.as_deref().or(if body.provider.is_none() {
        Some("auto")
    } else {
        None
    });
    for value in [
        Some(role),
        body.provider.as_deref(),
        body.model.as_deref(),
        account,
    ]
    .into_iter()
    .flatten()
    {
        if !swarm::config::valid_name(value) {
            return Err(error(StatusCode::BAD_REQUEST, "invalid launch option"));
        }
    }
    if body.cwd.contains('\0') || !Path::new(&body.cwd).is_absolute() {
        return Err(error(StatusCode::BAD_REQUEST, "absolute cwd required"));
    }
    let snapshot_app = app.clone();
    let allowed_cwd = body.cwd.clone();
    tokio::task::spawn_blocking(move || {
        let (tree, saved) = fresh_lifecycle_tree(&snapshot_app)?;
        let workspace = tree.workspace(&allowed_cwd).map(|(_, w)| w);
        if workspace.is_none() && !tree.projects.iter().any(|p| p.path == allowed_cwd) {
            return Err(error(
                StatusCode::FORBIDDEN,
                "cwd is outside the workspace tree",
            ));
        }
        lifecycle::checked_path(&snapshot_app.root, &allowed_cwd, true).map_err(lifecycle_error)?;
        if workspace.is_some_and(|w| w.missing || w.removed)
            || saved.owner.archived.contains(&allowed_cwd)
        {
            return Err(error(
                StatusCode::CONFLICT,
                "This workspace is read-only. Restore it to start a chat.",
            ));
        }
        Ok(())
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "cwd check failed"))??;
    let mut call = CliCall {
        args: Vec::new(),
        cwd: body.cwd.into(),
        session: None,
        adapter: "tmux-solo".into(),
    };
    let create = app.chat_create.lock().await;
    run_cli(app, call.args(&["init"]), None).await?;
    let output = run_cli(app, call.args(&["session", "new", "lane"]), None).await?;
    let session = output.trim();
    if uuid::Uuid::parse_str(session).is_err() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "swarm returned an invalid session id",
        ));
    }
    call.session = Some(session.to_owned());
    drop(create);
    call.args = vec!["launch".into(), "orchestrator".into(), role.to_owned()];
    for (flag, value) in [
        ("--provider", body.provider.as_deref()),
        ("--model", body.model.as_deref()),
        ("--account", account),
    ] {
        if let Some(value) = value {
            call.args.extend([flag.to_owned(), value.to_owned()]);
        }
    }
    let output = run_cli(app, call, None).await?;
    let pane = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "swarm returned no pane"))?;
    Ok(json!({"ok": true, "session": session, "agent": "orchestrator", "pane": pane}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContinueChat {
    old_session: String,
    provider: String,
    model: String,
    account: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelContinue {
    old_session: String,
}
struct HandoffRun {
    phase: &'static str,
    cancel: watch::Sender<bool>,
}
fn register_continue(app: &App, body: &ContinueChat) -> Result<(), ApiError> {
    if !matches!(body.provider.as_str(), "claude" | "codex") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Model switching supports Claude and Codex chats",
        ));
    }
    if !swarm::config::valid_name(&body.model)
        || body
            .account
            .as_deref()
            .is_some_and(|a| !swarm::config::valid_name(a))
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid launch option"));
    }
    action_session(app, &body.old_session)?;
    let mut runs = app.handoffs.lock().unwrap();
    if runs.contains_key(&body.old_session) {
        return Err(error(
            StatusCode::CONFLICT,
            "A model switch is already running for this chat",
        ));
    }
    if runs.len() >= 64 {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Too many model switches",
        ));
    }
    let (cancel, _) = watch::channel(false);
    runs.insert(
        body.old_session.clone(),
        HandoffRun {
            phase: "preparing",
            cancel,
        },
    );
    Ok(())
}
async fn continue_chat(
    State(app): State<App>,
    body: Result<Json<ContinueChat>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                e.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    register_continue(&app, &body)?;
    let old = body.old_session.clone();
    let result = enqueue_action(&app, Action::Continue(body)).await;
    // Execute owns cleanup after acceptance, even when the browser disconnects.
    if result
        .as_ref()
        .err()
        .is_some_and(|e| e.0 == StatusCode::SERVICE_UNAVAILABLE)
    {
        app.handoffs.lock().unwrap().remove(&old);
    }
    result.map(Json)
}
async fn cancel_continue(
    State(app): State<App>,
    body: Result<Json<CancelContinue>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| {
        error(
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                e.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            &e.body_text(),
        )
    })?;
    let runs = app.handoffs.lock().unwrap();
    let run = runs
        .get(&body.old_session)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "no model switch for this chat"))?;
    if !matches!(run.phase, "preparing" | "summarizing") {
        return Err(error(
            StatusCode::CONFLICT,
            "The new agent is starting; wait for the switch to finish",
        ));
    }
    run.cancel.send_replace(true);
    Ok(Json(json!({"ok":true})))
}
fn continue_phase(app: &App, old: &str, phase: &'static str) -> Result<(), ApiError> {
    let mut runs = app.handoffs.lock().unwrap();
    let run = runs
        .get_mut(old)
        .ok_or_else(|| error(StatusCode::CONFLICT, "model switch cancelled"))?;
    if *run.cancel.borrow() {
        return Err(error(StatusCode::CONFLICT, "model switch cancelled"));
    }
    run.phase = phase;
    app.hub.lock().unwrap().publish(Frame::new("handoff", json!({"old_session":old,"phase":phase,"canCancel":matches!(phase,"preparing"|"summarizing")}), None));
    Ok(())
}
async fn handoff_rows(app: &App, old: &str) -> Result<Vec<Row>, ApiError> {
    let log = resolve_log(&app.root, old, "chair").map_err(|_| {
        error(
            StatusCode::CONFLICT,
            "The current chat has no readable transcript to carry forward",
        )
    })?;
    let end = row_offsets(&log.path, 1, None)
        .map_err(|e| error(StatusCode::CONFLICT, &e.to_string()))?
        .1;
    let state = read_row_window(&app.tool, &log, 0, end).await?;
    Ok(view::rows(
        &state.records.values().cloned().collect::<Vec<_>>(),
        false,
        state.is_codex,
    ))
}

async fn execute_continue(app: &App, body: ContinueChat) -> Result<Value, ApiError> {
    execute_continue_timed(
        app,
        body,
        Duration::from_secs(90),
        Duration::from_secs(30),
        Duration::from_secs(1),
    )
    .await
}
async fn execute_continue_timed(
    app: &App,
    body: ContinueChat,
    summary_wait: Duration,
    ready_wait: Duration,
    poll: Duration,
) -> Result<Value, ApiError> {
    let old = &body.old_session;
    let result = async {
        let mut cancelled = app.handoffs.lock().unwrap().get(old).ok_or_else(|| error(StatusCode::CONFLICT, "model switch cancelled"))?.cancel.subscribe();
        if *cancelled.borrow() { return Err(error(StatusCode::CONFLICT, "model switch cancelled")); }
        let prepare = async {
            continue_phase(app, old, "preparing")?;
            let rows = handoff_rows(app, old).await?;
            let call = action_session(app, old)?;
            let output = run_cli(app, call.args(&["agents", "--json"]), None).await?;
            let agents: Value = serde_json::from_str(&output).map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid agents JSON from swarm"))?;
            let agents = agents["agents"].as_array().ok_or_else(|| error(StatusCode::BAD_GATEWAY, "missing agents list from swarm"))?;
            let alive = agents.iter().any(|a| a["id"] == "orchestrator" && a["alive"] == true);
            if alive && view::is_active(&rows) {
                return Err(error(StatusCode::CONFLICT, "Wait for the current reply to finish, or stop it before switching model."));
            }
            if alive {
                continue_phase(app, old, "summarizing")?;
                run_cli(app, call.args(&["type", "orchestrator"]), Some(handoff::REQUEST.into())).await?;
                let deadline = tokio::time::Instant::now() + summary_wait;
                loop {
                    let summary = tokio::time::timeout_at(deadline, async {
                        tokio::time::sleep(poll).await;
                        handoff_rows(app, old).await
                    }).await;
                    match summary {
                        Ok(Ok(new_rows)) => if let Some(context) = handoff::completed_summary(&new_rows, rows.len()) { return Ok((call, context, true)); },
                        Ok(Err(_)) => {}, // Swift keeps waiting if a summary poll cannot read the log.
                        Err(_) => break,
                    }
                }
            }
            let context = handoff::recent_context(&rows).ok_or_else(|| error(StatusCode::CONFLICT, "The current chat has no messages to carry forward"))?;
            Ok((call, context, false))
        };
        let (call, context, summary) = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(error(StatusCode::CONFLICT, "model switch cancelled")),
            prepared = prepare => prepared?,
        };
        continue_phase(app, old, "starting")?;
        let started = new_chat(app, NewChat { cwd: call.cwd.to_string_lossy().into_owned(), role: None, provider: Some(body.provider.clone()), model: Some(body.model), account: body.account }).await?;
        let session = started["session"].as_str().unwrap();
        continue_phase(app, old, "waiting")?;
        let deadline = tokio::time::Instant::now() + ready_wait;
        let ready = tokio::time::timeout_at(deadline, async {
            loop {
                tokio::time::sleep(poll).await;
                let output = run_cli(app, call.args(&["sessions", "--json"]), None).await?;
                let listing: Value = serde_json::from_str(&output).map_err(|_| error(StatusCode::BAD_GATEWAY, "invalid sessions JSON from swarm"))?;
                let sessions = listing["sessions"].as_array().ok_or_else(|| error(StatusCode::BAD_GATEWAY, "missing sessions list from swarm"))?;
                if sessions.iter().any(|s| s["id"] == session && handoff::ready(s, &body.provider)) { return Ok::<_, ApiError>(()); }
            }
        }).await;
        ready.map_err(|_| error(StatusCode::GATEWAY_TIMEOUT, "The new agent did not become ready to receive the summary"))??;
        continue_phase(app, old, "delivering")?;
        let new_call = CliCall { args: vec![], cwd: call.cwd.clone(), session: Some(session.into()), adapter: "tmux-solo".into() };
        run_cli(app, new_call.args(&["type", "orchestrator"]), Some(handoff::first_message(&context, summary))).await?;
        run_cli(app, call.args(&["session", "continue", session, old]), None).await?;
        Ok(started)
    }.await;
    let (phase, session, message) = match &result {
        Ok(value) => (
            "completed",
            value.get("session").cloned().unwrap_or(Value::Null),
            Value::Null,
        ),
        Err(e) => (
            if e.1.0["error"] == "model switch cancelled" {
                "cancelled"
            } else {
                "failed"
            },
            Value::Null,
            e.1.0["error"].clone(),
        ),
    };
    app.hub.lock().unwrap().publish(Frame::new("handoff", json!({"old_session":old,"phase":phase,"canCancel":false,"session":session,"error":message}), None));
    app.handoffs.lock().unwrap().remove(old);
    result
}

async fn run_cli(app: &Server, call: CliCall, text: Option<String>) -> Result<String, ApiError> {
    let timeout = if call.args.first().is_some_and(|arg| arg == "launch") {
        60
    } else {
        20
    };
    run_cli_with_timeout(app, call, text, Duration::from_secs(timeout)).await
}

async fn run_cli_with_timeout(
    app: &Server,
    call: CliCall,
    text: Option<String>,
    timeout: Duration,
) -> Result<String, ApiError> {
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new(&app.cli);
    // Match ChildProcessEnvironment.swarmCall so key and answer remain owner actions even when
    // serve was opened from an agent pane. Identity never comes from the HTTP request.
    for (name, _) in std::env::vars_os() {
        let key = name.to_string_lossy();
        if [
            "CLAUDECODE",
            "CLAUDE_PID",
            "AI_AGENT",
            "SWARM_SESSION_ID",
            "SWARM_AGENT_ID",
            "SWARM_ADAPTER",
            "SWARM_PANE",
            "TMUX_PANE",
            "SUPERSET_TERMINAL_ID",
        ]
        .contains(&key.as_ref())
            || key.starts_with("HERDR_")
            || key.starts_with("CLAUDE_CODE_")
        {
            command.env_remove(name);
        }
    }
    command
        .args(&call.args)
        .current_dir(&call.cwd)
        .env("PWD", &call.cwd)
        .env("SWARM_HOME", app.root.parent().expect("swarm home"))
        .env("SWARM_ADAPTER", &call.adapter)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    if let Some(session) = &call.session {
        command
            .env("SWARM_SESSION_ID", session)
            .env("SWARM_AGENT_ID", "orchestrator");
    }
    let mut child = command
        .spawn()
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let mut group = ActionProcessGroup(Some(child.id().expect("running child") as i32));
    let result = tokio::time::timeout(timeout, async {
        let mut stdin = child.stdin.take().expect("piped stdin");
        if let Some(text) = text {
            stdin.write_all(text.as_bytes()).await?;
        }
        drop(stdin);
        child.wait_with_output().await
    })
    .await;
    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Err(error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()));
        }
        Err(_) => {
            return Err(error(StatusCode::GATEWAY_TIMEOUT, "swarm action timed out"));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(error(
            StatusCode::CONFLICT,
            if message.is_empty() {
                "swarm action failed"
            } else {
                message
            },
        ));
    }
    // Launch can create a detached pane. Only failed or cancelled calls stop their process group.
    group.0 = None;
    Ok(stdout)
}

struct ActionProcessGroup(Option<i32>);

impl Drop for ActionProcessGroup {
    fn drop(&mut self) {
        if let Some(group) = self.0 {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            // SAFETY: the child has its own process group; kill takes plain integers only.
            unsafe {
                kill(-group, 9);
            }
        }
    }
}

fn start_coordinator(app: &App, action_queue: mpsc::Receiver<ActionJob>) {
    start_action_queue(app, action_queue);
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
        let mut last_tree = None;
        loop {
            let Some(app) = weak.upgrade() else {
                break;
            };
            if app.stopped.load(Ordering::Relaxed) {
                break;
            }
            let at = discovery::now();
            if last_tree.is_none_or(|last| at - last >= 2) {
                let (reply, _) = oneshot::channel();
                if app
                    .actions
                    .try_send(ActionJob {
                        action: Action::RefreshView,
                        reply,
                    })
                    .is_ok()
                {
                    last_tree = Some(at);
                }
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

type Position = (u64, usize);
const ROW_RECORD_LIMIT: usize = 2000;

#[derive(Clone)]
struct LiveChunk {
    records: Vec<Position>,
    rows: Vec<Row>,
    context: (bool, bool),
}
#[derive(Clone)]
struct LiveRows {
    records: BTreeMap<Position, Record>,
    start: u64,
    end: u64,
    is_codex: bool,
    shown: HashSet<String>,
    last_rows: Vec<Row>,
    last_usage: Option<view::ChatUsage>,
    usage: view::UsageState,
    chunks: BTreeMap<usize, LiveChunk>,
    record_chunks: HashMap<Position, usize>,
    dirty: HashSet<usize>,
    rebuild: bool,
    initialized: bool,
    context: (bool, bool),
    contexts: HashMap<Position, (bool, bool)>,
    scope_start: Position,
    saw_tool: bool,
    anchors: HashMap<String, Vec<Position>>,
    unmatched: HashSet<String>,
    shell_outputs: HashSet<Position>,
    next_chunk: usize,
    tail_steps: usize,
    last_prose: Option<(view::RowKind, String)>,
    clear_row: Option<Row>,
    pending_queue: Vec<Record>,
    queue_cursor: Option<Position>,
    last_queued: Vec<Row>,
    queue_dirty: bool,
}
impl LiveRows {
    fn empty(from: u64, is_codex: bool) -> Self {
        Self {
            records: BTreeMap::new(),
            start: from,
            end: from,
            is_codex,
            shown: HashSet::new(),
            last_rows: Vec::new(),
            last_usage: None,
            usage: view::UsageState::default(),
            chunks: BTreeMap::new(),
            record_chunks: HashMap::new(),
            dirty: HashSet::new(),
            rebuild: false,
            initialized: false,
            context: (from == 0, from > 0),
            contexts: HashMap::new(),
            scope_start: (from, 0),
            saw_tool: false,
            anchors: HashMap::new(),
            unmatched: HashSet::new(),
            shell_outputs: HashSet::new(),
            next_chunk: 0,
            tail_steps: 0,
            last_prose: None,
            clear_row: None,
            pending_queue: Vec::new(),
            queue_cursor: None,
            last_queued: Vec::new(),
            queue_dirty: false,
        }
    }
    fn ingest(&mut self, start: u64, end: u64, events: &[Value]) {
        self.start = self.start.min(start);
        self.end = self.end.max(end);
        for (index, event) in events.iter().enumerate() {
            let position = (start, index);
            if self.records.contains_key(&position) {
                continue;
            }
            let record = Record::new(event.clone(), format!("raw-{start}-{index}"), self.is_codex);
            self.usage.ingest(event);
            self.contexts.insert(position, self.context);
            self.records.insert(position, record);
            self.update_queue(position);
            self.append(position);
        }
    }
    fn update_queue(&mut self, key: Position) {
        let record = &self.records[&key];
        let event = &record.event;
        if event["type"] != "queue_operation"
            || self.queue_cursor.is_some_and(|cursor| key <= cursor)
        {
            return;
        }
        self.queue_cursor = Some(key);
        self.queue_dirty = true;
        match view::string(event, "operation") {
            "enqueue" if event["content"].is_string() => self.pending_queue.push(record.clone()),
            "dequeue" if !self.pending_queue.is_empty() => {
                self.pending_queue.remove(0);
            }
            "remove" | "popAll" => {
                if let Some(index) = self
                    .pending_queue
                    .iter()
                    .position(|record| record.event["content"] == event["content"])
                {
                    self.pending_queue.remove(index);
                }
            }
            _ => (),
        }
    }
    fn append(&mut self, position: Position) {
        let event = &self.records[&position].event;
        let event_type = view::string(event, "type");
        if matches!(event_type, "page" | "turn_started")
            || (event_type == "user_message_chunk" && self.saw_tool)
        {
            self.scope_start = position;
            self.saw_tool = false;
            self.anchors.clear();
            self.unmatched.clear();
            self.shell_outputs.clear();
        }
        if matches!(event_type, "turn_started" | "turn_ended") {
            self.context.0 = true;
        }
        let mut row = view::row(&self.records[&position]);
        if let Some(row) = &mut row {
            if self.is_codex
                && (row.kind == view::RowKind::User
                    || row.system_kind.as_deref() == Some("swarm_ring"))
            {
                if row.kind == view::RowKind::User && !self.context.0 {
                    row.arrives_mid_turn = true;
                }
                self.context.0 = false;
            }
            if row.system_kind.as_deref() == Some("swarm_ring") && !self.context.1 {
                row.starts_turn = true;
            }
            if row.opens_turn() {
                self.context.1 = true;
            }
            if row.ends_turn {
                self.context.1 = false;
            }
            row.hidden = view::is_hidden(row);
        }
        let reference = match event_type {
            "tool_call_update" | "tool_diff" => {
                Some(format!("tool:{}", view::string(event, "tool_call_id")))
            }
            "system_message" => match view::string(event, "kind") {
                "shell_output" => Some(format!(
                    "shell:{}",
                    event["meta"]["parent_uuid"].as_str().unwrap_or("")
                )),
                "command_output" | "skill_body" | "injected" => {
                    let kind = view::string(event, "kind");
                    let text = view::string(event, "text").trim_start();
                    if kind == "injected"
                        && ["<system-reminder>", "<local-command-caveat>"]
                            .iter()
                            .any(|prefix| text.starts_with(prefix))
                    {
                        None
                    } else {
                        let command = format!(
                            "command:{}",
                            event["meta"]["parent_uuid"].as_str().unwrap_or("")
                        );
                        if self.anchors.contains_key(&command) {
                            Some(command)
                        } else if kind != "command_output" {
                            let tool = format!(
                                "tool:{}",
                                event["meta"]["source_tool_use_id"].as_str().unwrap_or("")
                            );
                            if kind != "injected"
                                || self.anchors.get(&tool).is_some_and(|anchors| {
                                    anchors.iter().any(|key| {
                                        view::string(&self.records[key].event, "name") == "Skill"
                                    })
                                })
                            {
                                Some(tool)
                            } else {
                                None
                            }
                        } else {
                            Some(command)
                        }
                    }
                }
                _ => None,
            },
            _ => None,
        };
        let reference =
            reference.filter(|id| id.split_once(':').is_some_and(|(_, id)| !id.is_empty()));
        let target = reference
            .as_ref()
            .and_then(|id| self.anchors.get(id))
            .and_then(|anchors| {
                let session = event["meta"]["session_id"].as_str().unwrap_or("");
                let mut matches = anchors.iter().filter(|key| {
                    let anchor = self.records[key].event["meta"]["session_id"]
                        .as_str()
                        .unwrap_or("");
                    (session.is_empty() || anchor.is_empty() || session == anchor)
                        && (view::string(event, "kind") != "injected"
                            || !reference.as_ref().is_some_and(|id| id.starts_with("tool:"))
                            || view::string(&self.records[key].event, "name") == "Skill")
                });
                let first = matches.next()?;
                matches.next().is_none().then_some(*first)
            });
        let target = target.filter(|key| {
            view::string(event, "kind") != "shell_output" || !self.shell_outputs.contains(key)
        });
        if view::string(event, "kind") == "shell_output"
            && let Some(key) = target
        {
            self.shell_outputs.insert(key);
        }
        if target.is_none()
            && let Some(reference) = &reference
        {
            self.unmatched.insert(reference.clone());
        }
        let anchor = match event_type {
            "tool_call" => Some(format!("tool:{}", view::string(event, "tool_call_id"))),
            "system_message" if view::string(event, "kind") == "shell_input" => Some(format!(
                "shell:{}",
                event["meta"]["uuid"].as_str().unwrap_or("")
            )),
            "system_message" if row.as_ref().is_some_and(|row| row.command.is_some()) => Some(
                format!("command:{}", event["meta"]["uuid"].as_str().unwrap_or("")),
            ),
            _ => None,
        };
        if let Some(anchor) = anchor {
            if self.unmatched.contains(&anchor) {
                self.rebuild = true;
            }
            let anchors = self.anchors.entry(anchor).or_default();
            if !anchors.is_empty() {
                self.rebuild = true;
            }
            anchors.push(position);
        }
        if matches!(event_type, "tool_call" | "tool_call_update" | "tool_diff") {
            self.saw_tool = true;
        }
        if event_type == "turn_ended" {
            self.rebuild = true;
            self.scope_start = position;
            self.saw_tool = false;
            self.anchors.clear();
            self.unmatched.clear();
            self.shell_outputs.clear();
        }
        if matches!(view::string(event, "kind"), "skill_body" | "injected")
            && target.is_some_and(|key| {
                view::string(&self.records[&key].event, "type") == "system_message"
            })
        {
            self.context.1 = true;
        }
        if !self.initialized {
            return;
        }
        if let Some(target) = target.and_then(|key| self.record_chunks.get(&key).copied()) {
            self.chunks.get_mut(&target).unwrap().records.push(position);
            self.record_chunks.insert(position, target);
            self.dirty.insert(target);
            self.last_prose = None;
            return;
        }
        let Some(row) = row else {
            if matches!(event_type, "turn_started" | "page")
                && let Some((&id, _)) = self.chunks.last_key_value()
            {
                self.chunks.get_mut(&id).unwrap().records.push(position);
                self.record_chunks.insert(position, id);
                self.dirty.insert(id);
            }
            return;
        };
        let step = swarm::view::is_step(&row) || event_type == "tool_call";
        let prose = matches!(
            row.kind,
            view::RowKind::User | view::RowKind::Assistant | view::RowKind::Thought
        )
        .then(|| (row.kind, row.id.clone()));
        let merges = prose.is_some() && self.last_prose == prose;
        let new_chunk = self.chunks.is_empty()
            || (!merges
                && (!step
                    || self.tail_steps == 0
                    || self.tail_steps == swarm::view::FOLD_STEP_LIMIT));
        if new_chunk {
            let id = self.next_chunk;
            self.next_chunk += 1;
            self.chunks.insert(
                id,
                LiveChunk {
                    records: Vec::new(),
                    rows: Vec::new(),
                    context: self.contexts[&position],
                },
            );
            self.tail_steps = 0;
        }
        let id = *self.chunks.last_key_value().unwrap().0;
        self.chunks.get_mut(&id).unwrap().records.push(position);
        self.record_chunks.insert(position, id);
        self.dirty.insert(id);
        if step {
            if !merges {
                self.tail_steps += 1;
            }
        } else {
            self.tail_steps = 0;
        }
        self.last_prose = prose;
    }
    fn rows(&self) -> Vec<Row> {
        if !self.initialized || self.rebuild {
            return self.rows_full();
        }
        self.chunks
            .values()
            .flat_map(|chunk| chunk.rows.clone())
            .chain(self.last_queued.clone())
            .collect()
    }
    fn remember_shown(&mut self, rows: &[Row]) {
        for row in rows {
            if row.kind != view::RowKind::Fold {
                self.shown.insert(row.id.clone());
            } else if row.default_open {
                self.shown
                    .extend(row.steps.iter().map(|row| row.id.clone()));
            }
        }
    }
    fn prepare(&mut self, rows: Vec<Row>) {
        self.clear_row = rows
            .iter()
            .find(|row| row.kind == view::RowKind::Divider)
            .cloned();
        self.chunks.clear();
        self.record_chunks.clear();
        self.dirty.clear();
        self.next_chunk = 0;
        for queued in &self.last_queued {
            if !rows.iter().any(|row| row.id == queued.id) {
                self.shown.remove(&queued.id);
            }
        }
        self.last_queued = rows
            .iter()
            .filter(|row| row.kind == view::RowKind::Queued)
            .cloned()
            .collect();
        for row in rows
            .into_iter()
            .filter(|row| row.kind != view::RowKind::Queued)
        {
            let step = swarm::view::is_step(&row);
            let can_append = step
                && row.kind != view::RowKind::Fold
                && self.chunks.last_key_value().is_some_and(|(_, chunk)| {
                    chunk.rows.iter().all(swarm::view::is_step)
                        && chunk
                            .rows
                            .iter()
                            .map(|row| {
                                if row.kind == view::RowKind::Fold {
                                    row.steps.len()
                                } else {
                                    1
                                }
                            })
                            .sum::<usize>()
                            < swarm::view::FOLD_STEP_LIMIT
                });
            let id = if can_append {
                *self.chunks.last_key_value().unwrap().0
            } else {
                let id = self.next_chunk;
                self.next_chunk += 1;
                self.chunks.insert(
                    id,
                    LiveChunk {
                        records: Vec::new(),
                        rows: Vec::new(),
                        context: (self.start == 0, self.start > 0),
                    },
                );
                id
            };
            let mut records: Vec<_> = row
                .source_ids
                .iter()
                .filter_map(|source| {
                    let mut pieces = source.strip_prefix("raw-")?.split('-');
                    Some((pieces.next()?.parse().ok()?, pieces.next()?.parse().ok()?))
                })
                .collect();
            if let (Some(&first), Some(&last)) = (records.iter().min(), records.iter().max()) {
                records.extend(
                    self.records
                        .range(first..=last)
                        .filter(|(_, record)| {
                            matches!(
                                view::string(&record.event, "type"),
                                "page" | "turn_started" | "turn_ended"
                            )
                        })
                        .map(|(key, _)| *key),
                );
                records.sort_unstable();
                records.dedup();
            }
            for key in &records {
                self.record_chunks.insert(*key, id);
            }
            let chunk = self.chunks.get_mut(&id).unwrap();
            if chunk.records.is_empty() {
                chunk.context = records
                    .first()
                    .and_then(|key| self.contexts.get(key))
                    .copied()
                    .unwrap_or((self.start == 0, self.start > 0));
            }
            chunk.records.extend(records);
            chunk.rows.push(row);
        }
        self.tail_steps = self.chunks.last_key_value().map_or(0, |(_, chunk)| {
            if chunk
                .rows
                .iter()
                .all(|row| row.kind == view::RowKind::Fold || swarm::view::is_step(row))
            {
                chunk
                    .rows
                    .iter()
                    .map(|row| {
                        if row.kind == view::RowKind::Fold {
                            row.steps.len()
                        } else {
                            1
                        }
                    })
                    .sum()
            } else {
                0
            }
        });
        self.last_prose = self
            .records
            .values()
            .rev()
            .find_map(view::row)
            .and_then(|row| {
                matches!(
                    row.kind,
                    view::RowKind::User | view::RowKind::Assistant | view::RowKind::Thought
                )
                .then_some((row.kind, row.id))
            });
        self.initialized = true;
        self.rebuild = false;
        self.queue_dirty = false;
    }
    fn compact(&mut self) {
        // shortcut: one open turn can exceed 2000 records; spill it only if its memory becomes a problem.
        while self.records.len() > ROW_RECORD_LIMIT {
            let Some((&key, _)) = self.records.first_key_value() else {
                break;
            };
            if key >= self.scope_start {
                break;
            }
            if let Some(id) = self.record_chunks.get(&key).copied() {
                let chunk = &self.chunks[&id];
                if chunk.records.iter().any(|key| *key >= self.scope_start) {
                    break;
                }
                let chunk = self.chunks.remove(&id).unwrap();
                for row in &chunk.rows {
                    self.shown.remove(&row.id);
                    for step in &row.steps {
                        self.shown.remove(&step.id);
                    }
                }
                for key in chunk.records {
                    self.records.remove(&key);
                    self.contexts.remove(&key);
                    self.record_chunks.remove(&key);
                }
            } else {
                self.records.remove(&key);
                self.contexts.remove(&key);
            }
        }
    }
    fn delta(&mut self, key: &str) -> Frame {
        let mut upsert = Vec::new();
        let mut remove = Vec::new();
        if !self.initialized
            || self.rebuild
            || (self.start == 0 && self.records.len() < 30 && !self.dirty.is_empty())
        {
            let rows = self.rows_full();
            let old: Vec<_> = if self.initialized {
                self.chunks
                    .values()
                    .flat_map(|chunk| chunk.rows.iter())
                    .chain(self.last_queued.iter())
                    .collect()
            } else {
                self.last_rows.iter().collect()
            };
            let previous: HashMap<_, _> = old.iter().map(|row| (&row.id, *row)).collect();
            upsert.extend(
                rows.iter()
                    .filter(|row| previous.get(&row.id).is_none_or(|old| *old != *row))
                    .cloned(),
            );
            let ids: HashSet<_> = rows.iter().map(|row| &row.id).collect();
            remove.extend(
                old.iter()
                    .filter(|row| !ids.contains(&row.id))
                    .map(|row| row.id.clone()),
            );
            self.remember_shown(&rows);
            self.prepare(rows);
            self.last_rows.clear();
        } else {
            let mut dirty: Vec<_> = self.dirty.drain().collect();
            dirty.sort_unstable();
            for id in dirty {
                let chunk = &self.chunks[&id];
                let mut positions = chunk.records.clone();
                positions.sort_unstable();
                let records: Vec<_> = positions
                    .iter()
                    .filter_map(|key| self.records.get(key).cloned())
                    .collect();
                let rows = view::view_rows_with_context(
                    &records,
                    true,
                    self.is_codex,
                    &self.shown,
                    Some(chunk.context),
                );
                upsert.extend(rows.iter().filter(|row| !chunk.rows.contains(row)).cloned());
                remove.extend(
                    chunk
                        .rows
                        .iter()
                        .filter(|row| !rows.iter().any(|new| new.id == row.id))
                        .map(|row| row.id.clone()),
                );
                self.remember_shown(&rows);
                self.chunks.get_mut(&id).unwrap().rows = rows;
            }
            if self.queue_dirty {
                let rows = view::view_rows(&self.pending_queue, true, self.is_codex, &self.shown);
                upsert.extend(
                    rows.iter()
                        .filter(|row| !self.last_queued.contains(row))
                        .cloned(),
                );
                remove.extend(
                    self.last_queued
                        .iter()
                        .filter(|row| !rows.iter().any(|new| new.id == row.id))
                        .map(|row| row.id.clone()),
                );
                self.remember_shown(&rows);
                self.last_queued = rows;
                self.queue_dirty = false;
            }
        }
        let usage = self.usage.value();
        let mut payload =
            json!({"key": key, "upsert": upsert, "remove": remove, "end_offset": self.end});
        if usage != self.last_usage {
            payload["usage"] = json!(usage);
        }
        self.last_usage = usage;
        self.compact();
        Frame::new("rows", payload, Some(key))
    }
    fn reindex(&mut self) {
        let initial = self
            .records
            .first_key_value()
            .and_then(|(key, _)| self.contexts.get(key))
            .copied();
        let usage = self.usage.clone();
        let queue_lost = self.pending_queue.iter().any(|record| {
            !self
                .records
                .values()
                .any(|cached| cached.source_id == record.source_id)
        });
        let pending_queue = std::mem::take(&mut self.pending_queue);
        let queue_cursor = self.queue_cursor;
        let records = std::mem::take(&mut self.records);
        let mut fresh = Self::empty(self.start, self.is_codex);
        if let Some(context) = initial {
            fresh.context = context;
        }
        fresh.end = self.end;
        fresh.shown = std::mem::take(&mut self.shown);
        fresh.last_usage = self.last_usage.take();
        fresh.clear_row = self.clear_row.take();
        if queue_lost {
            fresh.pending_queue = pending_queue;
            fresh.queue_cursor = queue_cursor;
        }
        for (key, record) in records {
            fresh.usage.ingest(&record.event);
            fresh.contexts.insert(key, fresh.context);
            fresh.records.insert(key, record);
            fresh.update_queue(key);
            fresh.append(key);
        }
        if usage.has_report() {
            fresh.usage = usage;
        }
        *self = fresh;
    }
    fn rows_full(&self) -> Vec<Row> {
        let records = self.records.values().cloned().collect::<Vec<_>>();
        let compacted = self
            .records
            .first_key_value()
            .is_some_and(|(key, _)| key.0 > self.start);
        let context = compacted.then(|| self.contexts[self.records.first_key_value().unwrap().0]);
        let mut rows = view::view_rows_with_context(
            &records,
            self.start > 0 || compacted,
            self.is_codex,
            &self.shown,
            context,
        );
        rows.retain(|row| row.kind != view::RowKind::Queued);
        rows.extend(view::view_rows(
            &self.pending_queue,
            true,
            self.is_codex,
            &self.shown,
        ));
        if compacted && let Some(divider) = &self.clear_row {
            rows.insert(0, divider.clone());
        }
        rows
    }
}

// Source positions come from the existing follower's per-line page markers, not provider JSON.
async fn read_row_window(
    tool: &Path,
    log: &Log,
    start: u64,
    end: u64,
) -> Result<LiveRows, ApiError> {
    let mut state = LiveRows::empty(start, log.format == "codex");
    if start == end {
        return Ok(state);
    }
    let mut child = tokio::process::Command::new(tool)
        .args([
            "--follow",
            "--from",
            &start.to_string(),
            "--format",
            &log.format,
        ])
        .arg(&log.path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transcript tool unavailable",
            )
        })?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let read = tokio::time::timeout(Duration::from_secs(10), async {
        let mut first = true;
        let mut cursor = start;
        let mut events = Vec::new();
        loop {
            let line = lines
                .next_line()
                .await
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript read failed"))?
                .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript tool ended"))?;
            let value: Value = serde_json::from_str(&line).map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "invalid transcript output",
                )
            })?;
            if value["type"] == "page" {
                if first {
                    first = false;
                    continue;
                }
                let next = value["end_offset"].as_u64().ok_or_else(|| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "transcript offset missing",
                    )
                })?;
                if next > end {
                    return Err(error(
                        StatusCode::CONFLICT,
                        "history file changed; reopen this chat",
                    ));
                }
                state.ingest(cursor, next, &events);
                events.clear();
                cursor = next;
                if next == end {
                    return Ok(state);
                }
            } else {
                events.push(value);
            }
        }
    })
    .await
    .map_err(|_| error(StatusCode::GATEWAY_TIMEOUT, "transcript read timed out"));
    let _ = child.kill().await;
    let _ = child.wait().await;
    read?
}

fn row_offsets(path: &Path, tail: u16, before: Option<u64>) -> std::io::Result<(u64, u64)> {
    use std::io::{Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut end = file.metadata()?.len().min(before.unwrap_or(u64::MAX));
    let mut buffer = [0u8; 64 * 1024];
    // A partial line stays with the live follower until its terminating newline arrives.
    let mut cursor = end;
    while cursor > 0 {
        let count = (cursor as usize).min(buffer.len());
        let start = cursor - count as u64;
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer[..count])?;
        if let Some(index) = buffer[..count].iter().rposition(|byte| *byte == b'\n') {
            end = start + index as u64 + 1;
            break;
        }
        cursor = start;
    }
    if cursor == 0 {
        end = 0;
    }
    let mut cursor = end;
    let mut lines = 0;
    while cursor > 0 {
        let count = (cursor as usize).min(buffer.len());
        let start = cursor - count as u64;
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer[..count])?;
        for index in (0..count).rev() {
            let offset = start + index as u64;
            if buffer[index] == b'\n' && offset + 1 != end {
                lines += 1;
                if lines == tail {
                    return Ok((offset + 1, end));
                }
            }
        }
        cursor = start;
    }
    Ok((0, end))
}

async fn transcript_rows(
    State(app): State<App>,
    Query(query): Query<TranscriptQuery>,
) -> Result<Json<Value>, ApiError> {
    if !(1..=2000).contains(&query.tail) {
        return Err(error(StatusCode::BAD_REQUEST, "tail must be 1..2000"));
    }
    let root = app.root.clone();
    let session = query.session.clone();
    let agent = query.agent.clone();
    let (log, start, end) = tokio::task::spawn_blocking(move || {
        let log = resolve_log(&root, &session, &agent)?;
        let (start, end) = row_offsets(&log.path, query.tail, query.before)
            .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?;
        Ok::<_, ApiError>((log, start, end))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "rows task failed"))??;
    let key = format!("{}:{}", query.session, query.agent);
    if query.before.is_none() {
        let hub = app.hub.lock().unwrap();
        let cached = hub
            .followers
            .get(&key)
            .and_then(|follower| follower.rows.as_ref())
            .or_else(|| {
                hub.row_cache
                    .iter()
                    .find(|(cached, _)| cached == &key)
                    .map(|(_, state)| state)
            });
        if let Some(state) = cached
            && state.initialized
            && !state.rebuild
            && state.dirty.is_empty()
            && state
                .records
                .first_key_value()
                .is_some_and(|(key, _)| key.0 <= start)
            && state.end == end
        {
            return Ok(Json(
                json!({"page":{"start_offset":start,"end_offset":end},"rows":state.chunks.values().filter(|chunk|state.records.len()<=ROW_RECORD_LIMIT || chunk.records.iter().any(|key|key.0>=start)).flat_map(|chunk|chunk.rows.iter()).chain(state.last_queued.iter()).collect::<Vec<_>>(),"usage":state.usage.value(),"remove":[]}),
            ));
        }
    }
    let recovery = if query.before.is_some() {
        let hub = app.hub.lock().unwrap();
        hub.followers
            .get(&key)
            .and_then(|follower| follower.rows.as_ref())
            .or_else(|| {
                hub.row_cache
                    .iter()
                    .find(|(cached, _)| cached == &key)
                    .map(|(_, state)| state)
            })
            .filter(|state| {
                state
                    .records
                    .first_key_value()
                    .is_some_and(|(key, _)| key.0 > state.start)
            })
            .map(|state| (state.start, state.end))
    } else {
        None
    };
    let recovery = if let Some((start, end)) = recovery {
        Some(read_row_window(&app.tool, &log, start, end).await?)
    } else {
        None
    };
    let page = read_row_window(&app.tool, &log, start, end).await?;
    let key = format!("{}:{}", query.session, query.agent);
    let mut hub = app.hub.lock().unwrap();
    let active = hub
        .followers
        .get(&key)
        .and_then(|follower| follower.rows.clone());
    let history = query.before.and_then(|_| {
        hub.row_cache
            .iter()
            .find(|(cached, _)| cached == &key)
            .map(|(_, state)| state.clone())
    });
    let mut state = active.or(history).unwrap_or_else(|| page.clone());
    if let Some(recovery) = recovery {
        state.records.extend(recovery.records);
        state.reindex();
    }
    let previous = state.rows();
    state.records.extend(page.records);
    state.start = state.start.min(start);
    state.end = state.end.max(end);
    state.reindex();
    let rows = state.rows();
    let ids: HashSet<_> = rows.iter().map(|row| &row.id).collect();
    let remove: Vec<_> = previous
        .iter()
        .filter(|row| !ids.contains(&row.id))
        .map(|row| &row.id)
        .collect();
    let usage = state.usage.value();
    let previous: HashMap<_, _> = previous.iter().map(|row| (&row.id, row)).collect();
    let upsert: Vec<_> = rows
        .iter()
        .filter(|row| {
            query.before.is_none() || previous.get(&row.id).is_none_or(|old| *old != *row)
        })
        .collect();
    let response = json!({"page": {"start_offset": start, "end_offset": end}, "rows": upsert, "usage": usage, "remove": remove});
    state.last_usage = usage;
    state.remember_shown(&rows);
    state.prepare(rows.clone());
    state.last_rows = rows;
    state.compact();
    state.last_rows = state.rows();
    let subscribed = hub
        .streams
        .values()
        .any(|client| client.row_keys.contains(&key));
    if subscribed && let Some(follower) = hub.followers.get_mut(&key) {
        follower.rows = Some(state);
    } else {
        hub.cache_rows(key, state);
    }
    Ok(Json(response))
}

#[derive(Deserialize)]
struct ToolOutputQuery {
    session: String,
    agent: String,
    row: String,
}
fn output_locator(rows: &[Row], id: &str) -> Option<String> {
    for row in rows {
        if row.id == id {
            return row
                .tool
                .as_ref()
                .and_then(|tool| tool.output.as_ref())
                .map(|output| output.id.clone())
                .or_else(|| row.diff.as_ref().map(|diff| diff.id.clone()))
                .or_else(|| {
                    row.tool
                        .as_ref()
                        .and_then(|tool| tool.diffs.first())
                        .map(|diff| diff.id.clone())
                })
                .or_else(|| row.shell.as_ref().map(|shell| shell.output.id.clone()))
                .or_else(|| {
                    row.command
                        .as_ref()
                        .and_then(|command| command.output.as_ref())
                        .map(|output| output.id.clone())
                })
                .or_else(|| row.output.as_ref().map(|output| output.id.clone()));
        }
        if let Some(id) = output_locator(&row.steps, id) {
            return Some(id);
        }
    }
    None
}

async fn tool_output(
    State(app): State<App>,
    Query(mut query): Query<ToolOutputQuery>,
) -> Result<Json<Value>, ApiError> {
    {
        let key = format!("{}:{}", query.session, query.agent);
        let hub = app.hub.lock().unwrap();
        let state = hub
            .followers
            .get(&key)
            .and_then(|follower| follower.rows.as_ref())
            .or_else(|| {
                hub.row_cache
                    .iter()
                    .find(|(cached, _)| cached == &key)
                    .map(|(_, state)| state)
            });
        if let Some(state) = state
            && let Some(locator) = state
                .chunks
                .values()
                .find_map(|chunk| output_locator(&chunk.rows, &query.row))
                .or_else(|| output_locator(&state.last_rows, &query.row))
        {
            query.row = locator;
        }
    }
    if let Some((_, source)) = query.row.rsplit_once('@') {
        let offset = source
            .strip_prefix("raw-")
            .and_then(|source| source.split('-').next())
            .and_then(|offset| offset.parse::<u64>().ok())
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        let log = resolve_log(&app.root, &query.session, &query.agent)?;
        let path = log.path.clone();
        let end = tokio::task::spawn_blocking(move || {
            use std::io::{BufRead, Seek, SeekFrom};
            let mut file = std::fs::File::open(path)?;
            file.seek(SeekFrom::Start(offset))?;
            let mut reader = std::io::BufReader::new(file);
            let mut line = String::new();
            let mut end = offset;
            for _ in 0..ROW_RECORD_LIMIT {
                line.clear();
                let bytes = reader.read_line(&mut line)?;
                if bytes == 0 {
                    break;
                }
                end += bytes as u64;
            }
            Ok::<_, std::io::Error>(end)
        })
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tool locator task failed",
            )
        })?
        .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        let mut window = read_row_window(&app.tool, &log, offset, end).await?;
        if output_locator(&window.rows(), &query.row).is_none() {
            let full_end = std::fs::metadata(&log.path)
                .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?
                .len();
            if end < full_end {
                window = read_row_window(&app.tool, &log, offset, full_end).await?;
            }
        }
        query.row = output_locator(&window.rows(), &query.row)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    }
    let (source, component) = query
        .row
        .rsplit_once(':')
        .filter(|(_, component)| matches!(*component, "output" | "diff"))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    let mut pieces = source
        .strip_prefix("raw-")
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?
        .split('-');
    let offset: u64 = pieces
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    let index: usize = pieces
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    if pieces.next().is_some() {
        return Err(error(StatusCode::NOT_FOUND, "unknown tool output"));
    }
    let component = component.to_owned();
    let text = tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, Seek, SeekFrom};
        use std::os::unix::fs::OpenOptionsExt;
        let log = resolve_log(&app.root, &query.session, &query.agent)?;
        let mut file = std::fs::File::open(&log.path)
            .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))
                .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
            let mut newline = [0];
            file.read_exact(&mut newline)
                .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
            if newline != *b"\n" {
                return Err(error(StatusCode::NOT_FOUND, "unknown tool output"));
            }
        }
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        if line.is_empty() {
            return Err(error(StatusCode::NOT_FOUND, "unknown tool output"));
        }
        // The translator still owns parsing. A private one-line copy keeps on-demand work bounded.
        let temporary =
            std::env::temp_dir().join(format!("swarm-output-{}.jsonl", uuid::Uuid::now_v7()));
        let scratch = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if scratch.is_err() {
            let _ = std::fs::remove_file(&temporary);
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "output scratch write failed",
            ));
        }
        let output = std::process::Command::new(&app.tool)
            .args(["--format", &log.format])
            .arg(&temporary)
            .output();
        let _ = std::fs::remove_file(temporary);
        let output = output.map_err(|_| {
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
        let events: Vec<Value> = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(serde_json::from_slice)
            .collect::<Result<_, _>>()
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "invalid transcript output",
                )
            })?;
        let event = events
            .get(index)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        let text = match (view::string(event, "type"), component.as_str()) {
            ("tool_call_update", "output") => view::string(event, "content").to_owned(),
            ("tool_diff", "diff") => {
                swarm::view::diff_patch(
                    &serde_json::from_value::<swarm::view::Diff>(event.clone())
                        .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?,
                    true,
                )
                .0
            }
            ("system_message", "output") => match view::string(event, "kind") {
                "shell_output" => view::shell_output_text(view::string(event, "text")),
                "command_output" => view::command_output_text(view::string(event, "text")),
                _ => view::string(event, "text").to_owned(),
            },
            _ => return Err(error(StatusCode::NOT_FOUND, "unknown tool output")),
        };
        Ok(Json(json!({"text": text})))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "tool output task failed"))??;
    Ok(text)
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
        row_keys: HashSet::new(),
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
    #[serde(default)]
    view: Option<String>,
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
            if entry.view.as_deref().is_some_and(|view| view != "rows") {
                return Err(error(StatusCode::BAD_REQUEST, "unknown transcript view"));
            }
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
    let mut seeds = HashMap::new();
    for (entry, log) in &keys {
        if entry.view.as_deref() != Some("rows") {
            continue;
        }
        let known = {
            let hub = app.hub.lock().unwrap();
            hub.followers
                .get(&entry.key)
                .is_some_and(|follower| follower.rows.is_some() || follower.rendering_rows)
                || hub.row_cache.iter().any(|(key, _)| key == &entry.key)
        };
        if !known {
            let path = log.path.clone();
            let from = entry.from;
            let (start, end) =
                tokio::task::spawn_blocking(move || row_offsets(&path, 200, Some(from)))
                    .await
                    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "rows seed task failed"))?
                    .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?;
            let mut seed = read_row_window(&app.tool, log, start, end).await?;
            seed.last_rows = seed.rows();
            seed.prepare(seed.last_rows.clone());
            seeds.insert(entry.key.clone(), seed);
        }
    }
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
        client.row_keys = keys
            .iter()
            .filter(|(entry, _)| entry.view.as_deref() == Some("rows"))
            .map(|(entry, _)| entry.key.clone())
            .collect();
        client.queue.retain(|frame| frame.key.is_none());
        hub.prune();
        for (entry, log) in keys {
            let needs_rows = hub
                .streams
                .values()
                .any(|client| client.row_keys.contains(&entry.key));
            let active_seed = hub
                .followers
                .get(&entry.key)
                .and_then(|follower| follower.rows.clone());
            let seed = active_seed
                .or_else(|| {
                    hub.row_cache
                        .iter()
                        .position(|(key, _)| key == &entry.key)
                        .map(|index| hub.row_cache.remove(index).unwrap().1)
                })
                .or_else(|| seeds.remove(&entry.key));
            if let Some(follower) = hub.followers.get_mut(&entry.key)
                && needs_rows
                && follower.rows.is_none()
            {
                follower.rows = seed.clone();
            }
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
                    rendering_rows: false,
                    rows: if needs_rows {
                        seed.or_else(|| Some(LiveRows::empty(entry.from, log.format == "codex")))
                    } else {
                        None
                    },
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

async fn flush_transcript(
    app: &Weak<Server>,
    key: &str,
    generation: &str,
    batches: &mut Vec<(u64, Vec<Value>)>,
) -> std::io::Result<()> {
    if batches.is_empty() {
        return Ok(());
    }
    if let Some(app) = app.upgrade() {
        let pending = {
            let mut hub = app.hub.lock().unwrap();
            let Some(follower) = hub
                .followers
                .get_mut(key)
                .filter(|follower| follower.generation == generation)
            else {
                batches.clear();
                return Ok(());
            };
            if let Some(state) = &mut follower.rows {
                let mut start = follower.from;
                for (end, events) in batches.iter() {
                    state.ingest(start, *end, events);
                    start = *end;
                }
            }
            if follower
                .rows
                .as_ref()
                .is_some_and(|state| state.rebuild && state.records.len() > ROW_RECORD_LIMIT)
            {
                follower.rendering_rows = true;
                follower.rows.take().map(Err)
            } else {
                let frame = follower.rows.as_mut().map(|state| state.delta(key));
                frame.map(Ok)
            }
        };
        let row_frame = if let Some(pending) = pending {
            if let Ok(frame) = pending {
                Some(frame)
            } else {
                let state = pending.err().unwrap();
                let render_key = key.to_owned();
                let (mut state, frame) = tokio::task::spawn_blocking(move || {
                    let mut state = state;
                    let frame = state.delta(&render_key);
                    (state, frame)
                })
                .await
                .map_err(std::io::Error::other)?;
                loop {
                    let incoming = {
                        let mut hub = app.hub.lock().unwrap();
                        let Some(follower) = hub
                            .followers
                            .get_mut(key)
                            .filter(|follower| follower.generation == generation)
                        else {
                            batches.clear();
                            return Ok(());
                        };
                        if let Some(incoming) = follower.rows.take() {
                            Some(incoming)
                        } else {
                            follower.rows = Some(state);
                            follower.rendering_rows = false;
                            break;
                        }
                    };
                    if let Some(incoming) = incoming {
                        let merged = tokio::task::spawn_blocking(move || {
                            let usage = state.usage.clone();
                            state.records.extend(incoming.records);
                            state.shown.extend(incoming.shown);
                            state.start = state.start.min(incoming.start);
                            state.end = state.end.max(incoming.end);
                            state.reindex();
                            state.usage = usage;
                            let rows = state.rows_full();
                            state.prepare(rows);
                            state.compact();
                            state
                        })
                        .await
                        .map_err(std::io::Error::other)?;
                        state = merged;
                    }
                }
                Some(frame)
            }
        } else {
            None
        };
        let mut hub = app.hub.lock().unwrap();
        if hub
            .followers
            .get(key)
            .is_none_or(|follower| follower.generation != generation)
        {
            batches.clear();
            return Ok(());
        }
        for client in hub.streams.values_mut() {
            let row_view = client.row_keys.contains(key);
            let Some(cursor) = client.keys.get_mut(key) else {
                continue;
            };
            let mut events = Vec::new();
            let mut end_offset = *cursor;
            for (end, batch) in batches.iter() {
                if *end > end_offset {
                    if !row_view {
                        events.extend(batch.iter().cloned());
                    }
                    end_offset = *end;
                }
            }
            if end_offset != *cursor {
                *cursor = end_offset;
                if row_view {
                    if let Some(frame) = &row_frame {
                        client.push(frame.clone());
                    } else {
                        client.push(Frame::new("reset", json!({"key": key}), Some(key)));
                    }
                    continue;
                }
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
    Ok(())
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
                    flush_transcript(&app, &entry.key, &generation, &mut batches).await?;
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
        let _ = child.wait().await;
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
    let bytes: std::borrow::Cow<'static, [u8]> = match &app.assets {
        Some(assets) => match disk_file(assets, relative).await {
            Some(bytes) => bytes.into(),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
        None => match WEB_ASSETS.iter().find(|(name, _)| *name == relative) {
            Some((_, bytes)) => (*bytes).into(),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    let content_type = match Path::new(relative)
        .extension()
        .and_then(|extension| extension.to_str())
    {
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

/// A file under `--assets DIR`, refused when a link leads out of DIR.
async fn disk_file(assets: &Path, relative: &str) -> Option<Vec<u8>> {
    let path = tokio::fs::canonicalize(assets.join(relative)).await.ok()?;
    if !path.starts_with(assets) || !tokio::fs::metadata(&path).await.ok()?.is_file() {
        return None;
    }
    tokio::fs::read(path).await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(keys: &[(&str, u64)]) -> Client {
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

    #[test]
    fn setup_chunks_keep_utf8_boundaries_and_replace_invalid_bytes() {
        let mut pending = Vec::new();
        assert_eq!(setup_text(&mut pending, &[b'a', 0xf0, 0x9f], false), "a");
        assert_eq!(
            setup_text(&mut pending, &[0x98, 0x80, 0xff, 0xe2], false),
            "😀�"
        );
        assert_eq!(setup_text(&mut pending, &[], true), "�");
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn usage_reads_cache_and_reserves_provider_refresh_even_on_failure() {
        let fixture = action_fixture();
        std::fs::write(
            &fixture.app.cli,
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$SWARM_HOME/actions"
if [ -f "$SWARM_HOME/fail" ] && [ "$2" = --refresh ]; then printf 'refresh refused' >&2; exit 2; fi
printf '{"schema":"swarm.usage.v1","meters":[]}\n'
"#,
        )
        .unwrap();
        assert_eq!(
            usage_read(State(fixture.app.clone())).await.unwrap().0["schema"],
            "swarm.usage.v1"
        );
        let request = |provider: &str| {
            usage_refresh(
                State(fixture.app.clone()),
                Ok(Json(UsageRefresh {
                    provider: provider.into(),
                })),
            )
        };
        let (one, two) = tokio::join!(request("claude"), request("claude"));
        assert_eq!(one.unwrap().0["refresh"]["state"], "refreshed");
        assert_eq!(two.unwrap().0["refresh"]["state"], "cooldown");
        assert_eq!(
            action_calls(&fixture)
                .matches("usage --refresh --provider claude --json")
                .count(),
            1
        );
        std::fs::write(fixture.app.root.parent().unwrap().join("fail"), "fail").unwrap();
        let first = request("codex").await.unwrap_err();
        let second = request("codex").await.unwrap_err();
        assert_eq!(first.1.0, second.1.0);
        assert_eq!(
            action_calls(&fixture)
                .matches("usage --refresh --provider codex --json")
                .count(),
            1
        );
        std::fs::remove_file(fixture.app.root.parent().unwrap().join("fail")).unwrap();
        fixture
            .app
            .usage_refresh
            .lock()
            .unwrap()
            .get_mut("codex")
            .unwrap()
            .0 = tokio::time::Instant::now() - Duration::from_secs(300);
        assert_eq!(
            request("codex").await.unwrap().0["refresh"]["state"],
            "refreshed"
        );
        assert_eq!(request("agy").await.unwrap_err().0, StatusCode::BAD_REQUEST);
        assert!(
            Action::UsageRefresh(UsageRefresh {
                provider: "claude".into()
            })
            .lane()
            .is_none()
        );
    }

    #[tokio::test]
    async fn profile_and_account_writes_run_their_owner_and_only_a_stale_revision_is_409() {
        let fixture = action_fixture();
        std::fs::write(
            &fixture.app.cli,
            r#"#!/bin/sh
printf '%s|%s|%s\n' "$SWARM_ADAPTER" "$PWD" "$*" >> "$SWARM_HOME/actions"
case "$*" in
  *stale*) printf 'swarm: profiles changed on disk; reload and try again' >&2; exit 1 ;;
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
            assert_eq!(last_call(), format!("tmux-solo|{root}|{args}"));
        }
        let home = std::env::var("HOME").unwrap();
        let repo = env!("CARGO_MANIFEST_DIR");
        for (workspace, cwd) in [
            (json!(repo), repo),
            (json!("/not/listed"), &home),
            (Value::Null, &home),
        ] {
            let body =
                json!({"provider":"claude","name":"work","revision":"r2","workspace":workspace});
            assert_eq!(
                write("/api/accounts/login", body).await.unwrap().0["revision"],
                "next"
            );
            assert_eq!(
                last_call(),
                format!(
                    "tmux-solo|{root}|accounts login --provider claude --name work --revision r2 --json --cwd {cwd}"
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

    #[tokio::test]
    async fn setup_streams_both_channels_reports_failure_and_kills_timeout_group() {
        let fixture =
            Fixture::at(std::env::temp_dir().join(format!("swarm-setup-{}", uuid::Uuid::now_v7())));
        let workspace = fixture.dir.to_str().unwrap();
        fixture
            .app
            .hub
            .lock()
            .unwrap()
            .streams
            .insert("setup-test".into(), client(&[]));
        let result = run_setup(
            &fixture.app,
            workspace,
            "printf 'ready'; printf 'problem' >&2; exit 7",
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(result["state"], "failed");
        assert_eq!(result["exit_code"], 7);
        {
            let hub = fixture.app.hub.lock().unwrap();
            let frames: Vec<Value> = hub.streams["setup-test"]
                .queue
                .iter()
                .map(|f| serde_json::from_str(&f.data).unwrap())
                .collect();
            assert_eq!(frames.first().unwrap()["state"], "running");
            assert_eq!(frames.last().unwrap()["exit_code"], 7);
            assert!(
                frames
                    .iter()
                    .any(|f| f["channel"] == "stdout" && f["text"] == "ready")
            );
            assert!(
                frames
                    .iter()
                    .any(|f| f["channel"] == "stderr" && f["text"] == "problem")
            );
        }
        let result = run_setup(
            &fixture.app,
            workspace,
            "sleep 30 & printf '%s' \"$!\" > sleeper; wait",
            Duration::from_millis(150),
        )
        .await;
        assert_eq!(result["state"], "timeout");
        assert!(fixture.dir.is_dir());
        let pid = std::fs::read_to_string(fixture.dir.join("sleeper")).unwrap();
        assert!(
            !std::process::Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );
        let result = run_setup(
            &fixture.app,
            workspace,
            "sleep 30 & printf '%s' \"$!\" > detached; exit 0",
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(result["state"], "completed");
        let pid = std::fs::read_to_string(fixture.dir.join("detached")).unwrap();
        assert!(
            !std::process::Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );
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
            Self::at(dir)
        }
        fn at(dir: PathBuf) -> Self {
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
                None,
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

    fn action_fixture() -> Fixture {
        action_fixture_at(Fixture::new())
    }

    fn action_fixture_at(mut fixture: Fixture) -> Fixture {
        use std::os::unix::fs::PermissionsExt;
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET adapter = 'fixture' WHERE id = ?1",
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
  *) if [ "$1" = type ]; then cat > "$SWARM_HOME/text"; fi ;;
esac
"#)).unwrap();
        std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (actions, queue) = mpsc::channel(64);
        Arc::get_mut(&mut fixture.app).unwrap().actions = actions;
        start_action_queue(&fixture.app, queue);
        fixture
    }

    async fn call_action(
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

    fn action_calls(fixture: &Fixture) -> String {
        std::fs::read_to_string(fixture.app.root.parent().unwrap().join("actions"))
            .unwrap_or_default()
    }

    macro_rules! target_route_test {
        ($name:ident, $route:literal, $extra:expr, $args:literal) => {
            #[tokio::test]
            async fn $name() {
                let fixture = action_fixture();
                let mut body = json!({"session": fixture.session, "agent": "chair"});
                for (key, value) in $extra.as_object().unwrap() { body[key] = value.clone(); }
                let before = fixture.app.hub.lock().unwrap().version;
                assert_eq!(call_action(&fixture, $route, body.clone()).await.unwrap().0, json!({"ok": true}));
                assert!(fixture.app.hub.lock().unwrap().version > before);
                let calls = action_calls(&fixture);
                let expected = format!("fixture|{}|orchestrator|{}|{}", fixture.session,
                    env!("CARGO_MANIFEST_DIR"), $args);
                assert!(calls.contains(&expected), "{calls}");
                body["session"] = json!("unknown");
                assert_eq!(call_action(&fixture, $route, body.clone()).await.unwrap_err().0, StatusCode::NOT_FOUND);
                body["session"] = json!(fixture.session);
                body["agent"] = json!("unknown");
                assert_eq!(call_action(&fixture, $route, body).await.unwrap_err().0, StatusCode::NOT_FOUND);
                assert_eq!(call_action(&fixture, $route, json!({})).await.unwrap_err().0, StatusCode::BAD_REQUEST);
                assert_eq!(action_calls(&fixture), calls);
            }
        };
    }

    target_route_test!(
        send_route,
        "/api/chat/send",
        json!({"text": "hello ' $()\nworld"}),
        "type orchestrator"
    );
    target_route_test!(
        interrupt_route,
        "/api/chat/interrupt",
        json!({}),
        "interrupt orchestrator"
    );
    target_route_test!(
        key_route,
        "/api/chat/key",
        json!({"key": "Up"}),
        "key orchestrator Up"
    );
    target_route_test!(
        answer_route,
        "/api/chat/answer",
        json!({"prompt_id": "prompt", "choice": 1}),
        "answer orchestrator prompt 1"
    );
    target_route_test!(
        close_route,
        "/api/agent/close",
        json!({}),
        "close orchestrator"
    );

    macro_rules! session_route_test {
        ($name:ident, $route:literal, $verb:literal) => {
            #[tokio::test]
            async fn $name() {
                let fixture = action_fixture();
                assert_eq!(call_action(&fixture, $route, json!({"ids": [fixture.session]})).await.unwrap().0, json!({"ok": true}));
                let calls = action_calls(&fixture);
                assert!(calls.contains(&format!("session {} {}", $verb, fixture.session)));
                assert_eq!(call_action(&fixture, $route, json!({"ids": [fixture.session, "unknown"]})).await.unwrap_err().0, StatusCode::NOT_FOUND);
                for bad in [json!({}), json!({"ids": []}), json!({"ids": [1]})] {
                    assert_eq!(call_action(&fixture, $route, bad).await.unwrap_err().0, StatusCode::BAD_REQUEST);
                }
                assert_eq!(action_calls(&fixture), calls);
            }
        };
    }

    session_route_test!(archive_route, "/api/sessions/archive", "archive");
    session_route_test!(unarchive_route, "/api/sessions/unarchive", "unarchive");

    #[tokio::test]
    async fn new_chat_route() {
        let fixture = action_fixture();
        let body = json!({"cwd": env!("CARGO_MANIFEST_DIR"), "provider": "codex", "model": "gpt-test", "account": "auto"});
        let response = call_action(&fixture, "/api/chat/new", body)
            .await
            .unwrap()
            .0;
        assert_eq!(response["ok"], true);
        assert!(uuid::Uuid::parse_str(response["session"].as_str().unwrap()).is_ok());
        assert_eq!(response["agent"], "orchestrator");
        assert_eq!(response["pane"], "fixture-pane");
        let calls = action_calls(&fixture);
        let lines: Vec<_> = calls.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("|init"));
        assert!(lines[1].ends_with("|session new lane"));
        assert!(lines[2].ends_with(
            "|launch orchestrator chat --provider codex --model gpt-test --account auto"
        ));
        assert_eq!(
            call_action(
                &fixture,
                "/api/chat/new",
                json!({"cwd": "/not-in-snapshot"})
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        for bad in [
            json!({}),
            json!({"cwd": 3}),
            json!({"cwd": "relative"}),
            json!({"cwd": env!("CARGO_MANIFEST_DIR"), "model": "--bad"}),
        ] {
            assert_eq!(
                call_action(&fixture, "/api/chat/new", bad)
                    .await
                    .unwrap_err()
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(action_calls(&fixture), calls);
        let _ = call_action(
            &fixture,
            "/api/chat/new",
            json!({"cwd": env!("CARGO_MANIFEST_DIR")}),
        )
        .await
        .unwrap();
        assert!(action_calls(&fixture).ends_with("|launch orchestrator chat --account auto\n"));
    }

    #[tokio::test]
    async fn archived_sessions_route_filters_active_rows() {
        let fixture = action_fixture();
        let before = fixture.app.hub.lock().unwrap().version;
        let value = archived_sessions(State(fixture.app.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(
            value,
            json!({"ok": true, "sessions": [{"id": "archived", "archivedAt": 42}]})
        );
        assert!(action_calls(&fixture).ends_with("|sessions --json --archived\n"));
        assert_eq!(fixture.app.hub.lock().unwrap().version, before);
    }

    #[tokio::test]
    async fn invalid_values_fail_before_a_cli_call_and_send_preserves_stdin() {
        let fixture = action_fixture();
        for (route, fields) in [
            ("/api/chat/send", json!({"text": "  "})),
            ("/api/chat/key", json!({"key": "Escape"})),
            ("/api/chat/answer", json!({"prompt_id": "p", "choice": -1})),
            ("/api/chat/answer", json!({"prompt_id": "", "choice": 0})),
            (
                "/api/chat/interrupt",
                json!({"env": {"SWARM_ADAPTER": "bad"}}),
            ),
        ] {
            let mut body = json!({"session": fixture.session, "agent": "worker"});
            for (key, value) in fields.as_object().unwrap() {
                body[key] = value.clone();
            }
            assert_eq!(
                call_action(&fixture, route, body).await.unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            call_action(
                &fixture,
                "/api/chat/interrupt",
                json!([fixture.session, "chair"])
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(action_calls(&fixture), "");
        let text = "hello ' $()\nworld";
        let _ = call_action(
            &fixture,
            "/api/chat/send",
            json!({"session": fixture.session, "agent": "worker", "text": text}),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap(),
            text
        );
    }

    #[tokio::test]
    async fn action_queue_serializes_calls_and_preserves_cli_errors_without_retry() {
        let fixture = action_fixture();
        std::fs::write(
            &fixture.app.cli,
            r#"#!/bin/sh
printf 'start\n' >> "$SWARM_HOME/actions"
sleep 0.01
printf 'end\n' >> "$SWARM_HOME/actions"
printf 'exact CLI error\n' >&2
exit 1
"#,
        )
        .unwrap();
        let one = call_action(
            &fixture,
            "/api/chat/interrupt",
            json!({"session": fixture.session, "agent": "worker"}),
        );
        let two = call_action(
            &fixture,
            "/api/chat/interrupt",
            json!({"session": fixture.session, "agent": "worker"}),
        );
        let (one, two) = tokio::join!(one, two);
        for result in [one, two] {
            let (status, Json(error)) = result.unwrap_err();
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(error["error"], "exact CLI error");
        }
        assert_eq!(action_calls(&fixture), "start\nend\nstart\nend\n");
    }

    #[tokio::test]
    async fn a_session_send_finishes_while_another_session_action_or_launch_is_running() {
        for slow_route in ["/api/chat/interrupt", "/api/chat/new"] {
            let fixture = action_fixture();
            let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
            let other = swarm::store::create_session(
                &connection,
                "lane",
                Path::new(env!("CARGO_MANIFEST_DIR")),
                None,
                Some("fixture"),
            )
            .unwrap();
            swarm::store::add_agent(&connection, &other, "worker", "chat").unwrap();
            let new_id = uuid::Uuid::now_v7();
            std::fs::write(
                &fixture.app.cli,
                format!(
                    r#"#!/bin/sh
case "$1 $2" in
  'session new') printf '%s\n' '{new_id}'; exit 0 ;;
esac
if [ "$1" = interrupt ] || [ "$1" = launch ]; then
  printf 'running\n' > "$SWARM_HOME/slow-start"
  while [ ! -f "$SWARM_HOME/release" ]; do sleep 0.01; done
  printf 'fixture-pane\n'
fi
if [ "$1" = type ]; then cat > "$SWARM_HOME/text"; fi
"#
                ),
            )
            .unwrap();
            let slow_app = fixture.app.clone();
            let slow_body = if slow_route == "/api/chat/new" {
                json!({"cwd": env!("CARGO_MANIFEST_DIR")})
            } else {
                json!({"session": other, "agent": "worker"})
            };
            let slow = tokio::spawn(async move {
                chat_action(
                    State(slow_app),
                    slow_route.parse().unwrap(),
                    Ok(Json(slow_body)),
                )
                .await
            });
            let home = fixture.app.root.parent().unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while !home.join("slow-start").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let send = tokio::time::timeout(
                Duration::from_secs(2),
                call_action(
                    &fixture,
                    "/api/chat/send",
                    json!({"session": fixture.session, "agent": "worker", "text": "parallel send"}),
                ),
            )
            .await;
            let still_running = !slow.is_finished();
            // Release the stand-in before an assertion so a failed test cannot leave it waiting.
            std::fs::write(home.join("release"), "release").unwrap();
            let _ = tokio::time::timeout(Duration::from_secs(3), slow)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(still_running, "slow action ended before the send finished");
            assert_eq!(
                send.expect("send must not wait for another session or launch")
                    .unwrap()
                    .0,
                json!({"ok": true})
            );
            assert_eq!(
                std::fs::read_to_string(home.join("text")).unwrap(),
                "parallel send"
            );
        }
    }

    #[tokio::test]
    async fn archive_and_unarchive_share_a_lane_without_blocking_a_session_send() {
        let fixture = action_fixture();
        std::fs::write(
            &fixture.app.cli,
            r#"#!/bin/sh
case "$1 $2" in
  'session archive')
    printf 'running\n' > "$SWARM_HOME/slow-start"
    while [ ! -f "$SWARM_HOME/release" ]; do sleep 0.01; done ;;
  'session unarchive') printf 'restored\n' > "$SWARM_HOME/restored" ;;
  *) if [ "$1" = type ]; then cat > "$SWARM_HOME/text"; fi ;;
esac
"#,
        )
        .unwrap();
        let home = fixture.app.root.parent().unwrap();
        let archive_app = fixture.app.clone();
        let ids = json!({"ids": [fixture.session]});
        let archive_ids = ids.clone();
        let archive = tokio::spawn(async move {
            chat_action(
                State(archive_app),
                "/api/sessions/archive".parse().unwrap(),
                Ok(Json(archive_ids)),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            while !home.join("slow-start").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let restore_app = fixture.app.clone();
        let restore = tokio::spawn(async move {
            chat_action(
                State(restore_app),
                "/api/sessions/unarchive".parse().unwrap(),
                Ok(Json(ids)),
            )
            .await
        });
        let send = tokio::time::timeout(
            Duration::from_secs(2),
            call_action(
                &fixture,
                "/api/chat/send",
                json!({"session": fixture.session, "agent": "worker", "text": "parallel send"}),
            ),
        )
        .await;
        let restore_waited = !restore.is_finished() && !home.join("restored").exists();
        std::fs::write(home.join("release"), "release").unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(3), archive)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(3), restore)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            restore_waited,
            "unarchive must wait for archive in the shared lane"
        );
        assert_eq!(
            send.expect("send must not wait for the shared lane")
                .unwrap()
                .0,
            json!({"ok": true})
        );
        assert!(home.join("restored").exists());
    }

    #[tokio::test]
    async fn action_timeout_kills_the_process_group_without_retry() {
        let fixture = action_fixture();
        std::fs::write(
            &fixture.app.cli,
            r#"#!/bin/sh
printf 'start\n' >> "$SWARM_HOME/actions"
sleep 30 &
printf '%s\n' "$!" > "$SWARM_HOME/sleeper"
wait
"#,
        )
        .unwrap();
        let call = action_session(&fixture.app, &fixture.session)
            .unwrap()
            .args(&["interrupt", "worker"]);
        let result = run_cli_with_timeout(&fixture.app, call, None, Duration::from_secs(1)).await;
        assert_eq!(result.unwrap_err().0, StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(action_calls(&fixture), "start\n");
        let pid =
            std::fs::read_to_string(fixture.app.root.parent().unwrap().join("sleeper")).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(
            !std::process::Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );
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
                    view: None,
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
                    view: None,
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
    async fn row_page(
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
    fn codex_line(number: usize) -> String {
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":format!("message-{number}")}]}}).to_string() + "\n"
    }
    fn codex_log(fixture: &Fixture, count: usize) {
        std::fs::write(&fixture.log, (0..count).map(codex_line).collect::<String>()).unwrap();
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET chair_provider = 'codex' WHERE id = ?1",
                [&fixture.session],
            )
            .unwrap();
    }
    fn row_texts(page: &Value) -> Vec<String> {
        page["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["kind"] == "user")
            .map(|row| row["text"].as_str().unwrap().into())
            .collect()
    }

    #[tokio::test]
    async fn older_page_sends_only_new_and_changed_rows_in_thousand_row_history() {
        let fixture = Fixture::new();
        codex_log(&fixture, 0);
        let call = json!({"type":"response_item","payload":{"type":"function_call","call_id":"boundary","name":"shell","arguments":"{\"command\":\"true\"}"}});
        let result = json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"boundary","output":"done"}});
        let history = (0..99).map(codex_line).collect::<String>()
            + &format!("{call}\n{result}\n")
            + &(100..999).map(codex_line).collect::<String>();
        assert_eq!(history.lines().count(), 1000);
        std::fs::write(&fixture.log, history).unwrap();
        let first = row_page(&fixture, 900, None).await.unwrap();
        let orphan = first["rows"][0]["id"].clone();
        let older = row_page(&fixture, 100, first["page"]["start_offset"].as_u64())
            .await
            .unwrap();
        assert_eq!(older["rows"].as_array().unwrap().len(), 100);
        assert_eq!(row_texts(&older).len(), 99);
        assert_eq!(older["rows"][99]["tool"]["state"], "finished");
        assert_eq!(older["remove"], json!([orphan]));
        assert!(
            older["rows"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| !first["rows"].as_array().unwrap().contains(row))
        );
        let mut old_response = older.clone();
        old_response["rows"] = json!(fixture.app.hub.lock().unwrap().row_cache[0].1.last_rows);
        let new_bytes = older.to_string().len();
        let old_bytes = old_response.to_string().len();
        println!("older-page bytes {new_bytes}; old full-page bytes {old_bytes}");
        assert!(new_bytes * 5 < old_bytes);
    }
    #[tokio::test]
    async fn older_page_replaces_changed_fold_and_omits_unchanged_prose() {
        let fixture = Fixture::new();
        let calls = (0..4).map(|index| json!({"type":"assistant","uuid":format!("call-{index}"),"message":{"content":[{"type":"tool_use","id":format!("tool-{index}"),"name":"Read","input":{"file_path":"readme"}}]}}).to_string()+"\n").collect::<String>();
        let prose = json!({"type":"assistant","uuid":"prose","message":{"content":[{"type":"text","text":"done"}]}});
        std::fs::write(&fixture.log, calls + &format!("{prose}\n")).unwrap();
        let first = row_page(&fixture, 3, None).await.unwrap();
        let older = row_page(&fixture, 2, first["page"]["start_offset"].as_u64())
            .await
            .unwrap();
        assert_eq!(older["rows"].as_array().unwrap().len(), 1);
        assert_eq!(older["rows"][0]["steps"].as_array().unwrap().len(), 4);
        assert!(
            older["remove"]
                .as_array()
                .unwrap()
                .contains(&first["rows"][0]["id"])
        );
    }
    fn synthetic_event(number: usize, tools_only: bool) -> Value {
        if tools_only || number % 4 == 2 {
            json!({"type":"tool_call","tool_call_id":format!("tool-{number}"),"name":"Bash","input":{"command":"cargo test"},"status":"in_progress","meta":{"uuid":format!("call-{number}"),"timestamp":"2026-10-13T00:00:00Z"}})
        } else if number % 4 == 3 {
            json!({"type":"tool_call_update","tool_call_id":format!("tool-{}",number-1),"status":"completed","content":"test passed","meta":{"timestamp":"2026-10-13T00:00:01Z"}})
        } else {
            json!({"type":if number.is_multiple_of(4) {"user_message_chunk"} else {"agent_message_chunk"},"text":format!("message-{number}"),"meta":{"uuid":format!("message-{number}")}})
        }
    }
    fn reference_delta(state: &mut LiveRows, key: &str) -> Value {
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
    #[test]
    fn live_chunks_match_full_frames_for_claude_and_codex() {
        for codex in [false, true] {
            let mut incremental = LiveRows::empty(0, codex);
            let mut reference = LiveRows::empty(0, codex);
            let mut events = (0..320)
                .map(|number| synthetic_event(number, true))
                .collect::<Vec<_>>();
            events.extend([
                json!({"type":"tool_call_update","tool_call_id":"tool-5","status":"completed","content":"late result"}),
                json!({"type":"usage","kind":"context","context_tokens":80,"context_capacity_tokens":100}),
                json!({"type":"turn_ended","reason":"completed"}),
                json!({"type":"turn_started"}),
                json!({"type":"user_message_chunk","text":"next","meta":{"uuid":"next"}}),
                json!({"type":"agent_message_chunk","text":"answer","meta":{"uuid":"answer"}}),
                json!({"type":"agent_message_chunk","text":" more","meta":{"uuid":"answer"}}),
                json!({"type":"agent_thought_chunk","text":"thinking","meta":{"uuid":"thinking"}}),
                json!({"type":"system_message","kind":"swarm_ring","text":"ring"}),
                json!({"type":"tool_call","tool_call_id":"joined","name":"Bash","input":{"command":"true"}}),
                json!({"type":"tool_call_update","tool_call_id":"joined","status":"completed","content":"done"}),
                json!({"type":"tool_call","tool_call_id":"joined","name":"Bash","input":{"command":"false"}}),
                json!({"type":"system_message","kind":"compaction"}),
                json!({"type":"queue_operation","operation":"enqueue","content":"queued"}),
                json!({"type":"queue_operation","operation":"dequeue"}),
                json!({"type":"tool_call_update","tool_call_id":"retro","status":"completed","content":"before call"}),
                json!({"type":"tool_call","tool_call_id":"retro","name":"Read","input":{}}),
                json!({"type":"system_message","kind":"command","text":"<command-name>/example</command-name>","meta":{"uuid":"command"}}),
                json!({"type":"system_message","kind":"skill_body","text":"body","meta":{"parent_uuid":"command"}}),
                json!({"type":"system_message","kind":"command_output","text":"output","meta":{"parent_uuid":"command"}}),
                json!({"type":"system_message","kind":"shell_input","text":"<bash-input>true</bash-input>","meta":{"uuid":"shell"}}),
                json!({"type":"system_message","kind":"shell_output","text":"<bash-stdout>done</bash-stdout>","meta":{"parent_uuid":"shell"}}),
                json!({"type":"system_message","kind":"shell_output","text":"<bash-stdout>second</bash-stdout>","meta":{"parent_uuid":"shell"}}),

            ]);
            events.extend((0..99).map(|number|json!({"type":"session_info","kind":"title","value":"hidden","meta":{"uuid":format!("hidden-{number}")}})));
            events.extend([
                json!({"type":"tool_call","tool_call_id":"hidden-100","name":"Read","input":{}}),
                json!({"type":"turn_ended","reason":"completed"}),
                json!({"type":"tool_call","tool_call_id":"hidden-101","name":"Read","input":{}}),
            ]);
            for (index, event) in events.into_iter().enumerate() {
                incremental.ingest(index as u64, index as u64 + 1, std::slice::from_ref(&event));
                if event["type"] == "queue_operation" {
                    assert!(!incremental.rebuild, "a queue edit rebuilt the open turn");
                }
                reference.ingest(index as u64, index as u64 + 1, &[event]);
                let expected = reference_delta(&mut reference, "chat");
                let actual = incremental.delta("chat").data;
                assert_eq!(actual, expected.to_string(), "event {index}, codex={codex}");
            }
            assert_eq!(incremental.rows(), reference.last_rows);
            assert_eq!(
                incremental.usage.value(),
                view::usage(&reference.records.values().cloned().collect::<Vec<_>>())
            );
        }
    }
    #[test]
    fn trimmed_history_has_the_same_live_frames() {
        let mut live = LiveRows::empty(0, false);
        let mut reference = LiveRows::empty(0, false);
        let first = [
            json!({"type":"queue_operation","operation":"enqueue","content":"pending"}),
            json!({"type":"usage","kind":"context","context_tokens":80,"context_capacity_tokens":100}),
        ];
        live.ingest(0, 1, &first);
        reference.ingest(0, 1, &first);
        live.delta("chat");
        reference_delta(&mut reference, "chat");
        for index in 0..2400 {
            let event = synthetic_event(index, false);
            live.ingest(
                index as u64 + 1,
                index as u64 + 2,
                std::slice::from_ref(&event),
            );
            reference.ingest(index as u64 + 1, index as u64 + 2, &[event]);
            if index % 8 == 7 {
                assert_eq!(
                    serde_json::from_str::<Value>(&live.delta("chat").data).unwrap(),
                    reference_delta(&mut reference, "chat"),
                    "event {index}"
                );
            }
        }
        assert!(live.records.len() <= ROW_RECORD_LIMIT);
    }
    #[tokio::test]
    async fn synthetic_provider_logs_match_full_rebuild() {
        for codex in [false, true] {
            let fixture = Fixture::new();
            let lines=(0..12).flat_map(|turn| {
                let id=format!("tool-{turn}");
                if codex { vec![codex_line(turn).trim_end().to_owned(),
                    json!({"type":"response_item","payload":{"type":"function_call","call_id":id,"name":"shell","arguments":"{\"command\":\"true\"}"}}).to_string(),
                    json!({"type":"response_item","payload":{"type":"function_call_output","call_id":id,"output":"done"}}).to_string()]
                } else { vec![
                    json!({"type":"user","uuid":format!("user-{turn}"),"message":{"content":"prompt"}}).to_string(),
                    json!({"type":"assistant","uuid":format!("thought-{turn}"),"message":{"content":[{"type":"thinking","thinking":"think"}]}}).to_string(),
                    json!({"type":"assistant","uuid":format!("call-{turn}"),"message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":"true"}}]}}).to_string(),
                    json!({"type":"user","uuid":format!("result-{turn}"),"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"done"}]}}).to_string()]
                }
            }).collect::<Vec<_>>();
            std::fs::write(&fixture.log, lines.join("\n") + "\n").unwrap();
            let log = Log {
                path: fixture.log.clone(),
                format: if codex { "codex" } else { "claude" }.into(),
            };
            let mut state = LiveRows::empty(0, codex);
            let mut start = 0;
            for line in &lines {
                let end = start + line.len() as u64 + 1;
                let page = read_row_window(&fixture.app.tool, &log, start, end)
                    .await
                    .unwrap();
                state.ingest(
                    start,
                    end,
                    &page
                        .records
                        .values()
                        .map(|record| record.event.clone())
                        .collect::<Vec<_>>(),
                );
                state.delta("chat");
                start = end;
            }
            let mut full = read_row_window(&fixture.app.tool, &log, 0, start)
                .await
                .unwrap();
            full.shown = state.shown.clone();
            assert_eq!(state.rows(), full.rows_full());
            assert_eq!(
                state.usage.value(),
                view::usage(&full.records.values().cloned().collect::<Vec<_>>())
            );
        }
    }
    #[cfg(target_os = "macos")]
    fn allocated_bytes() -> usize {
        #[repr(C)]
        #[derive(Default)]
        struct Statistics {
            blocks_in_use: u32,
            size_in_use: usize,
            max_size_in_use: usize,
            size_allocated: usize,
        }
        unsafe extern "C" {
            fn malloc_zone_statistics(zone: *mut std::ffi::c_void, statistics: *mut Statistics);
        }
        let mut statistics = Statistics::default();
        // Apple's malloc.h specifies that a null zone sums all allocation zones.
        unsafe {
            malloc_zone_statistics(std::ptr::null_mut(), &mut statistics);
        }
        statistics.size_in_use
    }
    fn percentile(values: &mut [u128]) -> u128 {
        values.sort_unstable();
        values[values.len() * 95 / 100]
    }
    #[cfg(target_os = "macos")]
    fn live_soak(keys: usize, tools_only: bool) {
        let mut start_times = Vec::with_capacity(1000 * keys);
        let mut end_times = Vec::with_capacity(1000 * keys);
        let mut start_bytes = Vec::with_capacity(1000 * keys);
        let mut end_bytes = Vec::with_capacity(1000 * keys);
        let baseline = allocated_bytes();
        let mut states = (0..keys)
            .map(|_| LiveRows::empty(0, false))
            .collect::<Vec<_>>();
        let mut peak = 0;
        let mut warmed = 0;
        for index in 0..30_000 {
            for state in &mut states {
                let event = synthetic_event(index, tools_only);
                let timer = std::time::Instant::now();
                state.ingest(index as u64, index as u64 + 1, &[event]);
                let frame = state.delta("chat");
                let elapsed = timer.elapsed().as_nanos();
                if (1000..2000).contains(&index) {
                    start_times.push(elapsed);
                    start_bytes.push(frame.data.len() as u128);
                }
                if index >= 29_000 {
                    end_times.push(elapsed);
                    end_bytes.push(frame.data.len() as u128);
                }
                if index % 4 == 3 {
                    let idle = state.delta("chat");
                    let payload: Value = serde_json::from_str(&idle.data).unwrap();
                    assert_eq!(payload["upsert"], json!([]));
                }
            }
            if index % 50 == 49 {
                peak = peak.max(allocated_bytes().saturating_sub(baseline));
            }
            if index == 9999 {
                warmed = allocated_bytes().saturating_sub(baseline);
            }
        }
        let retained = allocated_bytes().saturating_sub(baseline);
        println!(
            "soak keys={keys} tools_only={tools_only} simulated_seconds=600 lines_per_second=50 start_p95_ns={} end_p95_ns={} start_frame_p95_bytes={} end_frame_p95_bytes={} peak_retained_heap_bytes={peak} warmed_retained_heap_bytes={warmed} final_retained_heap_bytes={retained} records={:?}",
            percentile(&mut start_times),
            percentile(&mut end_times),
            percentile(&mut start_bytes),
            percentile(&mut end_bytes),
            states
                .iter()
                .map(|state| state.records.len())
                .collect::<Vec<_>>()
        );
        if tools_only {
            let state = &mut states[0];
            let timer = std::time::Instant::now();
            state.ingest(
                30_000,
                30_001,
                &[json!({"type":"turn_ended","reason":"completed"})],
            );
            let frame = state.delta("chat");
            println!(
                "turn_ended_ms={} frame_bytes={} retained_heap_bytes={} records={}",
                timer.elapsed().as_secs_f64() * 1000.,
                frame.data.len(),
                allocated_bytes().saturating_sub(baseline),
                state.records.len()
            );
        } else {
            assert!(
                states
                    .iter()
                    .all(|state| state.records.len() <= ROW_RECORD_LIMIT)
            );
            assert!(
                retained < warmed * 5 / 4,
                "closed-turn heap grew after the cache filled"
            );
        }
        assert!(
            percentile(&mut end_times) < percentile(&mut start_times) * 3 + 100_000,
            "live flush cost grew with history"
        );
        assert!(
            percentile(&mut end_bytes) < percentile(&mut start_bytes) * 2,
            "live frame bytes grew with history"
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "release performance check, 5 keys x 50 records/s x 600 simulated seconds"]
    fn live_rows_closed_turn_soak() {
        live_soak(5, false);
    }
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "release performance check, 1 open turn x 50 tools/s x 600 simulated seconds"]
    fn live_rows_open_turn_soak() {
        live_soak(1, true);
    }
    #[test]
    #[ignore = "release baseline for the previous full-history rebuild"]
    fn live_rows_full_rebuild_baseline() {
        for count in [1000, 30000] {
            let mut state = LiveRows::empty(0, false);
            for index in 0..count {
                state.ingest(
                    index as u64,
                    index as u64 + 1,
                    &[synthetic_event(index, false)],
                );
            }
            let mut samples = Vec::new();
            for _ in 0..20 {
                let timer = std::time::Instant::now();
                let frame = reference_delta(&mut state, "chat");
                std::hint::black_box(frame.to_string());
                samples.push(timer.elapsed().as_nanos());
            }
            println!(
                "full_rebuild records={count} p95_ns={}",
                percentile(&mut samples)
            );
        }
    }
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "release cadence check during a 30000-tool turn-end rebuild"]
    async fn large_turn_end_keeps_other_keys_running() {
        let fixture = Fixture::new();
        let mut state = LiveRows::empty(0, false);
        state.ingest(
            0,
            30_000,
            &(0..30_000)
                .map(|number| synthetic_event(number, true))
                .collect::<Vec<_>>(),
        );
        state.delta("large");
        {
            let mut hub = fixture.app.hub.lock().unwrap();
            let mut stream = client(&[("large", 30_000), ("small", 0)]);
            stream.row_keys = HashSet::from(["large".into(), "small".into()]);
            hub.streams.insert("cadence".into(), stream);
            for (key, rows, from) in [
                ("large", state, 30_000),
                ("small", LiveRows::empty(0, false), 0),
            ] {
                let (stop, _) = watch::channel(false);
                let (_, done) = watch::channel(false);
                hub.followers.insert(
                    key.into(),
                    Follower {
                        generation: "cadence".into(),
                        from,
                        stop,
                        done,
                        rows: Some(rows),
                        rendering_rows: false,
                    },
                );
            }
        }
        let weak = Arc::downgrade(&fixture.app);
        let heavy = tokio::spawn(async move {
            let mut batches = vec![(
                30_001,
                vec![json!({"type":"turn_ended","reason":"completed"})],
            )];
            flush_transcript(&weak, "large", "cadence", &mut batches)
                .await
                .unwrap();
        });
        let mut ticks = Vec::new();
        let mut previous = std::time::Instant::now();
        for index in 0..15 {
            tokio::time::sleep(Duration::from_millis(16)).await;
            ticks.push(previous.elapsed().as_nanos());
            previous = std::time::Instant::now();
            let mut batches = vec![(index + 1, vec![synthetic_event(index as usize, false)])];
            flush_transcript(
                &Arc::downgrade(&fixture.app),
                "small",
                "cadence",
                &mut batches,
            )
            .await
            .unwrap();
        }
        heavy.await.unwrap();
        println!(
            "other_key_cadence_p95_ms={} worst_ms={}",
            percentile(&mut ticks) as f64 / 1_000_000.,
            *ticks.iter().max().unwrap() as f64 / 1_000_000.
        );
        assert!(
            percentile(&mut ticks) < 40_000_000,
            "another key stalled during turn end"
        );
        assert_eq!(
            fixture.app.hub.lock().unwrap().streams["cadence"].keys["small"],
            15
        );
    }
    #[tokio::test]
    #[ignore = "release HTTP-path costs on a 100000-line synthetic chat"]
    async fn row_http_paths_on_large_synthetic_chat() {
        let fixture = Fixture::new();
        let full = (0..100_000)
            .map(|index| format!("line {index}\n"))
            .collect::<String>();
        let call = json!({"type":"assistant","uuid":"giant","message":{"content":[{"type":"tool_use","id":"giant","name":"Bash","input":{"command":"print"}}]}});
        let result = json!({"type":"user","uuid":"giant-result","message":{"content":[{"type":"tool_result","tool_use_id":"giant","content":full}]}});
        let beginning = format!("{call}\n{result}\n");
        let tail=(0..100_000).map(|index|json!({"type":"user","uuid":format!("u-{index}"),"message":{"content":format!("message-{index}")}}).to_string()+"\n").collect::<String>();
        std::fs::write(&fixture.log, &beginning).unwrap();
        let giant = row_page(&fixture, 2, None).await.unwrap();
        std::fs::write(&fixture.log, beginning + &tail).unwrap();
        let direct = giant["rows"][0]["tool"]["output"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let legacy = giant["rows"][0]["id"].as_str().unwrap().to_owned();
        let mut cold = Vec::new();
        for _ in 0..20 {
            fixture.app.hub.lock().unwrap().row_cache.clear();
            let timer = std::time::Instant::now();
            row_page(&fixture, 200, None).await.unwrap();
            cold.push(timer.elapsed().as_nanos());
        }
        let mut warm = Vec::new();
        let mut older = Vec::new();
        for _ in 0..20 {
            let timer = std::time::Instant::now();
            let page = row_page(&fixture, 200, None).await.unwrap();
            warm.push(timer.elapsed().as_nanos());
            let timer = std::time::Instant::now();
            row_page(&fixture, 200, page["page"]["start_offset"].as_u64())
                .await
                .unwrap();
            older.push(timer.elapsed().as_nanos());
        }
        let mut output = Vec::new();
        let mut legacy_output = Vec::new();
        for (id, times) in [(&direct, &mut output), (&legacy, &mut legacy_output)] {
            for _ in 0..20 {
                let timer = std::time::Instant::now();
                let Json(value) = tool_output(
                    State(fixture.app.clone()),
                    Query(ToolOutputQuery {
                        session: fixture.session.clone(),
                        agent: "chair".into(),
                        row: id.clone(),
                    }),
                )
                .await
                .unwrap();
                assert_eq!(value["text"], full);
                times.push(timer.elapsed().as_nanos());
            }
        }
        println!(
            "http_100k cold_p95_ms={} warm_p95_ms={} older_p95_ms={} giant_direct_p95_ms={} giant_row_id_p95_ms={}",
            percentile(&mut cold) as f64 / 1_000_000.,
            percentile(&mut warm) as f64 / 1_000_000.,
            percentile(&mut older) as f64 / 1_000_000.,
            percentile(&mut output) as f64 / 1_000_000.,
            percentile(&mut legacy_output) as f64 / 1_000_000.
        );
        assert!(percentile(&mut cold) < 150_000_000);
        assert!(percentile(&mut warm) <= 100_000_000);
        assert!(percentile(&mut output) <= 150_000_000);
        assert!(percentile(&mut legacy_output) <= 150_000_000);
    }
    #[test]
    fn two_hundred_fifty_tools_make_three_stable_folds() {
        let records = (0..250)
            .map(|number| {
                Record::new(
                    synthetic_event(number, true),
                    format!("raw-{number}-0"),
                    false,
                )
            })
            .collect::<Vec<_>>();
        let rows = view::view_rows(&records, false, false, &HashSet::new());
        assert_eq!(
            rows.iter().map(|row| row.steps.len()).collect::<Vec<_>>(),
            [100, 100, 50]
        );
        let first = view::view_rows(&records[..100], false, false, &HashSet::new());
        assert_eq!(rows[0].id, first[0].id);
    }
    #[test]
    fn row_frames_report_changed_usage_and_clear_it() {
        let mut state = LiveRows::empty(0, false);
        let usage = json!({"type":"usage","kind":"context","context_tokens":80,"context_capacity_tokens":100});
        state.ingest(0, 1, &[usage]);
        let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
        assert_eq!(payload["usage"]["remaining_percent"], 20);
        let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
        assert!(payload.get("usage").is_none());
        state.ingest(1, 2, &[json!({"type":"usage","kind":"context","context_tokens":90,"context_capacity_tokens":100})]);
        let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
        assert_eq!(payload["usage"]["remaining_percent"], 10);
        state.ingest(
            2,
            3,
            &[json!({"type":"system_message","kind":"compaction"})],
        );
        let payload: Value = serde_json::from_str(&state.delta("chat").data).unwrap();
        assert!(payload.get("usage").unwrap().is_null());
    }
    #[tokio::test]
    async fn send_cleans_claude_only_and_refuses_empty_cleaned_text() {
        let fixture = action_fixture();
        let text = "soft\u{ad}hyphen a\u{200c}b c\u{200b}d e\u{85}f 🏴\u{e0067}\u{e007f}";
        for provider in ["claude", "codex", "agy"] {
            let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
            connection
                .execute(
                    "UPDATE session SET chair_provider = ?1 WHERE id = ?2",
                    (provider, &fixture.session),
                )
                .unwrap();
            let _ = call_action(
                &fixture,
                "/api/chat/send",
                json!({"session":fixture.session,"agent":"chair","text":text}),
            )
            .await
            .unwrap();
            let actual =
                std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap();
            assert_eq!(
                actual,
                if provider == "claude" {
                    claude_text::cleaned(text)
                } else {
                    text.into()
                }
            );
        }
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET chair_provider = 'claude' WHERE id = ?1",
                [&fixture.session],
            )
            .unwrap();
        let before = action_calls(&fixture);
        assert_eq!(
            call_action(
                &fixture,
                "/api/chat/send",
                json!({"session":fixture.session,"agent":"chair","text":"\u{200b}"})
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(action_calls(&fixture), before);
        // A child provider has its own rule, independent of the chair.
        connection
            .execute(
                "UPDATE agent SET provider = 'codex' WHERE session_id = ?1 AND id = 'worker'",
                [&fixture.session],
            )
            .unwrap();
        let _ = call_action(
            &fixture,
            "/api/chat/send",
            json!({"session":fixture.session,"agent":"worker","text":text}),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap(),
            text
        );
    }
    #[tokio::test]
    async fn send_with_unknown_provider_preserves_text() {
        let fixture = action_fixture();
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET chair_provider = NULL WHERE id = ?1",
                [&fixture.session],
            )
            .unwrap();
        let text = "a\u{200b}b";
        let _ = call_action(
            &fixture,
            "/api/chat/send",
            json!({"session":fixture.session,"agent":"chair","text":text}),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(fixture.app.root.parent().unwrap().join("text")).unwrap(),
            text
        );
    }
    fn queue_log(operation: &str, text: &str) -> String {
        json!({"type":"queue-operation","operation":operation,"content":text}).to_string() + "\n"
    }
    fn pull_back_fixture(records: &str, after: &str) -> Fixture {
        let fixture = action_fixture();
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET adapter = 'tmux-solo' WHERE id = ?1",
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
    #[tokio::test]
    async fn pull_back_route_decides_records_then_clears_only_confirmed_boxes() {
        for (after, outcome, text, clears) in [
            (
                queue_log("popAll", "first") + &queue_log("popAll", "second"),
                "pulled",
                Some("first\nsecond"),
                5,
            ),
            (
                queue_log("popAll", "first") + &queue_log("remove", "second"),
                "pulled",
                Some("first"),
                3,
            ),
            (
                queue_log("dequeue", "") + &queue_log("dequeue", ""),
                "alreadySent",
                None,
                3,
            ),
            (String::new(), "unconfirmed", None, 0),
        ] {
            let fixture = pull_back_fixture(
                &(queue_log("enqueue", "first") + &queue_log("enqueue", "second")),
                &after,
            );
            let Json(result) = call_action(
                &fixture,
                "/api/chat/pull-back",
                json!({"session":fixture.session,"agent":"chair"}),
            )
            .await
            .unwrap();
            assert_eq!(result, json!({"ok":true,"outcome":outcome,"text":text}));
            let calls = action_calls(&fixture);
            assert_eq!(
                calls.lines().filter(|line| line.ends_with(" Up")).count(),
                1
            );
            assert_eq!(
                calls.lines().filter(|line| line.ends_with(" C-u")).count(),
                clears
            );
        }
    }
    #[tokio::test]
    async fn pull_back_route_refuses_agent_queue_and_unsupported_targets_without_up() {
        for notification in ["<task-notification>needed", "<agent-message>needed"] {
            let fixture = pull_back_fixture(
                &(queue_log("enqueue", notification) + &queue_log("enqueue", "owner")),
                "",
            );
            assert_eq!(
                call_action(
                    &fixture,
                    "/api/chat/pull-back",
                    json!({"session":fixture.session,"agent":"chair"})
                )
                .await
                .unwrap_err()
                .0,
                StatusCode::CONFLICT
            );
            assert!(action_calls(&fixture).is_empty());
        }
        for (provider, adapter) in [
            ("claude", "herdr"),
            ("codex", "tmux"),
            ("claude", "fixture"),
        ] {
            let fixture = pull_back_fixture(&queue_log("enqueue", "owner"), "");
            let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
            connection
                .execute(
                    "UPDATE session SET chair_provider=?1,adapter=?2 WHERE id=?3",
                    (provider, adapter, &fixture.session),
                )
                .unwrap();
            assert_eq!(
                call_action(
                    &fixture,
                    "/api/chat/pull-back",
                    json!({"session":fixture.session,"agent":"chair"})
                )
                .await
                .unwrap_err()
                .0,
                StatusCode::CONFLICT
            );
            assert!(action_calls(&fixture).is_empty());
        }
        let fixture = pull_back_fixture(&queue_log("enqueue", "<agent-message>needed"), "");
        let Json(result) = call_action(
            &fixture,
            "/api/chat/pull-back",
            json!({"session":fixture.session,"agent":"chair"}),
        )
        .await
        .unwrap();
        assert_eq!(
            result,
            json!({"ok":true,"outcome":"alreadySent","text":null})
        );
        assert!(action_calls(&fixture).is_empty());
    }

    #[tokio::test]
    async fn pages() {
        let fixture = Fixture::new();
        codex_log(&fixture, 245);
        let first = row_page(&fixture, 100, None).await.unwrap();
        assert_eq!(
            row_texts(&first),
            (145..245)
                .map(|number| format!("message-{number}"))
                .collect::<Vec<_>>()
        );
        let second = row_page(&fixture, 100, first["page"]["start_offset"].as_u64())
            .await
            .unwrap();
        assert_eq!(
            row_texts(&second),
            (45..145)
                .map(|number| format!("message-{number}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(second["rows"].as_array().unwrap().len(), 100);
        let all = row_page(&fixture, 100, second["page"]["start_offset"].as_u64())
            .await
            .unwrap();
        assert_eq!(
            row_texts(&all),
            (0..45)
                .map(|number| format!("message-{number}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(all["page"]["start_offset"], 0);
        let again = row_page(&fixture, 100, Some(0)).await.unwrap();
        assert_eq!(again["rows"], json!([]));
    }
    #[tokio::test]
    async fn append_while_paging() {
        let fixture = Fixture::new();
        codex_log(&fixture, 200);
        let first = row_page(&fixture, 100, None).await.unwrap();
        fixture
            .app
            .hub
            .lock()
            .unwrap()
            .streams
            .insert("live".into(), client(&[]));
        let key = format!("{}:chair", fixture.session);
        let _ = subscribe(
            State(fixture.app.clone()),
            Json(Subscription {
                stream: "live".into(),
                keys: vec![SubscriptionKey {
                    key: key.clone(),
                    from: first["page"]["end_offset"].as_u64().unwrap(),
                    view: Some("rows".into()),
                }],
            }),
        )
        .await
        .unwrap();
        let older = row_page(&fixture, 100, first["page"]["start_offset"].as_u64());
        let append = async {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&fixture.log)
                .unwrap()
                .write_all(codex_line(200).as_bytes())
                .unwrap();
        };
        let (page, ()) = tokio::join!(older, append);
        page.unwrap();
        let size = std::fs::metadata(&fixture.log).unwrap().len();
        until_cursor(&fixture.app, "live", &key, size).await;
        {
            let hub = fixture.app.hub.lock().unwrap();
            let state = hub.followers[&key].rows.as_ref().unwrap();
            let texts: Vec<_> = state
                .rows()
                .iter()
                .filter(|row| row.kind == view::RowKind::User)
                .map(|row| row.text.clone())
                .collect();
            assert_eq!(
                texts,
                (0..=200)
                    .map(|number| format!("message-{number}"))
                    .collect::<Vec<_>>()
            );
        }
        // A fresh tail request must preserve history already held by a live row key.
        assert_eq!(
            row_texts(&row_page(&fixture, 1, None).await.unwrap()).len(),
            201
        );
        let mut done = fixture.app.hub.lock().unwrap().followers[&key].done.clone();
        drop(Events {
            app: fixture.app.clone(),
            id: "live".into(),
        });
        done.wait_for(|finished| *finished).await.unwrap();
    }
    #[tokio::test]
    async fn retry_history() {
        let fixture = Fixture::new();
        codex_log(&fixture, 150);
        let data = std::fs::read(&fixture.log).unwrap();
        let first = row_page(&fixture, 100, None).await.unwrap();
        let cursor = first["page"]["start_offset"].as_u64().unwrap();
        std::fs::remove_file(&fixture.log).unwrap();
        assert!(row_page(&fixture, 100, Some(cursor)).await.is_err());
        assert_eq!(fixture.app.hub.lock().unwrap().row_cache[0].1.start, cursor);
        std::fs::write(&fixture.log, data).unwrap();
        let all = row_page(&fixture, 100, Some(cursor)).await.unwrap();
        assert_eq!(
            row_texts(&all),
            (0..50)
                .map(|number| format!("message-{number}"))
                .collect::<Vec<_>>()
        );
    }
    #[tokio::test]
    async fn empty() {
        let fixture = Fixture::new();
        std::fs::write(&fixture.log, "").unwrap();
        let started = std::time::Instant::now();
        let page = row_page(&fixture, 100, None).await.unwrap();
        assert_eq!(page["rows"], json!([]));
        assert_eq!(page["page"]["start_offset"], 0);
        assert_eq!(page["page"]["end_offset"], 0);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    #[tokio::test]
    async fn multiple_events() {
        let fixture = Fixture::new();
        let content: Vec<_> = (0..120).map(|index| json!({"type":"tool_use","id":format!("call-{index}"),"name":"Read","input":{}})).collect();
        std::fs::write(&fixture.log, json!({"type":"assistant","sessionId":"test","uuid":"many","message":{"content":content}}).to_string()+"\n").unwrap();
        let page = row_page(&fixture, 1, None).await.unwrap();
        assert_eq!(page["page"]["start_offset"], 0);
        assert_eq!(page["rows"].as_array().unwrap().len(), 2);
        assert_eq!(page["rows"][0]["steps"].as_array().unwrap().len(), 100);
        assert_eq!(page["rows"][1]["steps"].as_array().unwrap().len(), 20);
    }
    #[tokio::test]
    async fn boundary_tool() {
        let fixture = Fixture::new();
        codex_log(&fixture, 0);
        let old = "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"call_id\":\"boundary\",\"name\":\"shell\",\"arguments\":\"{\\\"command\\\":\\\"true\\\"}\"}}\n";
        let result = "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"boundary\",\"output\":\"done\"}}\n";
        std::fs::write(&fixture.log, old.to_owned() + result).unwrap();
        let first = row_page(&fixture, 1, None).await.unwrap();
        assert_eq!(first["rows"][0]["kind"], "toolResult");
        let orphan_id = first["rows"][0]["id"].clone();
        let all = row_page(&fixture, 1, first["page"]["start_offset"].as_u64())
            .await
            .unwrap();
        assert_eq!(all["rows"].as_array().unwrap().len(), 1);
        assert_eq!(all["rows"][0]["tool"]["output"]["text"], "done");
        assert_eq!(all["rows"][0]["tool"]["state"], "finished");
        assert_eq!(all["remove"], json!([orphan_id]));
        let Json(output) = tool_output(
            State(fixture.app.clone()),
            Query(ToolOutputQuery {
                session: fixture.session.clone(),
                agent: "chair".into(),
                row: all["rows"][0]["id"].as_str().unwrap().into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(output["text"], "done");
    }
    #[tokio::test]
    async fn giant_output_stays_small_and_full_output_is_exact() {
        let fixture = Fixture::new();
        let full = (0..100000)
            .map(|index| format!("line {index}\n"))
            .collect::<String>();
        let call = json!({"type":"assistant","uuid":"call","message":{"content":[{"type":"tool_use","id":"giant","name":"Bash","input":{"command":"print"}}]}});
        let result = json!({"type":"user","uuid":"result","message":{"content":[{"type":"tool_result","tool_use_id":"giant","content":full}]}});
        std::fs::write(&fixture.log, format!("{call}\n{result}\n")).unwrap();
        let page = row_page(&fixture, 200, None).await.unwrap();
        assert!(page.to_string().len() < 20000);
        assert_eq!(page["rows"][0]["tool"]["output"]["full_lines"], 100000);
        assert_eq!(page["rows"][0]["tool"]["output"]["full_bytes"], full.len());
        let Json(output) = tool_output(
            State(fixture.app.clone()),
            Query(ToolOutputQuery {
                session: fixture.session.clone(),
                agent: "chair".into(),
                row: page["rows"][0]["tool"]["output"]["id"]
                    .as_str()
                    .unwrap()
                    .into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(output["text"], full);
        assert_eq!(
            tool_output(
                State(fixture.app.clone()),
                Query(ToolOutputQuery {
                    session: fixture.session.clone(),
                    agent: "chair".into(),
                    row: "raw-1-0:output".into()
                })
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::NOT_FOUND
        );
    }
    #[tokio::test]
    async fn rows_and_raw_clients_share_one_follower_and_stable_tool_id() {
        let fixture = Fixture::new();
        let call = json!({"type":"assistant","uuid":"call","message":{"content":[{"type":"tool_use","id":"live-tool","name":"Bash","input":{"command":"print"}}]}}).to_string()+"\n";
        std::fs::write(&fixture.log, call).unwrap();
        let page = row_page(&fixture, 200, None).await.unwrap();
        let call_id = page["rows"][0]["id"].clone();
        let key = format!("{}:chair", fixture.session);
        let from = page["page"]["end_offset"].as_u64().unwrap();
        {
            let mut hub = fixture.app.hub.lock().unwrap();
            hub.streams.insert("rows".into(), client(&[]));
            hub.streams.insert("raw".into(), client(&[]));
        }
        for (stream, view) in [("rows", Some("rows")), ("raw", None)] {
            let _ = subscribe(
                State(fixture.app.clone()),
                Json(Subscription {
                    stream: stream.into(),
                    keys: vec![SubscriptionKey {
                        key: key.clone(),
                        from,
                        view: view.map(str::to_owned),
                    }],
                }),
            )
            .await
            .unwrap();
        }
        assert_eq!(fixture.app.hub.lock().unwrap().followers.len(), 1);
        let result = json!({"type":"user","uuid":"result","message":{"content":[{"type":"tool_result","tool_use_id":"live-tool","content":"finished output"}]}}).to_string()+"\n";
        std::fs::OpenOptions::new()
            .append(true)
            .open(&fixture.log)
            .unwrap()
            .write_all(result.as_bytes())
            .unwrap();
        until_cursor(&fixture.app, "rows", &key, from + result.len() as u64).await;
        until_cursor(&fixture.app, "raw", &key, from + result.len() as u64).await;
        {
            let hub = fixture.app.hub.lock().unwrap();
            let frame = hub.streams["rows"].queue.back().unwrap();
            assert_eq!(frame.name, "rows");
            let rows: Value = serde_json::from_str(&frame.data).unwrap();
            assert_eq!(rows["upsert"][0]["id"], call_id);
            assert_eq!(rows["upsert"][0]["tool"]["state"], "finished");
            assert_eq!(hub.streams["raw"].queue.back().unwrap().name, "transcript");
        }
        let mut done = fixture.app.hub.lock().unwrap().followers[&key].done.clone();
        drop(Events {
            app: fixture.app.clone(),
            id: "rows".into(),
        });
        {
            let hub = fixture.app.hub.lock().unwrap();
            assert!(hub.followers[&key].rows.is_none());
            assert_eq!(hub.row_cache.len(), 1);
        }
        // A page read while only raw listeners remain belongs to the bounded inactive cache.
        row_page(&fixture, 200, None).await.unwrap();
        assert!(
            fixture.app.hub.lock().unwrap().followers[&key]
                .rows
                .is_none()
        );
        drop(Events {
            app: fixture.app.clone(),
            id: "raw".into(),
        });
        done.wait_for(|finished| *finished).await.unwrap();
        let hub = fixture.app.hub.lock().unwrap();
        assert!(hub.followers.is_empty());
        assert_eq!(hub.row_cache.len(), 1);
    }
    #[test]
    fn cold_row_cache_keeps_only_four_keys() {
        let fixture = Fixture::new();
        let mut hub = fixture.app.hub.lock().unwrap();
        for number in 0..12 {
            hub.cache_rows(format!("chat-{number}"), LiveRows::empty(0, false));
        }
        assert_eq!(hub.row_cache.len(), 4);
        assert_eq!(hub.row_cache[0].0, "chat-8");
    }
    #[tokio::test]
    async fn twelve_row_keys_keep_live_state_and_drop_all_children() {
        let fixture = Fixture::new();
        codex_log(&fixture, 2);
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        for number in 0..12 {
            let agent = format!("worker-{number}");
            swarm::store::add_agent(&connection, &fixture.session, &agent, "chat").unwrap();
            swarm::store::set_provider(&connection, &fixture.session, &agent, "codex").unwrap();
            swarm::store::set_log(&connection, &fixture.session, &agent, &fixture.log).unwrap();
        }
        drop(connection);
        fixture
            .app
            .hub
            .lock()
            .unwrap()
            .streams
            .insert("all".into(), client(&[]));
        let from = std::fs::metadata(&fixture.log).unwrap().len();
        let keys = (0..12)
            .map(|number| SubscriptionKey {
                key: format!("{}:worker-{number}", fixture.session),
                from,
                view: Some("rows".into()),
            })
            .collect();
        let _ = subscribe(
            State(fixture.app.clone()),
            Json(Subscription {
                stream: "all".into(),
                keys,
            }),
        )
        .await
        .unwrap();
        let mut receivers = {
            let hub = fixture.app.hub.lock().unwrap();
            assert_eq!(hub.followers.len(), 12);
            assert!(hub.followers.values().all(|follower| {
                follower
                    .rows
                    .as_ref()
                    .is_some_and(|rows| rows.records.len() == 2)
            }));
            hub.followers
                .values()
                .map(|follower| follower.done.clone())
                .collect::<Vec<_>>()
        };
        drop(Events {
            app: fixture.app.clone(),
            id: "all".into(),
        });
        for done in &mut receivers {
            tokio::time::timeout(Duration::from_secs(2), done.wait_for(|finished| *finished))
                .await
                .unwrap()
                .unwrap();
        }
        let hub = fixture.app.hub.lock().unwrap();
        assert!(hub.followers.is_empty());
        assert_eq!(hub.row_cache.len(), 4);
    }
    #[tokio::test]
    async fn full_diff_uses_source_ordinal_and_retains_all_lines() {
        let fixture = Fixture::new();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("packages/transcript/src/fixtures/claude-edit.jsonl"),
            &fixture.log,
        )
        .unwrap();
        let page = row_page(&fixture, 200, None).await.unwrap();
        let locator = page["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|row| row["diff"]["id"].as_str())
            .unwrap();
        let Json(output) = tool_output(
            State(fixture.app.clone()),
            Query(ToolOutputQuery {
                session: fixture.session.clone(),
                agent: "chair".into(),
                row: locator.into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(locator, "raw-0-1:diff");
        let diff = page["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["diff"]["id"] == locator)
            .unwrap();
        assert_eq!(output["text"], diff["diff"]["patch"]);
        assert!(
            output["text"]
                .as_str()
                .unwrap()
                .contains("@@ -445,6 +445,7 @@")
        );
    }
    #[tokio::test]
    async fn choices_handler_uses_shared_lane_rejects_stale_and_bumps_version() {
        let fixture = action_fixture();
        let workspace = env!("CARGO_MANIFEST_DIR");
        refresh_tree(&fixture.app).unwrap();
        let first = choices_store::load(&fixture.app.root, true).unwrap();
        let version = fixture.app.hub.lock().unwrap().version;
        let body: choices::ChoiceWrite = serde_json::from_value(json!({"revision": first.revision, "op":"rename_workspace", "workspace":workspace, "name":"New title"})).unwrap();
        assert!(Action::Choices(body.clone()).lane().is_none());
        let reply = choice_write(State(fixture.app.clone()), Ok(Json(body.clone())))
            .await
            .unwrap()
            .0;
        assert!(reply["version"].as_u64().unwrap() > version);
        let bytes = std::fs::read(fixture.app.root.join("choices.json")).unwrap();
        let stale = choice_write(State(fixture.app.clone()), Ok(Json(body)))
            .await
            .unwrap_err();
        assert_eq!(stale.0, StatusCode::CONFLICT);
        assert_eq!(
            std::fs::read(fixture.app.root.join("choices.json")).unwrap(),
            bytes
        );
        let Json(tree) = tree_view(State(fixture.app.clone()), Query(TreeQuery { view: None }))
            .await
            .unwrap();
        assert!(tree.to_string().contains("New title"));
        let Json(tabs) = tabs_view(
            State(fixture.app.clone()),
            Query(TabsQuery {
                workspace: workspace.into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(tabs["revision"], reply["revision"]);
        let version = fixture.app.hub.lock().unwrap().version;
        let modified = std::fs::metadata(fixture.app.root.join("choices.json"))
            .unwrap()
            .modified()
            .unwrap();
        let before_choices = choices_store::load(&fixture.app.root, true).unwrap().owner;
        refresh_tree(&fixture.app).unwrap();
        assert_eq!(
            choices_store::load(&fixture.app.root, true).unwrap().owner,
            before_choices
        );
        assert_eq!(
            std::fs::metadata(fixture.app.root.join("choices.json"))
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        assert_eq!(fixture.app.hub.lock().unwrap().version, version);
    }
    #[tokio::test]
    async fn tree_handler_keeps_last_good_choices_on_busy_or_invalid_state() {
        let fixture = Fixture::new();
        refresh_tree(&fixture.app).unwrap();
        choices_store::update(&fixture.app.root, None, |owner, _| {
            owner
                .names
                .insert(env!("CARGO_MANIFEST_DIR").into(), "External name".into());
            Ok(())
        })
        .unwrap();
        let request = || tree_view(State(fixture.app.clone()), Query(TreeQuery { view: None }));
        assert!(
            request()
                .await
                .unwrap()
                .0
                .to_string()
                .contains("External name")
        );
        let held = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(fixture.app.root.join("choices.lock"))
            .unwrap();
        held.lock().unwrap();
        let busy = request().await.unwrap().0;
        assert!(busy["choicesError"].is_string());
        assert!(busy.to_string().contains("External name"));
        drop(held);
        std::fs::write(fixture.app.root.join("view-state.json"), "{broken").unwrap();
        let broken = request().await.unwrap().0;
        assert!(broken["choicesError"].is_string());
        assert!(broken.to_string().contains("External name"));
        std::fs::remove_file(fixture.app.root.join("view-state.json")).unwrap();
        assert!(request().await.unwrap().0["choicesError"].is_null());
        std::fs::write(fixture.app.root.join("choices.json"), "{broken").unwrap();
        assert!(
            !request().await.unwrap().0["sections"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(!fixture.app.root.join("choices.json").exists());
    }

    #[test]
    fn lifecycle_routes_are_exercised_without_sockets() {
        let root = std::env::temp_dir().join(format!("swarm-life-routes-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(
            root.join(".gitconfig"),
            "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir(root.join("bin")).unwrap();
        let git = root.join("bin/git");
        std::fs::write(
            &git,
            r#"#!/bin/sh
if [ "$3" = init ] && [ -f "$HOME/fail-init" ]; then
    if [ -f "$HOME/fail-rollback" ]; then
        store=$(/bin/cat "$HOME/fail-rollback")
        /bin/mv "$store/choices.json" "$store/choices.saved"
        /bin/mkdir "$store/choices.json"
    fi
    printf 'Fixture git init refused\n' >&2
    exit 1
fi
exec /usr/bin/git "$@"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "serve::tests::lifecycle_routes_child",
                "--nocapture",
            ])
            .env_clear()
            .env("HOME", &root)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", root.join("bin").display()),
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", root.join(".gitconfig"))
            .env("GIT_CEILING_DIRECTORIES", &root)
            .env("SWARM_LIFECYCLE_ROUTES", "1")
            .output()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    async fn life_post(fixture: &Fixture, route: &str, body: Value) -> Result<Value, ApiError> {
        lifecycle_write(
            State(fixture.app.clone()),
            route.parse().unwrap(),
            Ok(Json(body)),
        )
        .await
        .map(|v| v.0)
    }
    #[test]
    fn workspace_panels_and_issue_setup_are_exercised_without_sockets() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("swarm-panel-routes-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(
            root.join("bin/gh"),
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/gh-calls"
case "$1 $2" in
 'issue list') printf '[{"number":7,"title":"Fix Sidebar","body":"Use less space"}]\n' ;;
 'issue view') printf '{"number":7,"title":"Fix Sidebar","body":"Use less space"}\n' ;;
 *) exit 1 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(root.join("bin/gh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "serve::tests::workspace_panel_routes_child",
                "--nocapture",
            ])
            .env_clear()
            .env("HOME", &root)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", root.join("bin").display()),
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", root.join(".gitconfig"))
            .env("GIT_CEILING_DIRECTORIES", &root)
            .env("SWARM_PANEL_ROUTES", "1")
            .output()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[tokio::test]
    async fn workspace_panel_routes_child() {
        if std::env::var("SWARM_PANEL_ROUTES").is_err() {
            return;
        }
        let root = PathBuf::from(std::env::var("HOME").unwrap());
        let fixture = action_fixture_at(Fixture::at(root.join("surface")));
        let source = root.join("project").to_string_lossy().into_owned();
        std::fs::create_dir(&source).unwrap();
        lifecycle::check_git(&source, &["init", "-q", "-b", "main"]).unwrap();
        lifecycle::check_git(
            &source,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-qm",
                "base",
            ],
        )
        .unwrap();
        lifecycle::check_git(
            &source,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/project.git",
            ],
        )
        .unwrap();
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET cwd = ?1, chair_log = NULL WHERE id = ?2",
                (&source, &fixture.session),
            )
            .unwrap();
        fixture.app.directories.lock().unwrap().clear();
        choices_store::update(&fixture.app.root, None, |owner, _| {
            owner.project_paths.push(source.clone());
            owner.project_defaults.insert(
                source.clone(),
                json!({"setup_script":"printf 'setting up'; printf failure >&2; exit 9"}),
            );
            Ok(())
        })
        .unwrap();
        let issues = panel_read(
            State(fixture.app.clone()),
            "/api/issues".parse().unwrap(),
            Query(HashMap::from([("project".into(), source.clone())])),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(issues["issues"][0]["number"], 7);
        let made = life_post(&fixture, "/api/workspace/new", json!({"project":source,"mode":"issue","issue":7,"prefix":"swarm/","folder":"../tasks"})).await.unwrap();
        let path = made["workspace"].as_str().unwrap();
        assert_eq!(made["issue"]["title"], "Fix Sidebar");
        assert_eq!(made["issue"]["body"], "Use less space");
        assert_eq!(made["setup"]["state"], "failed");
        assert_eq!(made["setup"]["exit_code"], 9);
        assert!(Path::new(path).is_dir());
        assert_eq!(
            lifecycle::check_git(path, &["symbolic-ref", "--short", "HEAD"]).unwrap(),
            "swarm/7-fix-sidebar"
        );
        let saved = choices_store::load(&fixture.app.root, true).unwrap();
        assert_eq!(saved.state.selected_workspace.as_deref(), Some(path));
        assert!(saved.owner.project_defaults[&source]["setup_script"].is_string());
        std::fs::write(Path::new(path).join("hello.txt"), "hello\n").unwrap();
        let params = HashMap::from([
            ("workspace".into(), path.into()),
            ("path".into(), "hello.txt".into()),
        ]);
        assert_eq!(
            panel_read(
                State(fixture.app.clone()),
                "/api/changes/file".parse().unwrap(),
                Query(params.clone())
            )
            .await
            .unwrap()
            .0["preview"]["text"],
            "hello\n"
        );
        assert_eq!(
            panel_read(
                State(fixture.app.clone()),
                "/api/changes/diff".parse().unwrap(),
                Query(params)
            )
            .await
            .unwrap()
            .0["added"],
            1
        );
        for route in [
            "/api/changes",
            "/api/changes/file",
            "/api/changes/diff",
            "/api/pr",
            "/api/issues",
        ] {
            let key = if route == "/api/issues" {
                "project"
            } else {
                "workspace"
            };
            let params = HashMap::from([(key.into(), root.to_string_lossy().into_owned())]);
            assert_eq!(
                panel_read(
                    State(fixture.app.clone()),
                    route.parse().unwrap(),
                    Query(params)
                )
                .await
                .unwrap_err()
                .0,
                StatusCode::FORBIDDEN
            );
        }
    }
    #[tokio::test]
    async fn lifecycle_routes_child() {
        if std::env::var("SWARM_LIFECYCLE_ROUTES").is_err() {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let root = PathBuf::from(std::env::var("HOME").unwrap());
        let fixture = action_fixture_at(Fixture::at(root.join("surface")));
        let source = root.join("project").to_string_lossy().into_owned();
        let plain = root.join("plain").to_string_lossy().into_owned();
        std::fs::create_dir(&plain).unwrap();
        // Remove the inherited source session so this surface contains only fixture projects.
        let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
        connection
            .execute(
                "UPDATE session SET cwd = ?1, chair_log = NULL WHERE id = ?2",
                (&plain, &fixture.session),
            )
            .unwrap();
        fixture.app.directories.lock().unwrap().clear();
        let create = life_post(
            &fixture,
            "/api/project/create",
            json!({"path":source,"name":"Project title"}),
        )
        .await
        .unwrap();
        assert_eq!(create["firstCommit"]["state"], "made");
        assert_eq!(
            lifecycle::check_git(&source, &["log", "--format=%s"]).unwrap(),
            "Create project"
        );
        assert_eq!(
            choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .project_names[&source],
            "Project title"
        );
        assert!(
            life_post(
                &fixture,
                "/api/project/create",
                json!({"path":source,"name":"Exists"})
            )
            .await
            .is_err()
        );
        let options = lifecycle_read(
            State(fixture.app.clone()),
            "/api/workspace/new-options".parse().unwrap(),
            Query(HashMap::from([("project".into(), source.clone())])),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(options["defaultBranch"], "master"); // The isolated Git has no init.defaultBranch override.
        let base = options["defaultBranch"].as_str().unwrap();
        let made=life_post(&fixture,"/api/workspace/new",json!({"project":source,"name":"Feature","mode":"new-branch","base":base,"prefix":"","folder":"../tasks"})).await.unwrap();
        let path = made["workspace"].as_str().unwrap().to_owned();
        assert_eq!(
            lifecycle::check_git(&path, &["branch", "--show-current"]).unwrap(),
            "feature"
        );
        let initial_version = made["version"].as_u64().unwrap();
        let launch = call_action(&fixture, "/api/chat/new", json!({"cwd":path}))
            .await
            .unwrap()
            .0;
        assert_eq!(launch["ok"], true); // No session existed in the new workspace before this call.
        let impact = lifecycle_read(
            State(fixture.app.clone()),
            "/api/workspace/impact".parse().unwrap(),
            Query(HashMap::from([("workspace".into(), path.clone())])),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(impact["canDelete"], true);
        assert_eq!(impact["liveAgents"], 0);
        assert!(
            impact["deleteBlocker"]
                .as_str()
                .unwrap()
                .contains("unpushed")
        );
        life_post(
            &fixture,
            "/api/workspace/archive",
            json!({"workspace":path}),
        )
        .await
        .unwrap();
        assert!(
            choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .archived
                .contains(&path)
        );
        assert!(Path::new(&path).is_dir());
        assert_eq!(
            call_action(&fixture, "/api/chat/new", json!({"cwd":path}))
                .await
                .unwrap_err()
                .0,
            StatusCode::CONFLICT
        );
        life_post(
            &fixture,
            "/api/workspace/restore",
            json!({"workspace":path}),
        )
        .await
        .unwrap();
        assert!(
            !choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .archived
                .contains(&path)
        );
        assert!(fixture.app.hub.lock().unwrap().version > initial_version);
        // An unanswered import returns the choice without saving its answer.
        let ask = life_post(&fixture, "/api/project/import", json!({"path":plain}))
            .await
            .unwrap();
        assert_eq!(ask["needsChoice"], true);
        assert!(
            !choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .plain_folders
                .contains(&plain)
        );
        life_post(
            &fixture,
            "/api/project/import",
            json!({"path":plain,"as":"folder"}),
        )
        .await
        .unwrap();
        assert_eq!(
            life_post(&fixture, "/api/project/import", json!({"path":plain}))
                .await
                .unwrap()["needsChoice"],
            false
        );
        assert!(!Path::new(&plain).join(".git").exists());
        life_post(
            &fixture,
            "/api/project/import",
            json!({"path":plain,"as":"git"}),
        )
        .await
        .unwrap();
        assert!(Path::new(&plain).join(".git").exists());
        assert!(
            !choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .plain_folders
                .contains(&plain)
        );
        // Dirty and unpushed work must remain before any agent close.
        let marker = Path::new(&path).join("keep.txt");
        std::fs::write(&marker, "keep").unwrap();
        let calls = action_calls(&fixture);
        assert_eq!(
            life_post(&fixture, "/api/workspace/delete", json!({"workspace":path}))
                .await
                .unwrap_err()
                .0,
            StatusCode::CONFLICT
        );
        assert_eq!(action_calls(&fixture), calls);
        assert!(marker.exists());
        std::fs::remove_file(marker).unwrap();
        assert!(
            life_post(&fixture, "/api/workspace/delete", json!({"workspace":path}))
                .await
                .is_err()
        );
        lifecycle::check_git(&source, &["update-ref", "refs/remotes/origin/main", "HEAD"]).unwrap();
        let deleted = life_post(&fixture, "/api/workspace/delete", json!({"workspace":path}))
            .await
            .unwrap();
        assert_eq!(deleted["ok"], true);
        assert!(!Path::new(&path).exists());
        let missing = life_post(
            &fixture,
            "/api/workspace/new",
            json!({"project":source,"name":"Missing","mode":"new-branch","base":base}),
        )
        .await
        .unwrap()["workspace"]
            .as_str()
            .unwrap()
            .to_owned();
        std::fs::remove_dir_all(&missing).unwrap();
        life_post(
            &fixture,
            "/api/workspace/prune",
            json!({"workspace":missing}),
        )
        .await
        .unwrap();
        assert!(
            !lifecycle::check_git(&source, &["worktree", "list", "--porcelain"])
                .unwrap()
                .contains(&missing)
        );
        // A full fresh listing supplies both child and chair live counts before closing.
        connection
            .execute(
                "UPDATE session SET cwd = ?1 WHERE id = ?2",
                (&source, &fixture.session),
            )
            .unwrap();
        let cli = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$SWARM_HOME/actions"
case "$1 $2" in
'agents --json') printf '%s\n' '{{"{id}":{{"agents":[{{"id":"orchestrator","alive":true}},{{"id":"worker","alive":true}}]}}}}' ;;
'close worker') if [ -f "$HOME/change-during-close" ]; then printf keep > "$PWD/new-work.txt"; fi ;;
esac
"#,
            id = fixture.session
        );
        std::fs::write(&fixture.app.cli, cli).unwrap();
        std::fs::set_permissions(&fixture.app.cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let impact = lifecycle_read(
            State(fixture.app.clone()),
            "/api/workspace/impact".parse().unwrap(),
            Query(HashMap::from([("workspace".into(), source.clone())])),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(impact["liveAgents"], 2);
        assert_eq!(impact["confirmationRequired"], true);
        life_post(
            &fixture,
            "/api/workspace/archive",
            json!({"workspace":source}),
        )
        .await
        .unwrap();
        let calls = action_calls(&fixture);
        assert!(calls.rfind("close worker").unwrap() < calls.rfind("close orchestrator").unwrap());
        life_post(
            &fixture,
            "/api/workspace/restore",
            json!({"workspace":source}),
        )
        .await
        .unwrap();
        let clean = life_post(
            &fixture,
            "/api/workspace/new",
            json!({"project":source,"name":"Clean","mode":"new-branch","base":base}),
        )
        .await
        .unwrap()["workspace"]
            .as_str()
            .unwrap()
            .to_owned();
        connection
            .execute(
                "UPDATE session SET cwd = ?1 WHERE id = ?2",
                (&clean, &fixture.session),
            )
            .unwrap();
        std::fs::write(root.join("change-during-close"), "").unwrap();
        assert_eq!(
            life_post(
                &fixture,
                "/api/workspace/delete",
                json!({"workspace":clean})
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            std::fs::read_to_string(Path::new(&clean).join("new-work.txt")).unwrap(),
            "keep"
        );
        std::fs::remove_file(root.join("change-during-close")).unwrap();
        let before = choices_store::load(&fixture.app.root, true).unwrap();
        for route in ["new", "archive", "restore", "delete", "prune"] {
            let value = if route == "new" {
                json!({"project":root.to_string_lossy(),"name":"Outside","mode":"new-branch"})
            } else {
                json!({"workspace":root.to_string_lossy()})
            };
            assert_eq!(
                life_post(&fixture, &format!("/api/workspace/{route}"), value)
                    .await
                    .unwrap_err()
                    .0,
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            life_post(
                &fixture,
                "/api/project/remove",
                json!({"project":root.to_string_lossy()})
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        std::os::unix::fs::symlink(&fixture.app.root, root.join("home-link")).unwrap();
        for (route, value) in [
            (
                "create",
                json!({"path":root.join("home-link/escape").to_string_lossy(),"name":"Escape"}),
            ),
            (
                "import",
                json!({"path":root.join("home-link").to_string_lossy(),"as":"git"}),
            ),
        ] {
            assert_eq!(
                life_post(&fixture, &format!("/api/project/{route}"), value)
                    .await
                    .unwrap_err()
                    .0,
                StatusCode::FORBIDDEN
            );
        }
        let after = choices_store::load(&fixture.app.root, true).unwrap();
        assert_eq!(after.revision, before.revision);
        assert!(!fixture.app.root.join("escape").exists());
        life_post(&fixture, "/api/project/remove", json!({"project":source}))
            .await
            .unwrap();
        assert!(Path::new(&source).is_dir());
        assert!(
            choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .removed_projects
                .contains(&source)
        );
        life_post(&fixture, "/api/project/import", json!({"path":clean}))
            .await
            .unwrap();
        assert!(
            !choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .removed_projects
                .contains(&source)
        );
        // A choices failure removes the empty new folder before any Git init.
        let (tree, saved) = fresh_lifecycle_tree(&fixture.app).unwrap();
        let target = root.join("choices-failed").to_string_lossy().into_owned();
        let marker = fixture.app.root.join("swarm-home");
        std::fs::rename(&marker, fixture.app.root.join("marker.saved")).unwrap();
        assert!(
            apply_lifecycle(
                &fixture.app,
                LifecycleAction::Create(lifecycle::CreateProject {
                    path: target.clone(),
                    name: "Failed title".into()
                }),
                &tree,
                &saved,
                0
            )
            .is_err()
        );
        assert!(!Path::new(&target).exists());
        std::fs::rename(fixture.app.root.join("marker.saved"), marker).unwrap();
        // Git init failures remove only newly remembered paths and retain the first error.
        std::fs::write(root.join("fail-init"), "").unwrap();
        let remembered = root.join("remembered").to_string_lossy().into_owned();
        choices_store::update(&fixture.app.root, None, |owner, _| {
            owner.project_paths.push(remembered.clone());
            Ok(())
        })
        .unwrap();
        for path in [target.clone(), remembered.clone()] {
            let failure = life_post(
                &fixture,
                "/api/project/create",
                json!({"path":path,"name":"Failed title"}),
            )
            .await
            .unwrap_err();
            assert!(
                failure.1.0["error"]
                    .as_str()
                    .unwrap()
                    .contains("Fixture git init refused")
            );
            assert!(!Path::new(&path).exists());
        }
        let owner = choices_store::load(&fixture.app.root, true).unwrap().owner;
        assert!(!owner.project_paths.contains(&target));
        assert!(!owner.project_names.contains_key(&target));
        assert!(owner.project_paths.contains(&remembered));
        std::fs::write(
            root.join("fail-rollback"),
            fixture.app.root.to_string_lossy().as_bytes(),
        )
        .unwrap();
        let failure = life_post(
            &fixture,
            "/api/project/create",
            json!({"path":target,"name":"Failed title"}),
        )
        .await
        .unwrap_err();
        assert!(
            failure.1.0["error"]
                .as_str()
                .unwrap()
                .contains("Fixture git init refused")
        );
        assert!(!Path::new(&target).exists());
        std::fs::remove_dir(fixture.app.root.join("choices.json")).unwrap();
        std::fs::rename(
            fixture.app.root.join("choices.saved"),
            fixture.app.root.join("choices.json"),
        )
        .unwrap();
        assert!(
            choices_store::load(&fixture.app.root, true)
                .unwrap()
                .owner
                .project_paths
                .contains(&target)
        );
    }

    include!("../tests/serve_handoff/mod.rs");
}
