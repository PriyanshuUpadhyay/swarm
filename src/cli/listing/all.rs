//! List the agents of every session at once.

use super::*;

pub(crate) fn all_agent_listings(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    budget: std::time::Duration,
) -> Result<std::collections::BTreeMap<String, AgentListOutput>, Box<dyn std::error::Error>> {
    all_agent_listings_kept(connection, root, budget, &mut ListingCache::default())
}

/// `all_agent_listings` for a caller that lists again and again, so an idle pane keeps its last
/// screen read in `cache`.
pub(crate) fn all_agent_listings_kept(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    budget: std::time::Duration,
    cache: &mut ListingCache,
) -> Result<std::collections::BTreeMap<String, AgentListOutput>, Box<dyn std::error::Error>> {
    let mut listings = std::collections::BTreeMap::new();
    let deadline = std::time::Instant::now() + budget;
    let sessions: Vec<_> = swarm::store::sessions(connection)?
        .into_iter()
        .filter(|session| {
            session.agents > 0
                && session
                    .adapter
                    .as_deref()
                    .is_some_and(|name| !name.trim().is_empty())
        })
        .collect();
    fn adapter_of(session: &swarm::store::SessionRow) -> &str {
        session.adapter.as_deref().expect("filtered adapter").trim()
    }
    // Every shipped adapter's `list` names all panes, not one session's, so it runs once per
    // adapter, with the env of the first session that uses it. A load error or expired deadline
    // leaves those sessions out.
    let mut pane_lists: Vec<(&str, Result<Option<String>, String>)> = Vec::new();
    for session in &sessions {
        if !pane_lists
            .iter()
            .any(|(name, _)| *name == adapter_of(session))
        {
            pane_lists.push((adapter_of(session), Ok(None)));
        }
    }
    let adapters = pane_lists.len();
    for (index, (name, panes)) in pane_lists.iter_mut().enumerate() {
        let first = sessions
            .iter()
            .find(|s| adapter_of(s) == *name)
            .expect("listed adapter");
        // A list that failed recently counts as failed again, as if it ran.
        if cache
            .failed_lists
            .get(*name)
            .is_some_and(|at| at.elapsed() < LIST_RETRY)
        {
            continue;
        }
        *panes = swarm::adapter::load(root, name)
            .and_then(|mut adapter| {
                adapter.session_id = Some(first.id.clone());
                let now = std::time::Instant::now();
                adapter.deadline =
                    Some(now + deadline.saturating_duration_since(now) / (adapters - index) as u32);
                let result = adapter.run("list", &[]);
                pane_list_result(&adapter, result)
            })
            .map_err(|error| error.to_string());
        if matches!(panes, Ok(None)) {
            cache
                .failed_lists
                .insert(name.to_string(), std::time::Instant::now());
        } else {
            cache.failed_lists.remove(*name);
        }
    }
    let panes_of = |session: &swarm::store::SessionRow| {
        &pane_lists
            .iter()
            .find(|(name, _)| *name == adapter_of(session))
            .expect("listed adapter")
            .1
    };
    // Only a session with a live pane reads its screen, so only those share the time left.
    // A failed pane list reads no screen.
    let mut live = Vec::with_capacity(sessions.len());
    for session in &sessions {
        let rows = swarm::store::agents(connection, &session.id)?;
        live.push(match panes_of(session) {
            Ok(Some(panes)) => rows.iter().any(|row| {
                row.pane
                    .as_deref()
                    .is_some_and(|pane| swarm::adapter::listing_has_pane(panes, pane))
            }),
            Ok(None) | Err(_) => false,
        });
    }
    for (index, session) in sessions.iter().enumerate() {
        let sharers = if live[index] {
            live[index..].iter().filter(|live| **live).count()
        } else {
            sessions.len() - index
        };
        let listing = panes_of(session)
            .clone()
            .map_err(Into::into)
            .and_then(|panes| {
                let mut adapter = swarm::adapter::load(root, adapter_of(session))?;
                adapter.session_id = Some(session.id.clone());
                let now = std::time::Instant::now();
                adapter.deadline =
                    Some(now + deadline.saturating_duration_since(now) / sharers as u32);
                list_agents_from(connection, root, &session.id, &adapter, panes, cache)
            });
        match listing {
            Ok(listing) => {
                listings.insert(session.id.clone(), listing);
            }
            Err(error) => eprintln!("swarm: {}: {error}", session.id),
        }
    }
    cache.screens.retain(|(adapter, pane, _), _| {
        pane_lists.iter().any(|(name, panes)| {
            name == adapter
                && matches!(panes, Ok(Some(list)) if swarm::adapter::listing_has_pane(list, pane))
        })
    });
    Ok(listings)
}
