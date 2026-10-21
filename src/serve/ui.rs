//! Server ui.

use super::*;

/// The port `swarm ui` takes when no `--port` names one. A fixed port keeps the page's origin, so
/// the token that the page saves in local storage is there after a restart (ADR 0088).
pub(super) const UI_PORT: u16 = 4747;

/// `swarm ui`: reuse the `swarm ui` server of this home when its recorded URL still answers with its
/// token, else start one in the background and wait until it answers. Then open the page and start
/// the usage HUD when it is not running. `--foreground` serves in this process until Ctrl-C, and
/// `stop` stops the recorded server.
pub fn ui(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    open_ui(args, None)
}

/// ADR 0102: a notice click on a chat of the app's own adapter opens Swarm.app on that chat. An
/// open page gets a `show` frame; with none, the app starts on the chat's path.
pub fn open_chat(session: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !valid_session(session) {
        return Err(format!("swarm: bad session id {session}").into());
    }
    open_ui(&[], Some(session))
}

/// A session id that may go into a URL path and a `show` frame as it is.
pub(super) fn valid_session(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// The page URL that opens `session`: its path before the `#token=` fragment.
pub(super) fn chat_url(url: &str, session: &str) -> String {
    match url.split_once("/#") {
        Some((base, fragment)) => format!("{base}/chat/{session}#{fragment}"),
        None => url.to_string(),
    }
}

fn open_ui(args: &[String], chat: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let record = root.join("ui-url");
    if args.first().map(String::as_str) == Some("stop") {
        if args.len() > 1 {
            return Err("usage: swarm ui stop".into());
        }
        return stop_ui(&record);
    }
    let foreground = args.first().map(String::as_str) == Some("--foreground");
    let rest = if foreground { &args[1..] } else { args };
    let (port, assets, dev_origin) = options(rest)?;
    if assets.is_none() && WEB_ASSETS.is_empty() {
        return Err("swarm ui: this build has no web UI; run `pnpm --dir web build` and rebuild, or pass --assets DIR".into());
    }
    let running = recorded(&record).filter(|(url, ..)| answers(url));
    if foreground {
        if let Some((url, ..)) = running {
            return Err(format!(
                "swarm ui: a server of this home already runs at {}; run `swarm ui stop` first",
                page_base(&url)
            )
            .into());
        }
        return start(port.unwrap_or(UI_PORT), assets, dev_origin, Some(record));
    }
    let (url, spawned) = match running {
        Some((url, _, build)) if build.as_deref() == Some(BUILD) => (url, false),
        // A server of another build, such as one whose files an upgrade removed, gives way.
        Some(_) => {
            stop_ui(&record)?;
            (spawn_ui(&root, &record, rest)?, true)
        }
        None => (spawn_ui(&root, &record, rest)?, true),
    };
    // The URL holds the token, so a notice click, which any caller can run, does not print it.
    if chat.is_none() {
        println!("{url}");
    }
    // An open page reloads in place. After a new server starts, its pages need up to a second to
    // reconnect, so wait for them before a new window opens beside them. Swarm.app is opened even
    // then, because that `open` brings its window to the front.
    let deadline =
        std::time::Instant::now() + Duration::from_millis(if spawned { 1500 } else { 0 });
    // ponytail: a page that reloads for a new build in this second can miss the `show` frame and
    // keep its own chat; the app still comes to the front.
    let page = chat.map_or_else(|| url.clone(), |session| chat_url(&url, session));
    loop {
        let reached = match chat {
            Some(session) => show_chat(&url, session) > 0,
            None => reload_pages(&url) > 0,
        };
        if reached || std::time::Instant::now() >= deadline {
            if !reached || shell_app().is_some() {
                open_page(&page)?;
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    start_hud();
    Ok(())
}

/// The version and commit of this binary, which `swarm ui` records so it can tell its own server.
pub(super) const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), " ", env!("SWARM_BUILD_COMMIT"));

/// The URL, server pid, and server build in `record`: the first line, the second, and the third.
pub(super) fn recorded(record: &Path) -> Option<(String, Option<i32>, Option<String>)> {
    let text = std::fs::read_to_string(record).ok()?;
    let mut lines = text.lines();
    let url = lines.next()?.trim().to_string();
    let pid = lines.next().and_then(|line| line.trim().parse().ok());
    let build = lines.next().map(|line| line.trim().to_string());
    Some((url, pid, build))
}

/// The doctor's row for the `swarm ui` server of this home: whether it runs, and with this build.
pub fn ui_state() -> (&'static str, String) {
    let Ok(root) = swarm::paths::root_dir() else {
        return ("broken", "No swarm home".into());
    };
    match recorded(&root.join("ui-url")).filter(|(url, ..)| answers(url)) {
        None => (
            "ok",
            "No `swarm ui` server is recorded; `swarm ui` starts one".into(),
        ),
        Some((_, _, build)) if build.as_deref() == Some(BUILD) => ("ok", format!("Runs {BUILD}")),
        Some((_, _, build)) => (
            "broken",
            format!(
                "Runs {}, not {BUILD}; `swarm ui` replaces it",
                build.as_deref().unwrap_or("an older build")
            ),
        ),
    }
}

/// The URL without its `#token=` fragment, for a message that may be shown or logged.
pub(super) fn page_base(url: &str) -> &str {
    url.split_once('#').map_or(url, |(base, _)| base)
}

/// Start `swarm ui --foreground` with `args` as a background process whose output goes to
/// `<root>/ui.log`, and wait until the URL that it records answers.
pub(super) fn spawn_ui(
    root: &Path,
    record: &Path,
    args: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::os::unix::process::CommandExt;
    let log_path = root.join("ui.log");
    // The server prints its URL with the token, so only the owner may read the log.
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&log_path)?;
    log.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    let _ = std::fs::remove_file(record);
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("ui")
        .arg("--foreground")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        // Its own process group, so a Ctrl-C in this terminal does not stop it.
        .process_group(0)
        .spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            let last = log.lines().last().unwrap_or("no output");
            return Err(format!("{last} (log: {})", log_path.display()).into());
        }
        if let Some((url, ..)) = recorded(record).filter(|(url, ..)| answers(url)) {
            return Ok(url);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "swarm ui: the server did not answer within 15 s; see {}",
        log_path.display()
    )
    .into())
}

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

