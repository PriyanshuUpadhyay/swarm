//! Server live turn: the text a running turn shows on its pane, sent as a `live` frame
//! (ADR 0131).

use super::*;

/// How often a working agent's pane is read. One capture is one adapter process, or one socket
/// read on a PTY host.
const TICK: Duration = Duration::from_millis(250);

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
    let lookup = (root.clone(), session.clone(), agent.clone());
    let found = tokio::task::spawn_blocking(move || {
        let (root, session, agent) = lookup;
        let connection = read_connection(&root).ok()?;
        let adapter = swarm::store::adapter_of(&connection, &session).ok()??;
        let provider = swarm::store::provider_of(&connection, &session, &agent).ok()??;
        Some((swarm::adapter::load(&root, &adapter).ok()?, provider))
    })
    .await;
    let Ok(Some((adapter, provider))) = found else {
        return;
    };
    let adapter = Arc::new(adapter);
    let mut last: Option<String> = None;
    let mut timer = tokio::time::interval(TICK);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = stop.changed() => return,
            _ = timer.tick() => {}
        }
        let Some(server) = app.upgrade() else {
            return;
        };
        let pane = {
            let hub = server.hub.lock().unwrap();
            if hub
                .followers
                .get(&key)
                .is_none_or(|follower| follower.generation != generation)
            {
                return;
            }
            live_agents(&hub.agents[&session])
                .find(|row| row["id"] == agent.as_str() && row["state"] == "working")
                .and_then(|row| row["pane"].as_str().map(str::to_string))
        };
        drop(server);
        let text = match pane {
            Some(pane) => {
                let (adapter, provider) = (adapter.clone(), provider.clone());
                tokio::task::spawn_blocking(move || {
                    let rows = adapter.capture_within(&[("pane", &pane)], TICK)?;
                    swarm::screen::turn_text(&provider, &rows)
                })
                .await
                .ok()
                .flatten()
            }
            None => None,
        };
        if text == last {
            continue;
        }
        last = text;
        let Some(server) = app.upgrade() else {
            return;
        };
        let frame = Frame::new("live", json!({"key": key, "text": last}), Some(&key));
        // ponytail: a client that subscribes mid-turn sees the text from its next change.
        let mut hub = server.hub.lock().unwrap();
        for client in hub.streams.values_mut() {
            if client.row_keys.contains(&key) {
                client.push(frame.clone());
            }
        }
    }
}
