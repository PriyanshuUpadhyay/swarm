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
        let env: HashMap<String, String> = ["SESSION_ID", "AGENT_ID", "HOME", "ADAPTER"]
            .iter()
            .map(|name| (format!("SWARM_{name}"), var(name)))
            .collect();
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
        "attach" => Err("swarm pty: a hosted pane has no attach yet".into()),
        _ => Err(format!("swarm pty: unknown verb {verb}").into()),
    }
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
