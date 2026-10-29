//! Server live turn: the text a running turn shows on its pane, sent as a `live` frame
//! (ADR 0131).

use super::*;

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
    let log_agent = agent.clone();
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
    // The first read is always sent, so a page that held an earlier watcher's text gets this
    // one's, or null.
    let mut last: Option<Option<String>> = None;
    // pi's text starts under the turn's message. A send can show as working before pi logs the
    // message, so only a message other than the one before this turn is this turn's.
    // ponytail: the same message sent twice in a row shows no live text on its second turn.
    let mut prompt: Option<String> = None;
    let mut before: Option<Option<String>> = None;
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
        if provider == "pi" {
            match (&pane, &prompt) {
                (None, Some(_)) => before = Some(prompt.take()),
                (None, None) if before.is_none() => {
                    before = Some(sent_prompt(&server, &session, &log_agent).await);
                }
                (Some(_), None) => {
                    let sent = sent_prompt(&server, &session, &log_agent).await;
                    if before.as_ref() != Some(&sent) {
                        prompt = sent;
                    }
                }
                _ => {}
            }
        }
        drop(server);
        let text = match pane {
            Some(pane) => {
                let (adapter, provider) = (adapter.clone(), provider.clone());
                let sent = prompt.clone();
                let read = tokio::task::spawn_blocking(move || {
                    let rows = adapter.capture_within(&[("pane", &pane)], CAPTURE)?;
                    Some(swarm::screen::turn_text(&provider, &rows, sent.as_deref()))
                })
                .await
                .ok()
                .flatten();
                // A failed or slow read keeps the shown text, so the row does not blink.
                let Some(text) = read else {
                    continue;
                };
                text
            }
            None => None,
        };
        if last.as_ref() == Some(&text) {
            continue;
        }
        last = Some(text);
        let Some(server) = app.upgrade() else {
            return;
        };
        let frame = Frame::new(
            "live",
            json!({"key": key, "text": last.clone().flatten()}),
            Some(&key),
        );
        // ponytail: a client that joins a running follower sees the text from its next change;
        // the page clears its live text on reconnect, so it never keeps a stale one.
        let mut hub = server.hub.lock().unwrap();
        // A newer watcher of this key may have started during the read; its frames win.
        if hub
            .followers
            .get(&key)
            .is_none_or(|follower| follower.generation != generation)
        {
            return;
        }
        for client in hub.streams.values_mut() {
            if client.row_keys.contains(&key) {
                client.push(frame.clone());
            }
        }
    }
}

/// The last message the owner sent to `agent`, from its log.
async fn sent_prompt(server: &App, session: &str, agent: &str) -> Option<String> {
    let (root, session, agent) = (server.root.clone(), session.to_string(), agent.to_string());
    let log = tokio::task::spawn_blocking(move || resolve_log(&root, &session, &agent))
        .await
        .ok()?
        .ok()?;
    last_prompt(server, &log)
        .await
        .ok()
        .filter(|prompt| !prompt.is_empty())
}
