//! Workspace shells for the Cmd+J panel. Each is a shell in a tmux session of its own on the
//! `swarm-shell` socket, so it outlives the panel and a server restart, and the list is read from
//! tmux, never from memory. A shell starts in a chat's folder, which the server reads from the
//! session, so the page never names a path, and belongs to the workspace that holds that folder,
//! so every chat of a workspace lists and counts the same shells.

use super::terminal::pty_command;
use super::*;
use portable_pty::CommandBuilder;

#[cfg(not(test))]
const SOCKET: &str = "swarm-shell";
/// A cargo test that reaches tmux never touches the owner's shells.
#[cfg(test)]
const SOCKET: &str = "swarm-shell-test";
/// Names another socket, so a test run never touches the owner's shells.
const SOCKET_VAR: &str = "SWARM_SHELL_SOCKET";

fn socket() -> String {
    std::env::var(SOCKET_VAR).unwrap_or_else(|_| SOCKET.to_owned())
}

/// tmux on the shell socket, with no agent identity: the shell is the owner's, and an agent's
/// SWARM_* or Claude variables in it would make a `swarm` call act as that agent.
fn tmux() -> std::process::Command {
    let mut command = std::process::Command::new("tmux");
    command.args(["-L", &socket(), "-f", "/dev/null"]);
    // The first shell's locale is the tmux server's, and tmux mangles text outside UTF-8.
    command.env("LANG", "en_US.UTF-8");
    for (name, _) in std::env::vars_os() {
        let key = name.to_string_lossy();
        if ["SWARM_", "CLAUDE", "TMUX", "HERDR_"]
            .iter()
            .any(|prefix| key.starts_with(prefix))
            || ["AI_AGENT", "SUPERSET_TERMINAL_ID", "REX_BLOCK"].contains(&key.as_ref())
        {
            command.env_remove(name);
        }
    }
    command
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub(super) struct Shell {
    pub(super) id: String,
    /// What runs in the shell now, as tmux names it.
    pub(super) command: String,
}

/// An id the server made is `shell-` and this many hex digits.
const ID_PREFIX: &str = "shell-";
const ID_HEX_LEN: usize = 8;

/// How many shells one workspace may hold, since each is a login shell that keeps running.
const MAX_SHELLS: usize = 8;

/// Held while a create checks that its workspace is writable, counts its shells, and makes one,
/// and while an archive or delete closes a workspace's shells. So two creates cannot both pass
/// the cap, and a create cannot pass the check before an archive and start its shell after the
/// archive closed the others.
// ponytail: one lock for all folders; per-folder locks if shells start often.
static SHELLS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock_shells() -> std::sync::MutexGuard<'static, ()> {
    SHELLS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Refuses, counts the workspace's shells, and creates one if there is room, all under the lock.
fn create_with_room(
    refuse: impl FnOnce() -> Result<(), ApiError>,
    list: impl FnOnce() -> Result<Vec<Shell>, String>,
    create: impl FnOnce() -> Result<String, String>,
) -> Result<String, ApiError> {
    let _held = lock_shells();
    refuse()?;
    let shells = list().map_err(internal)?;
    check_room(&shells)?;
    create().map_err(internal)
}

fn check_room(shells: &[Shell]) -> Result<(), ApiError> {
    if shells.len() >= MAX_SHELLS {
        return Err(error(
            StatusCode::CONFLICT,
            &format!("A workspace can have {MAX_SHELLS} shells. Close one first."),
        ));
    }
    Ok(())
}

/// An id the server made, so it is safe as a tmux target.
pub(super) fn valid_id(id: &str) -> bool {
    id.strip_prefix(ID_PREFIX)
        .is_some_and(|hex| hex.len() == ID_HEX_LEN && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// tmux prints a tab as `_` outside a UTF-8 locale, so the fields part on `|`, and the folder,
/// which may hold one, comes last.
const FORMAT: &str = "#{session_name}|#{session_created}|#{pane_current_command}|#{@swarm_cwd}";
/// The number of fields in `FORMAT`; the folder, which may hold a `|`, is the last.
const FORMAT_FIELDS: usize = 4;

/// The shells in `list-sessions` output that belong to workspace `cwd`, oldest first. A shell
/// belongs to the deepest workspace that holds its folder, so one inside a workspace in `nested`
/// is not `cwd`'s.
pub(super) fn parse(text: &str, cwd: &str, nested: &[String]) -> Vec<Shell> {
    let mut found: Vec<(i64, Shell)> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(FORMAT_FIELDS, '|');
            let (id, created, command) = (fields.next()?, fields.next()?, fields.next()?);
            (valid_id(id)
                && fields.next().is_some_and(|shell_cwd| {
                    tree::contains(shell_cwd, cwd)
                        && !nested.iter().any(|inner| tree::contains(shell_cwd, inner))
                }))
            .then(|| {
                let shell = Shell {
                    id: id.into(),
                    command: command.into(),
                };
                (created.parse().unwrap_or(0), shell)
            })
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
    found.into_iter().map(|(_, shell)| shell).collect()
}

/// The workspaces of `tree` strictly inside workspace `cwd`.
pub(super) fn nested(tree: &tree::Tree, cwd: &str) -> Vec<String> {
    tree.projects
        .iter()
        .flat_map(|p| &p.workspaces)
        .filter(|w| w.path != cwd && tree::contains(&w.path, cwd))
        .map(|w| w.path.clone())
        .collect()
}

/// An error's text, as the page shows it.
fn api_text(err: &ApiError) -> String {
    err.1.0["error"].as_str().unwrap_or_default().to_owned()
}

/// The workspace that holds `folder` (the folder itself when none does), and the workspaces
/// inside it, whose shells are theirs.
fn workspace(app: &Server, folder: &str) -> Result<(String, Vec<String>), String> {
    let tree = loaded_tree(app).map_err(|err| api_text(&err))?.0;
    Ok(workspace_in(&tree, folder))
}

fn workspace_in(tree: &tree::Tree, folder: &str) -> (String, Vec<String>) {
    let path = tree
        .workspace_containing(folder)
        .map_or_else(|| folder.to_owned(), |(_, w)| w.path.clone());
    let inner = nested(tree, &path);
    (path, inner)
}

/// What a close of `cwd` ends. A workspace that the tree already dropped must not widen to its
/// parent, or the close would end the parent's shells.
fn close_scope(tree: &tree::Tree, cwd: &str) -> (String, Vec<String>) {
    (cwd.to_owned(), nested(tree, cwd))
}

/// How long one tmux call may take. The calls run under the shell lock, and a hung tmux server
/// must not hold it for ever.
const TMUX_LIMIT: Duration = Duration::from_secs(5);

/// Runs `command` and collects its output, or kills it when it outlasts `limit`.
fn run(
    mut command: std::process::Command,
    limit: Duration,
) -> Result<std::process::Output, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("tmux did not run: {err}"))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = std::time::Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "tmux did not answer in {} s",
                    limit.as_secs().max(1)
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(err) => return Err(format!("tmux did not run: {err}")),
        }
    };
    Ok(std::process::Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

/// The shells of the workspace that holds `folder`.
pub(super) fn list(app: &Server, folder: &str) -> Result<Vec<Shell>, String> {
    let (cwd, inner) = workspace(app, folder)?;
    list_in(&cwd, &inner)
}

/// The shells of folder `cwd`, less those of the workspaces `inner` inside it.
fn list_in(cwd: &str, inner: &[String]) -> Result<Vec<Shell>, String> {
    let mut call = tmux();
    call.args(["list-sessions", "-F", FORMAT]);
    let output = run(call, TMUX_LIMIT)?;
    if output.status.success() {
        return Ok(parse(&String::from_utf8_lossy(&output.stdout), cwd, inner));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    // No shell yet: tmux starts its server with the first one.
    if stderr.contains("no server running") || stderr.contains("error connecting") {
        return Ok(Vec::new());
    }
    Err(stderr.trim().to_owned())
}

fn create(cwd: &str) -> Result<String, String> {
    // The end of a v7 uuid is random.
    let uuid = uuid::Uuid::now_v7().simple().to_string();
    let id = format!("{ID_PREFIX}{}", &uuid[uuid.len() - ID_HEX_LEN..]);
    // set-option takes a pane target, where `=` names no session; the id is unique anyway.
    let target = &id;
    let mut call = tmux();
    call.args(["new-session", "-d", "-s", &id, "-c", cwd])
        .args([";", "set-option", "-t", target, "status", "off"])
        // The owner's keys go to the shell, as in a terminal.
        .args([";", "set-option", "-t", target, "prefix", "None"])
        .args([";", "set-option", "-t", target, "@swarm_cwd", cwd]);
    let output = run(call, TMUX_LIMIT)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(id)
}

fn kill(id: &str) -> Result<(), String> {
    let mut call = tmux();
    call.args(["kill-session", "-t", &format!("={id}")]);
    let output = run(call, TMUX_LIMIT)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Ends every shell of the workspace at exactly `cwd`, for an archived or deleted one. A failure only warns, since the archive
/// already ran.
pub(super) fn close_all(app: &Server, cwd: &str) {
    let _held = lock_shells();
    let shells = loaded_tree(app)
        .map_err(|err| api_text(&err))
        .and_then(|tree| {
            let (path, inner) = close_scope(&tree.0, cwd);
            list_in(&path, &inner)
        })
        .unwrap_or_else(|err| {
            eprintln!("swarm: shells of {cwd} were not listed, so none closed: {err}");
            Vec::new()
        });
    for shell in shells {
        if let Err(err) = kill(&shell.id) {
            eprintln!("swarm: shell {} did not close: {err}", shell.id);
        }
    }
}

fn internal(err: String) -> ApiError {
    error(StatusCode::INTERNAL_SERVER_ERROR, &err)
}

/// `tmux attach` for the terminal's PTY, with no agent identity and a terminal type xterm.js draws.
pub(super) fn attach_command(id: &str) -> CommandBuilder {
    let command = tmux();
    let mut builder = pty_command(&command);
    builder.args(["attach-session", "-t", &format!("={id}")]);
    builder.env("TERM", "xterm-256color");
    builder.env("LANG", "en_US.UTF-8");
    builder
}

/// The folder of `session`'s chat, where its shells start. The page lists the shells of the
/// workspace that holds it.
pub(super) fn session_cwd(app: &Server, session: &str) -> Result<String, ApiError> {
    let connection = read_connection(&app.root)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?;
    let cwd = swarm::store::session_cwd(&connection, session)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown session"))?;
    if !Path::new(&cwd).is_dir() {
        return Err(error(StatusCode::CONFLICT, "this chat's folder is gone"));
    }
    Ok(cwd)
}

/// The shell `id` of the workspace that holds `session`'s folder, or a 404.
pub(super) fn owned(app: &Server, session: &str, id: &str) -> Result<String, ApiError> {
    let cwd = session_cwd(app, session)?;
    let shells = list(app, &cwd).map_err(internal)?;
    shells
        .into_iter()
        .find(|shell| valid_id(id) && shell.id == id)
        .map(|shell| shell.id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown shell"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ShellRequest {
    session: String,
    id: Option<String>,
}

async fn blocking<T: Send + 'static>(
    run: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(run)
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "shell action failed"))?
}

pub(super) async fn shells_list(
    State(app): State<App>,
    Query(query): Query<BackgroundQuery>,
) -> Result<Json<Value>, ApiError> {
    let shells = blocking(move || {
        let cwd = session_cwd(&app, &query.session)?;
        list(&app, &cwd).map_err(internal)
    })
    .await?;
    Ok(Json(json!({ "shells": shells })))
}

pub(super) async fn shells_create(
    State(app): State<App>,
    Json(request): Json<ShellRequest>,
) -> Result<Json<Value>, ApiError> {
    let id = blocking(move || {
        let cwd = session_cwd(&app, &request.session)?;
        create_with_room(
            || refuse_read_only_workspace(&app, &cwd, "shell"),
            || list(&app, &cwd),
            || create(&cwd),
        )
    })
    .await?;
    Ok(Json(json!({ "id": id })))
}

pub(super) async fn shells_close(
    State(app): State<App>,
    Json(request): Json<ShellRequest>,
) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let id = owned(
            &app,
            &request.session,
            request.id.as_deref().unwrap_or_default(),
        )?;
        kill(&id).map_err(internal)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
#[path = "shells/tests.rs"]
mod tests;
