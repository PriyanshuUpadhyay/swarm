//! CLI listing.

use super::*;

pub(crate) fn set_chair_for_caller(
    connection: &rusqlite::Connection,
    session_id: &str,
    caller: &str,
    chair: Option<(&str, &str)>,
) -> Result<(), Box<dyn std::error::Error>> {
    if swarm::store::orchestrator_of(connection, session_id)? != caller {
        return Err("swarm: only the orchestrator can set the session chair".into());
    }
    swarm::store::set_chair(connection, session_id, chair)
}

#[derive(serde::Serialize)]
pub(crate) struct AgentListOutput {
    pub(crate) agents: Vec<swarm::bus::Agent>,
    pub(crate) attachable: bool,
}

pub(crate) fn list_agents(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session_id: &str,
    adapter: &swarm::adapter::Adapter,
) -> Result<AgentListOutput, Box<dyn std::error::Error>> {
    let listing = read_pane_list(adapter)?;
    list_agents_from(
        connection,
        root,
        session_id,
        adapter,
        listing,
        &mut ListingCache::default(),
    )
}

/// What one listing leaves for the next, for a caller that lists again and again.
#[derive(Default)]
pub(crate) struct ListingCache {
    /// Each pane's last screen read, by (adapter, pane, provider): the unix second before the
    /// read started, and what it read.
    screens: std::collections::HashMap<(String, String, Option<String>), (i64, ScreenRead)>,
    /// When each adapter's `list` last failed. It runs again after LIST_RETRY.
    failed_lists: std::collections::HashMap<String, std::time::Instant>,
}

/// How long a failed `list` waits before it runs again, so an adapter whose server is down does
/// not start a process and log an error on every listing.
const LIST_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

/// The longest a kept screen read stands in for a new one. The Claude prompt check also reads the
/// transcript log, which can change while the screen prints nothing.
const SCREEN_REUSE_S: i64 = 10;

fn screen_key(
    adapter: &swarm::adapter::Adapter,
    pane: &str,
    provider: Option<&str>,
) -> (String, String, Option<String>) {
    (
        adapter.name.clone(),
        pane.to_string(),
        provider.map(str::to_string),
    )
}

/// The `activity=<unix s>` token on `pane`'s line of a pane list, the last time its window
/// printed. An adapter without it gets a new screen read on every listing.
fn pane_activity(listing: &str, pane: &str) -> Option<i64> {
    let line = listing
        .lines()
        .find(|line| line.split_whitespace().next() == Some(pane))?;
    line.split_whitespace()
        .find_map(|token| token.strip_prefix("activity=")?.parse().ok())
}

/// The adapter's `list` output, or None when it fails before the deadline.
pub(crate) fn read_pane_list(
    adapter: &swarm::adapter::Adapter,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    pane_list_result(adapter, adapter.run("list", &[]))
}

fn pane_list_result(
    adapter: &swarm::adapter::Adapter,
    result: Result<String, Box<dyn std::error::Error>>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match result {
        Ok(listing) => Ok(Some(listing)),
        Err(error) => {
            adapter.check_deadline()?;
            eprintln!("swarm: {}", error.to_string().replace(['\r', '\n'], " "));
            Ok(None)
        }
    }
}

