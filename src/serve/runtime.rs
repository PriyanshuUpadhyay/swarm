//! Server runtime.

use super::*;

/// `web/dist`, embedded by `build.rs` when built with the serve feature (ADR 0079). Empty in a dev
/// build that has no `web/dist`; `--assets DIR` replaces it.
pub(super) static WEB_ASSETS: &[(&str, &[u8])] =
    include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (port, assets, dev_origin) = options(args)?;
    start(port.unwrap_or(0), assets, dev_origin, None)
}

pub(super) fn options(args: &[String]) -> Result<Options, Box<dyn std::error::Error>> {
    let mut port = None;
    let mut assets = None;
    let mut dev_origin = None;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or("usage: swarm serve|ui [stop | --foreground] [--port N] [--assets DIR] [--dev-origin ORIGIN]")?;
        match flag.as_str() {
            "--port" => port = Some(value.parse::<u16>()?),
            "--assets" => assets = Some(std::fs::canonicalize(value)?),
            "--dev-origin" => dev_origin = Some(value.clone()),
            _ => {
                return Err(
                    "usage: swarm serve|ui [stop | --foreground] [--port N] [--assets DIR] [--dev-origin ORIGIN]".into(),
                );
            }
        }
    }
    Ok((port, assets, dev_origin))
}

/// Serves until killed. With `record` (`swarm ui`), uses the home's lasting token and writes the URL
/// and this pid there.
pub(super) fn start(
    port: u16,
    assets: Option<PathBuf>,
    dev_origin: Option<String>,
    record: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    drop(swarm::store::open(&root.join("swarm.db"))?);
    let token = match record {
        Some(_) => ui_token(&root)?,
        None => new_token()?,
    };
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).map_err(
        |error| -> Box<dyn std::error::Error> {
            if error.kind() == std::io::ErrorKind::AddrInUse {
                format!("swarm: port {port} is in use by another program; pass --port N").into()
            } else {
                error.into()
            }
        },
    )?;
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
        commands: Mutex::new(HashMap::new()),
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
            .write_all(format!("{url}\n{}\n{BUILD}\n", std::process::id()).as_bytes())?;
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

pub(super) fn router(app: App) -> Router {
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
        .route("/api/hud", get(hud_read).post(hud_write))
        .route("/api/notices", get(notices_read).post(notices_write))
        .route("/api/doctor", get(doctor_read))
        .route("/api/kit", get(kit_read))
        .route("/api/kit/check", post(kit_check))
        .route("/api/setup/plan", get(setup_plan))
        .route("/api/setup/apply", post(setup_write))
        .route(
            "/api/import/conductor",
            get(conductor_plan).post(conductor_write),
        )
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
        .route("/api/background", get(background))
        .route("/api/terminal/ticket", post(terminal_ticket))
        .route(SOCKET_PATH, get(terminal_socket))
        .route("/api/stream", get(stream))
        .route("/api/reload", post(reload))
        .route("/api/show", post(show))
        .route("/api/subscribe", post(subscribe))
        .route("/api/chat/send", post(chat_action))
        .route("/api/chat/pull-back", post(chat_action))
        .route("/api/chat/interrupt", post(chat_action))
        .route("/api/chat/key", post(chat_action))
        .route("/api/chat/answer", post(chat_action))
        .route("/api/chat/commands", get(chat_commands))
        .route("/api/chat/files", get(chat_files))
        .route(
            "/api/chat/image",
            get(chat_image_read)
                .post(chat_image)
                .layer(DefaultBodyLimit::max(IMAGE_LIMIT)),
        )
        .route("/api/agent/close", post(chat_action))
        .route("/api/sessions/archive", post(chat_action))
        .route("/api/sessions/unarchive", post(chat_action))
        .route("/api/chat/new", post(chat_action))
        .route("/api/chat/resume", post(chat_action))
        .route("/api/sessions/archived", get(archived_sessions))
        .route("/api/chats/importable", get(importable_chats))
        .route("/api/ui-error", post(ui_error))
        .fallback(static_file)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}