/// `swarm ui stop`: stop the recorded server only while it still answers with its token, so a pid
/// that the system gave to another process since is never signalled.
pub(super) fn stop_ui(record: &Path) -> Result<(), Box<dyn std::error::Error>> {
    const SIGTERM: i32 = 15;
    let Some((url, Some(pid), _)) = recorded(record).filter(|(url, ..)| answers(url)) else {
        let _ = std::fs::remove_file(record);
        println!("swarm ui: no server of this home is running");
        return Ok(());
    };
    // SAFETY: kill has no memory preconditions; a gone pid only returns an error.
    if unsafe { kill(pid, SIGTERM) } != 0 {
        return Err(format!(
            "swarm ui: could not stop pid {pid}: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while answers(&url) {
        if std::time::Instant::now() >= deadline {
            return Err(format!("swarm ui: pid {pid} still answers after 5 s").into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(record);
    println!("swarm ui: stopped the server at {}", page_base(&url));
    Ok(())
}

/// Start the usage HUD when it is not running: its login item when one is installed, else the
/// app beside this swarm. Without a HUD app it says so and goes on, because the page still works.
pub(super) fn start_hud() {
    let running = std::process::Command::new("/usr/bin/pgrep")
        .args(["-x", "SwarmHUD"])
        .stdout(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if running {
        return;
    }
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    if swarm::hud::plist_path(Path::new(&home)).exists() {
        let _ = swarm::hud::kickstart();
        return;
    }
    let Some(app) = swarm::hud::app() else {
        eprintln!("swarm ui: no SwarmHUD.app beside this swarm, so the usage HUD stays off");
        return;
    };
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new(app.join("Contents/MacOS/SwarmHUD"));
    if let Ok(swarm) = std::env::current_exe() {
        command.env("SWARM_BIN", swarm);
    }
    if let Err(error) = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
    {
        eprintln!("swarm ui: the usage HUD did not start: {error}");
    }
}

#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;
