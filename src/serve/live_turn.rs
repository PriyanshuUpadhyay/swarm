//! Server live turn: the text a running turn shows on its pane, sent as a `live` frame
//! (ADR 0131).

use super::*;
use swarm::view::transcript::{
    live_shown::{LiveRaw as Live, LiveShown},
    markdown::display,
};

/// How often a working agent's pane is read. One capture is one adapter process, or one socket
/// read on a PTY host.
const TICK: Duration = Duration::from_millis(250);
/// How long one screen read may take. A slow read delays the next tick, so the row updates
/// less often but still updates.
const CAPTURE: Duration = Duration::from_secs(2);

/// Sends `{key, text}` to each client that shows `key`'s rows whenever the turn text on the
/// agent's screen changes, and `text: null` once the agent stops working. Ends with its follower.
pub(super) async fn watch_turn(
    app: Weak<Server>,
    key: String,
    generation: String,
    mut stop: watch::Receiver<bool>,
) {
    let Some((session, agent)) = key.split_once(':') else {
        return;
    };
    let (session, agent) = (session.to_string(), agent.to_string());
    let Some(server) = app.upgrade() else {
        return;
    };
    let root = server.root.clone();
    drop(server);
    let lookup = (root.clone(), session.clone(), agent);
    let found = tokio::task::spawn_blocking(move || {
        let (root, session, agent) = lookup;
        let connection = read_connection(&root).ok()?;
        // The page names the chair's chat `chair`, as `resolve_log` reads it.
        let agent = match agent.as_str() {
            "chair" => swarm::store::orchestrator_of(&connection, &session).ok()?,
            _ => agent,
        };
        let adapter = swarm::store::adapter_of(&connection, &session).ok()??;
        let provider = swarm::store::provider_of(&connection, &session, &agent).ok()??;
        Some((swarm::adapter::load(&root, &adapter).ok()?, provider, agent))
    })
    .await;
    let Ok(Some((adapter, provider, agent))) = found else {
        return;
    };
    let adapter = Arc::new(adapter);
    // pi's extension writes the streaming message to this file, so its live row needs no screen.
    let pi_file = (provider == "pi").then(|| swarm::bus::pi_live_file(&root, &session, &agent));
    let Some(server) = app.upgrade() else {
        return;
    };
    let (mut rows_changed, mut rows_version) = {
        let hub = server.hub.lock().unwrap();
        let Some(follower) = hub
            .followers
            .get(&key)
            .filter(|f| f.generation == generation)
        else {
            return;
        };
        (follower.rows_changed.subscribe(), follower.rows_version)
    };
    drop(server);
    let mut shown = LiveShown::default();
    // The first read is always sent, so a page that held an earlier watcher's text gets this
    // one's, or null.
    let mut last_read: Option<Live> = None;
    let mut timer = tokio::time::interval(TICK);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let tick = tokio::select! {
            _ = stop.changed() => return,
            result = rows_changed.changed() => {
                if result.is_err() { return; }
                false
            },
            _ = timer.tick() => true,
        };
        let read = if tick {
            let Some(server) = app.upgrade() else {
                return;
            };
            let pane = {
                let hub = server.hub.lock().unwrap();
                if hub
                    .followers
                    .get(&key)
                    .is_none_or(|f| f.generation != generation)
                {
                    return;
                }
                live_agents(&hub.agents[&session])
                    .find(|row| row["id"] == agent.as_str() && row["state"] == "working")
                    .and_then(|row| row["pane"].as_str().map(str::to_string))
            };
            drop(server);
            let live = match pane {
                // A failed or slow read keeps the shown text, so the row does not blink.
                Some(pane) => {
                    let (adapter, provider, file) =
                        (adapter.clone(), provider.clone(), pi_file.clone());
                    let read = tokio::task::spawn_blocking(move || match file {
                        Some(file) => {
                            Some(pi_live(&std::fs::read_to_string(file).unwrap_or_default()))
                        }
                        None => {
                            let rows = adapter.capture_within(&[("pane", &pane)], CAPTURE)?;
                            Some(Live {
                                text: swarm::screen::turn_text(&provider, &rows),
                                thinking: None,
                            })
                        }
                    })
                    .await
                    .ok()
                    .flatten();
                    let Some(live) = read else {
                        continue;
                    };
                    live
                }
                None => Live::default(),
            };
            (last_read.as_ref() != Some(&live)).then_some(live)
        } else {
            None
        };
        // ponytail: a client that joins a running follower sees the text from its next change;
        // the page clears its live text on reconnect, so it never keeps a stale one.
        if tick && read.is_none() {
            continue;
        }
        // A newer watcher of this key may have started during the read; its frames win.
        loop {
            let Some(server) = app.upgrade() else {
                return;
            };
            let (version, rows) = {
                let hub = server.hub.lock().unwrap();
                let Some(follower) = hub
                    .followers
                    .get(&key)
                    .filter(|f| f.generation == generation)
                else {
                    return;
                };
                let rows = if follower.rendering_rows {
                    None
                } else {
                    follower.rows.as_ref().and_then(|rows| rows.tail(50))
                };
                (follower.rows_version, rows)
            };
            let Some(rows) = rows else {
                break;
            };
            if read.is_none() && rows_version == version {
                break;
            }
            let mut next = shown.clone();
            let changed = version != rows_version;
            let read_step = read.clone();
            let result = tokio::task::spawn_blocking(move || {
                let frame = next.step(&rows, read_step, changed).map(|shown| {
                    let display = shown
                        .raw
                        .text
                        .as_deref()
                        .filter(|text| !text.is_empty())
                        .and_then(|text| display::display(text).ok());
                    (shown, display)
                });
                (next, frame)
            })
            .await;
            let Ok((next, frame)) = result else {
                break;
            };
            let frame =
                frame.map(|(shown, display)| super::live_delta::live_frame(&key, shown, display));
            let mut hub = server.hub.lock().unwrap();
            let Some(follower) = hub
                .followers
                .get(&key)
                .filter(|f| f.generation == generation)
            else {
                return;
            };
            if follower.rendering_rows {
                break;
            }
            if follower.rows_version != version {
                continue;
            }
            shown = next;
            rows_version = version;
            if let Some(read) = &read {
                last_read = Some(read.clone());
            }
            if let Some(frame) = frame {
                for client in hub.streams.values_mut() {
                    if client.row_keys.contains(&key) {
                        client.push(frame.clone());
                    }
                }
            }
            break;
        }
    }
}

/// The `{thinking, text}` that pi's extension writes. A missing or empty file is no text.
fn pi_live(file: &str) -> Live {
    let value: serde_json::Value = serde_json::from_str(file).unwrap_or_default();
    let field = |name: &str| {
        value[name]
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string)
    };
    Live {
        text: field("text"),
        thinking: field("thinking"),
    }
}
