//! Server browser.

use super::*;

/// The token in `<root>/ui-token`, made once with mode 0600, so the page's saved token still works
/// after `swarm ui` starts a new server.
pub(super) fn ui_token(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = root.join("ui-token");
    if let Ok(token) = std::fs::read_to_string(&path) {
        let token = token.trim();
        if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(token.to_string());
        }
    }
    let token = new_token()?;
    let _ = std::fs::remove_file(&path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?
        .write_all(format!("{token}\n").as_bytes())?;
    Ok(token)
}

pub(super) fn new_token() -> Result<String, Box<dyn std::error::Error>> {
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// True when the server at `http://127.0.0.1:<port>/#token=<token>` takes that token.
pub(super) fn answers(url: &str) -> bool {
    call(url, "GET", "/api/badge").is_some()
}

/// Asks the server at `url` to reload its open pages, and returns how many it reached.
pub(super) fn reload_pages(url: &str) -> u64 {
    pages_reached(call(url, "POST", "/api/reload"))
}

/// Asks the server at `url` to open `session` in its open pages, and returns how many it reached.
pub(super) fn show_chat(url: &str, session: &str) -> u64 {
    let body = json!({"session": session}).to_string();
    pages_reached(call_with(url, "POST", "/api/show", &body))
}

fn pages_reached(answer: Option<String>) -> u64 {
    answer
        .and_then(|body| serde_json::from_str::<Value>(&body).ok())
        .and_then(|body| body["pages"].as_u64())
        .unwrap_or(0)
}

/// The body of a 200 answer from the server at `url` to `method path` with its token.
pub(super) fn call(url: &str, method: &str, path: &str) -> Option<String> {
    call_with(url, method, path, "{}")
}

/// `call` with a JSON `body`.
pub(super) fn call_with(url: &str, method: &str, path: &str, body: &str) -> Option<String> {
    let (port, token) = url
        .strip_prefix("http://127.0.0.1:")?
        .split_once("/#token=")?;
    let port = port.parse::<u16>().ok()?;
    let address = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port));
    let mut stream =
        std::net::TcpStream::connect_timeout(&address, Duration::from_millis(500)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok()?;
    let (head, body) = answer.split_once("\r\n\r\n")?;
    head.starts_with("HTTP/1.1 200").then(|| body.to_string())
}

/// `SWARM_APP` alone when set, else the Swarm.app that ships beside this swarm.
pub(super) fn shell_app() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SWARM_APP") {
        return Some(PathBuf::from(path)).filter(|path| path.exists());
    }
    swarm::paths::bundled("Swarm.app")
}

/// Opens `url` in Swarm.app, which opens its window or, when it runs, comes to the front.
/// Without the app, it opens an app window of the default browser when it is Chromium based, else
/// a tab. An app window has no tab bar, and the browser passes ⌘N, ⌘T and ⌘W to the page.
pub(super) fn open_page(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let mut open = std::process::Command::new("open");
    if let Some(app) = shell_app() {
        open.arg("-a").arg(app).args(["--args", url]);
        return opened(open);
    }
    match default_browser().filter(|id| CHROMIUM.contains(&id.as_str())) {
        // `-n` starts a second process, which hands `--app` to the running browser and exits.
        Some(id) => open
            .args(["-n", "-b", &id, "--args"])
            .arg(format!("--app={url}")),
        None => open.arg(url),
    };
    opened(open)
}

/// Runs `open`; a failure is an error, so a notice click does not report a window it never opened.
fn opened(mut open: std::process::Command) -> Result<(), Box<dyn std::error::Error>> {
    let status = open.stdin(std::process::Stdio::null()).status()?;
    if !status.success() {
        return Err(format!("swarm: open could not show the page ({status})").into());
    }
    Ok(())
}

/// Bundle ids of browsers that take `--app=URL`, in the lower case that Launch Services stores.
pub(super) const CHROMIUM: &[&str] = &[
    "com.google.chrome",
    "com.google.chrome.beta",
    "com.google.chrome.canary",
    "com.brave.browser",
    "com.microsoft.edgemac",
    "com.vivaldi.vivaldi",
    "org.chromium.chromium",
];

/// The bundle id of the `http` handler that the user chose, or None when the system default holds.
pub(super) fn default_browser() -> Option<String> {
    let plist = std::env::home_dir()?
        .join("Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist");
    let output = std::process::Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(plist)
        .output()
        .ok()?;
    let prefs: Value = serde_json::from_slice(&output.stdout).ok()?;
    prefs["LSHandlers"]
        .as_array()?
        .iter()
        .find(|handler| handler["LSHandlerURLScheme"] == "http")?["LSHandlerRoleAll"]
        .as_str()
        .map(str::to_lowercase)
}

pub(super) type Options = (Option<u16>, Option<PathBuf>, Option<String>);
