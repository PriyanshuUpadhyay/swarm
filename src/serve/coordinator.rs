//! Server coordinator.

use super::*;

pub(super) fn start_coordinator(app: &App, action_queue: mpsc::Receiver<ActionJob>) {
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
        let mut last_listings = BTreeMap::new();
        loop {
            let Some(app) = weak.upgrade() else {
                break;
            };
            if app.stopped.load(Ordering::Relaxed) {
                break;
            }
            let started = std::time::Instant::now();
            match super::all_agent_listings(&mut connection, &app.root, Duration::from_millis(900))
            {
                Ok(listings) => {
                    let value = agent_snapshot(&connection, &listings, &mut last_listings);
                    let mut hub = app.hub.lock().unwrap();
                    if hub.agents != value {
                        hub.agents = value;
                        hub.bump();
                        // The sidebar reads agent states from the tree's own copy, so a changed
                        // state rebuilds the tree in this pass, not at the next 2 s refresh.
                        last_tree = None;
                    }
                }
                Err(err) => eprintln!("swarm serve: coordinator: {err}"),
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

fn agent_snapshot(
    connection: &rusqlite::Connection,
    listings: &BTreeMap<String, crate::cli::AgentListOutput>,
    last_listings: &mut BTreeMap<String, Value>,
) -> Value {
    let mut agents = BTreeMap::new();
    // A failed listing keeps the last pane state. Stored-only rows must stay fresh.
    if let Ok(sessions) = swarm::store::sessions(connection) {
        for session in sessions {
            let rows = if let Some(listing) = listings.get(&session.id) {
                serde_json::to_value(&listing.agents)
                    .inspect(|rows| {
                        last_listings.insert(session.id.clone(), rows.clone());
                    })
                    .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)
            } else if let Some(rows) = last_listings.get(&session.id) {
                Ok(rows.clone())
            } else {
                stored_agents(connection, &session.id)
            };
            if let Ok(rows) = rows {
                agents.insert(session.id, rows);
            }
        }
    }
    json!(agents)
}

pub(super) fn stored_agents(
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

#[cfg(test)]
#[path = "coordinator/tests.rs"]
mod tests;
