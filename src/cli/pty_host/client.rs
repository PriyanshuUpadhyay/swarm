//! `swarm pty <verb>`, the client half that the tmux-swarm adapter runs for a `pty:` pane.

use super::{REQUEST_TIMEOUT, socket_dir, socket_of};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;

/// One request to the host behind `socket`, and its `out`.
fn request(
    socket: &std::path::Path,
    request: serde_json::Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let stream = UnixStream::connect(socket)
        .map_err(|error| format!("swarm pty: no host at {}: {error}", socket.display()))?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    writeln!(&stream, "{request}")?;
    let mut line = String::new();
    std::io::BufReader::new(&stream).read_line(&mut line)?;
    let reply: serde_json::Value = serde_json::from_str(&line)?;
    match reply["ok"].as_bool() {
        Some(true) => Ok(reply["out"].as_str().unwrap_or_default().to_string()),
        _ => Err(reply["error"]
            .as_str()
            .unwrap_or("swarm pty: bad reply")
            .into()),
    }
}

/// The session and child number of a `pty:<session>:<n>` pane.
fn pane_of(pane: &str) -> Result<(&str, u64), Box<dyn std::error::Error>> {
    let bad = || format!("swarm pty: bad pane {pane:?}");
    let (session, n) = pane
        .strip_prefix("pty:")
        .and_then(|rest| rest.rsplit_once(':'))
        .ok_or_else(bad)?;
    crate::cli::valid_session_id(session)?;
    Ok((session, n.parse().map_err(|_| bad())?))
}

/// `swarm pty <verb>`: one tmux-swarm verb for a `pty:` pane. It reads the `SWARM_*` vars that
/// the adapter runtime exports.
pub(crate) fn client(verb: &str) -> Result<String, Box<dyn std::error::Error>> {
    let var = |name: &str| std::env::var(format!("SWARM_{name}")).unwrap_or_default();
    if verb == "spawn" {
        let mut env: HashMap<String, String> = ["SESSION_ID", "AGENT_ID", "HOME", "ADAPTER"]
            .iter()
            .map(|name| (format!("SWARM_{name}"), var(name)))
            .collect();
        // A hosted chair spawns its children on the same host (ADR 0131).
        if var("AGENT_ID") == "orchestrator" {
            env.insert("SWARM_PTY_HOST".into(), var("PTY_HOST"));
        }
        let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
        let feed = var("FEED");
        return request(
            std::path::Path::new(&var("PTY_HOST")),
            serde_json::json!({"op": "spawn", "cwd": cwd, "env": env, "feed": feed}),
        );
    }
    if verb == "list" {
        // Every live host of this user, as tmux lists every pane on its server.
        let mut out = String::new();
        for entry in std::fs::read_dir(socket_dir()?)?.flatten() {
            if let Ok(text) = request(&entry.path(), serde_json::json!({"op": "list"})) {
                out.push_str(&text);
            }
        }
        return Ok(out);
    }
    let pane = var("PANE");
    let (session, n) = pane_of(&pane)?;
    let socket = socket_of(session)?;
    let send = |op: &str, field: &str, value: String| {
        request(&socket, serde_json::json!({"op": op, "n": n, field: value}))
    };
    let ask = |op: &str| request(&socket, serde_json::json!({"op": op, "n": n}));
    match verb {
        "ring" => {
            send("write", "text", var("TEXT"))?;
            // As tmux-swarm's ring: a CLI that reads a fast Enter as part of a paste keeps it.
            std::thread::sleep(std::time::Duration::from_millis(500));
            send("key", "key", "Enter".into())
        }
        "interrupt" => send("key", "key", "Escape".into()),
        "key" => send("key", "key", var("KEY")),
        "screen" | "capture" | "close" => ask(verb),
        "attach" => attach(&socket, n).map(|()| String::new()),
        _ => Err(format!("swarm pty: unknown verb {verb}").into()),
    }
}

/// Set by SIGWINCH, so the viewer sends its new size.
static RESIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn note_resize(_: libc::c_int) {
    RESIZED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// This terminal's rows and columns, as `{"rows", "cols"}`, or none on a non-terminal.
fn terminal_size() -> serde_json::Value {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ writes a local winsize.
    if unsafe { libc::ioctl(0, libc::TIOCGWINSZ, &mut size) } != 0
        || size.ws_row == 0
        || size.ws_col == 0
    {
        return serde_json::json!({});
    }
    serde_json::json!({"rows": size.ws_row, "cols": size.ws_col})
}

/// `swarm pty attach`: shows a hosted pane on this terminal and sends it the keys typed here,
/// until the pane ends or this terminal goes (ADR 0131).
fn attach(socket: &std::path::Path, n: u64) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let stream = UnixStream::connect(socket)
        .map_err(|error| format!("swarm pty: no host at {}: {error}", socket.display()))?;
    let mut ask = terminal_size();
    ask["op"] = "attach".into();
    ask["n"] = n.into();
    // Only the reply is bounded; the viewer then waits on the pane for as long as it shows.
    stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    writeln!(&stream, "{ask}")?;
    let mut reader = std::io::BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    stream.set_read_timeout(None)?;
    stream.set_write_timeout(None)?;
    let mut keys = stream.try_clone()?;
    let reply: serde_json::Value = serde_json::from_str(&line)?;
    if reply["ok"] != true {
        return Err(reply["error"]
            .as_str()
            .unwrap_or("swarm pty: bad reply")
            .into());
    }
    // SAFETY: the handler only stores to an atomic.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = note_resize as extern "C" fn(libc::c_int) as usize;
        action.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGWINCH, &action, std::ptr::null_mut());
    }
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: tcgetattr fills a local termios; cfmakeraw and tcsetattr read it.
    let raw_mode = unsafe { libc::tcgetattr(0, &mut saved) } == 0 && {
        let mut attrs = saved;
        unsafe { libc::cfmakeraw(&mut attrs) };
        (unsafe { libc::tcsetattr(0, libc::TCSANOW, &attrs) }) == 0
    };
    let socket = socket.to_path_buf();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if RESIZED.swap(false, std::sync::atomic::Ordering::Relaxed) {
                let mut size = terminal_size();
                size["op"] = "resize".into();
                size["n"] = n.into();
                let _ = request(&socket, size);
            }
        }
    });
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut keys);
        let _ = keys.shutdown(std::net::Shutdown::Write);
    });
    let mut stdout = std::io::stdout().lock();
    let mut chunk = [0; 65536];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                if stdout
                    .write_all(&chunk[..read])
                    .and_then(|()| stdout.flush())
                    .is_err()
                {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    if raw_mode {
        // SAFETY: restores the termios read above.
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_pane_id() {
        let session = uuid::Uuid::now_v7().to_string();
        let pane = format!("pty:{session}:12");
        assert_eq!(pane_of(&pane).unwrap(), (session.as_str(), 12));
        assert!(pane_of("pty:../x:1").is_err());
        assert!(pane_of("%3").is_err());
    }
}