pub(crate) fn list_agents_from(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session_id: &str,
    adapter: &swarm::adapter::Adapter,
    listing: Option<String>,
    cache: &mut ListingCache,
) -> Result<AgentListOutput, Box<dyn std::error::Error>> {
    let rows = swarm::store::agents(connection, session_id)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let alive: Vec<Option<bool>> = rows
        .iter()
        .map(|row| {
            row.pane.as_deref().and_then(|pane| {
                listing
                    .as_deref()
                    .map(|list| swarm::adapter::listing_has_pane(list, pane))
            })
        })
        .collect();
    // A session with no chair yet still lists; it has no one to report to.
    let chair = swarm::store::orchestrator_of(connection, session_id).ok();
    // A chair that `session new` registered names its CLI only on the session, so its screen is
    // read with the provider `provider_of` resolves. The row's own provider stays as listed.
    let chair_provider = chair.as_ref().and_then(|chair| {
        swarm::store::provider_of(connection, session_id, chair)
            .ok()
            .flatten()
    });
    let providers: Vec<_> = rows
        .iter()
        .map(|row| {
            if Some(&row.id) == chair.as_ref() {
                chair_provider.clone()
            } else {
                row.provider.clone()
            }
        })
        .collect();
    // The screen check (ADR 0021) reads each live pane once per listing, all at the same
    // time, so six agents cost about one capture. A pi pane's host record is read beside it.
    // A pane whose window printed nothing since its last read keeps that read for up to
    // SCREEN_REUSE_S, so the app's 1 s listing starts no process for an idle pane.
    type HostRead = Option<(swarm::host::HookState, i64)>;
    let screens: Vec<(Option<ScreenRead>, HostRead, bool)> = std::thread::scope(|scope| {
        let reads: Vec<_> = rows
            .iter()
            .zip(&alive)
            .zip(&providers)
            .map(|((row, alive), provider)| {
                let (Some(true), Some(pane)) = (alive, row.pane.as_deref()) else {
                    return Err(Box::new((None, None, false)));
                };
                let provider = provider.as_deref();
                let kept = listing
                    .as_deref()
                    .and_then(|list| pane_activity(list, pane))
                    .and_then(|activity| {
                        cache
                            .screens
                            .get(&screen_key(adapter, pane, provider))
                            .filter(|(read_at, _)| {
                                activity < *read_at && now - read_at < SCREEN_REUSE_S
                            })
                    })
                    .map(|(_, read)| read.clone());
                if kept.is_some() && provider != Some("pi") {
                    return Err(Box::new((kept, None, false)));
                }
                let seen_work = matches!(row.state.as_deref(), Some("working" | "waiting"));
                Ok(scope.spawn(move || {
                    let host = host_record_of(adapter, provider, pane, seen_work);
                    let fresh = kept.is_none();
                    let screen =
                        kept.or_else(|| read_screen(adapter, pane, provider, row.log.as_deref()));
                    (screen, host, fresh)
                }))
            })
            .collect();
        reads
            .into_iter()
            .map(|read| match read {
                Ok(handle) => handle.join().unwrap_or((None, None, false)),
                Err(done) => *done,
            })
            .collect()
    });
    for ((row, provider), (screen, _, fresh)) in rows.iter().zip(&providers).zip(&screens) {
        if let (true, Some(pane), Some(screen)) = (*fresh, row.pane.as_deref(), screen) {
            cache.screens.insert(
                screen_key(adapter, pane, provider.as_deref()),
                (now, screen.clone()),
            );
        }
    }
    // All reads must finish in time; a due ring below can run past their deadline.
    adapter.check_deadline()?;
    let mut chair_row_idle = false;
    let mut agents = Vec::new();
    for (((mut row, alive), (screen, host, _)), provider) in
        rows.into_iter().zip(alive).zip(screens).zip(providers)
    {
        let (screen, detail, prompt) = match screen {
            Some((screen, detail, prompt)) => (Some(screen), detail, prompt),
            None => (None, None, None),
        };
        // A host record is stored with its own time when it differs from the row, so a record
        // read again keeps the time its turn began.
        if let Some((record, at)) = &host
            && (
                row.state.as_deref(),
                row.state_kind.as_deref(),
                row.state_source.as_deref(),
                row.state_at,
            ) != (Some(record.state), record.kind, Some("host"), Some(*at))
        {
            match store_state(connection, session_id, &row.id, record, "host", *at) {
                Ok(_) => {
                    row.state = Some(record.state.to_string());
                    row.state_kind = record.kind.map(str::to_string);
                    (row.state_at, row.state_source, row.state_detail) =
                        (Some(*at), Some("host".into()), record.detail.clone());
                }
                Err(error) => eprintln!("swarm: host record not stored: {error}"),
            }
        }
        let (state, write) = if swarm::screen::held_by_send(row.sent_at, row.state_at, screen, now)
        {
            (Some("working".to_string()), None)
        } else if let Some((record, _)) = &host {
            (Some(record.state.to_string()), None)
        } else {
            swarm::screen::resolve(
                provider.as_deref(),
                row.state.as_deref(),
                row.state_at,
                row.state_source.as_deref(),
                screen,
                now,
            )
        };
        // The app runs no `swarm sweep`, and a ring typed while the CLI still starts is lost,
        // so the listing it polls rings a due message again once the pane shows it idle. A
        // fresh hook outranks the screen, so a turn it reports gets no ring typed into it.
        let idle = screen == Some(swarm::screen::ScreenState::Idle)
            && prompt.is_none()
            && !matches!(state.as_deref(), Some("working" | "waiting"));
        if Some(&row.id) == chair.as_ref() {
            chair_row_idle = idle;
        }
        if idle
            && let Some(pane) = row.pane.as_deref()
            && let Err(error) = rering_if_due(
                connection,
                root,
                adapter,
                session_id,
                &row.id,
                pane,
                Proof::Later,
            )
        {
            eprintln!("swarm: {error}");
        }
        if let Some(seen) = write {
            let detail = detail.filter(|_| seen == "failed");
            match swarm::store::set_screen_state(
                connection,
                session_id,
                &row.id,
                seen,
                detail.as_deref(),
                now,
                (
                    row.state.as_deref(),
                    row.state_source.as_deref(),
                    row.state_at,
                    row.state_kind.as_deref(),
                ),
            ) {
                Ok(true) => {
                    if seen == "waiting"
                        && row.state.as_deref() != Some("waiting")
                        && let Err(error) = notice_waiting(connection, session_id, &row.id, None)
                    {
                        eprintln!("swarm: {error}");
                    }
                    (row.state_at, row.state_source, row.state_detail) =
                        (Some(now), Some("screen".into()), detail);
                    row.state_kind = None;
                }
                // A newer hook report landed after this listing read the row; it stands,
                // and the next listing shows it.
                Ok(false) => {}
                Err(error) => eprintln!("swarm: {error}"),
            }
        }
        let state_kind = row
            .state_kind
            .filter(|_| state.as_deref() == Some("waiting"));
        agents.push(swarm::bus::Agent {
            id: row.id,
            role: row.role,
            pane: row.pane,
            provider: row.provider,
            created_at: row.created_at,
            alive,
            state,
            state_at_s: row.state_at,
            state_source: row.state_source,
            state_detail: row.state_detail,
            state_kind,
            log: row.log.map(resolved_agent_log),
            prompt,
            profile: row.profile,
            runner: row.runner,
            model: row.model,
            effort: row.effort,
            account: row.account,
            cost_usd: row.cost_usd,
            tokens: row.tokens,
        });
    }
    // The app runs no `swarm sweep`, so its listing also settles rings and reports lost messages
    // and stalls, after its state writes. The app kills the listing at 20 s, so none of its rings
    // waits for proof; the next pass settles them. A report rings the chair's stored pane, which a
    // restarted tmux or Herdr server can give to another pane, and its Enter would answer a
    // question on the chair's screen, so it waits for a listing that shows the chair's pane idle,
    // the same check as the re-ring above. Rings and settling take seconds, so the chair's screen
    // is read again right before the report.
    if let Err(error) = settle_rings(
        connection,
        root,
        adapter,
        session_id,
        Proof::Later,
        &mut Vec::new(),
    ) {
        eprintln!("swarm: {error}");
    }
    // The chair's screen is read again only when there is a report to send. A failed query
    // counts as one, so the report below prints its error.
    let reports_due = chair.as_ref().is_none_or(|chair| {
        swarm::store::unreported_lost(connection, session_id, chair)
            .map_or(true, |lost| !lost.is_empty())
    }) || swarm::store::stalls(connection, session_id)
        .map_or(true, |stalls| !stalls.is_empty());
    let chair_still_idle = chair_row_idle
        && reports_due
        && chair_idle(connection, adapter, session_id).unwrap_or_else(|error| {
            eprintln!("swarm: {error}");
            false
        });
    if chair_still_idle {
        if let Err(error) = report_lost(connection, root, &adapter.name, session_id, Proof::Later) {
            eprintln!("swarm: {error}");
        }
        if let Err(error) = report_stalls(connection, root, &adapter.name, session_id, Proof::Later)
        {
            eprintln!("swarm: {error}");
        }
    }
    Ok(AgentListOutput {
        agents,
        attachable: adapter.attach.is_some(),
    })
}

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

#[cfg(test)]
#[path = "listing/tests_1.rs"]
mod tests_1;

#[cfg(test)]
#[path = "listing/tests_2.rs"]
mod tests_2;
