//! CLI ring.

use super::*;

pub(crate) const RERING_UNSEEN_AFTER_SECS: i64 = 60;

/// The ring is typed into the recipient's prompt, so it names the next step. An agent that did
/// not load the swarm skill otherwise takes the bare ring as the whole task and waits. The ack comes
/// after the work: an acked message leaves the inbox, so an agent whose context is compacted
/// mid-task can find the task again only while it is unacked. The re-ring keys on `seen_at`, so a
/// late ack never rings twice.
pub(crate) fn ring_text(root: &std::path::Path) -> String {
    format!(
        "swarm: new message. Run swarm inbox and read each body at {}/<body_path>. Run swarm ack <seq> only after you finish that message.",
        root.display()
    )
}

/// How long a ring waits for a starting CLI to draw its input box.
pub(crate) const RING_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// How many more times a ring presses Enter while the input box still holds it.
pub(crate) const RING_ENTER_RETRIES: usize = 3;
/// How long one ring may take in all, its proof wait included, so a hung adapter verb or a lost
/// ring cannot hold the caller for ever.
pub(crate) const RING_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(if cfg!(test) { 5 } else { 30 });
/// How often a ring reads the pane and the store for its proof.
pub(crate) const RING_POLL: std::time::Duration = std::time::Duration::from_millis(500);

/// Now in unix seconds, the unit of `rung_at` and `state_at`.
pub(crate) fn unix_now() -> Result<i64, std::time::SystemTimeError> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64)
}

/// What one ring proved (ADR 0041), stored on `message.delivery`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// The recipient's own report, from its hook or its host record, showed a turn after the ring.
    Hook,
    /// The recipient's screen showed a turn or a question after the ring. A busy CLI queues or
    /// steers the ring and fires no turn-start hook (ADR 0038), so this is its only proof.
    Screen,
    /// The recipient read its messages after the ring, though no hook or screen shows a turn now:
    /// a short turn can end between two listing passes, and its `done` hides its turn start.
    Seen,
    /// No proof came before the ring's deadline.
    Unconfirmed,
    /// No proof can come: the pane's CLI is unknown and its screen is no Herdr status.
    Unchecked,
}

impl Delivery {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Delivery::Hook => "hook",
            Delivery::Screen => "screen",
            Delivery::Seen => "seen",
            Delivery::Unconfirmed => "unconfirmed",
            Delivery::Unchecked => "unchecked",
        }
    }
}

/// Whether a ring waits for its proof (ADR 0041). The app kills `agents --json` at 20 s, so the
/// listing types its rings and returns, and `settle_rings` in a later pass finds their proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Proof {
    Wait,
    Later,
}

/// Type the ring into `agent`'s `pane` and, with `Proof::Wait`, wait for proof that it started a
/// turn. A CLI that is still starting loses a ring typed before its input box is drawn, or reads
/// the text and its Enter as one paste, so the Enter becomes a newline and the ring sits unsent in
/// the box. So a waiting ring to a CLI whose box swarm can read waits for the box, and presses
/// Enter again while the box still holds the ring. With `Proof::Later` it skips both waits and
/// returns None once the ring is typed; a ring that a starting CLI loses is rung again after 60 s,
/// like any unseen message. A waiting ring also returns None when a store error hides its proof.
pub(crate) fn ring_pane(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    root: &std::path::Path,
    session_id: &str,
    agent_id: &str,
    pane: &str,
    proof: Proof,
) -> Result<Option<Delivery>, Box<dyn std::error::Error>> {
    let text = ring_text(root);
    let provider = swarm::store::provider_of(connection, session_id, agent_id)?;
    let provider = provider.as_deref();
    // The ring's own deadline replaces a batch deadline, so a ring that starts within the batch
    // still gets the time to send its text and Enter, and to see its proof.
    let deadline = std::time::Instant::now() + RING_TIMEOUT;
    let adapter = swarm::adapter::Adapter {
        deadline: Some(deadline),
        ..adapter.clone()
    };
    let vars = [("pane", pane)];
    let read = || {
        adapter
            .capture_within(&vars, std::time::Duration::from_secs(1))
            .unwrap_or_default()
    };
    if proof == Proof::Wait {
        match provider {
            Some(provider) if swarm::screen::reads_composer(provider) => {
                let _ = wait_for_input_box(provider, &read);
            }
            Some("agy") if adapter.status.is_some() => wait_for_record(&adapter, pane),
            _ => {}
        }
    }
    let rung_at = unix_now()?;
    adapter.run("ring", &[("pane", pane), ("text", &text)])?;
    if proof == Proof::Later {
        return Ok(None);
    }
    let mut enters = 0;
    loop {
        std::thread::sleep(RING_POLL);
        let rows = read();
        let screen = screen_of(&adapter, &vars, &rows);
        if provider.is_some_and(|provider| swarm::screen::holds(provider, &rows, &text)) {
            if enters < RING_ENTER_RETRIES && adapter.key.is_some() {
                adapter.run("key", &[("pane", pane), ("key", "Enter")])?;
                enters += 1;
            }
        } else {
            refresh_host(connection, &adapter, session_id, agent_id, provider, pane);
            match ring_proof(
                connection,
                session_id,
                agent_id,
                provider,
                &rows,
                screen.as_deref(),
                rung_at,
            ) {
                Ok(None) => {}
                Ok(delivery) => return Ok(delivery),
                // A store error proves nothing either way, so the ring keeps no result and a
                // later sweep or listing pass settles it.
                Err(error) => {
                    eprintln!("swarm: ring proof not read: {error}");
                    return Ok(None);
                }
            }
        }
        if std::time::Instant::now() + RING_POLL >= deadline {
            return Ok(Some(Delivery::Unconfirmed));
        }
    }
}

/// Wait until the agent in `pane` reports a program-status record, for at most
/// `RING_READY_TIMEOUT`. AGY draws no input box that swarm reads, and it reports `idle` once it
/// takes input, or `blocked` while a dialog, such as folder trust, waits.
pub(crate) fn wait_for_record(adapter: &swarm::adapter::Adapter, pane: &str) {
    let ready = std::time::Instant::now() + RING_READY_TIMEOUT;
    while std::time::Instant::now() < ready {
        if adapter
            .status(&[("pane", pane)], std::time::Duration::from_secs(1))
            .is_some()
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// A pi agent's host record and its time from the adapter's `status` verb (ADR 0070: hooks first,
/// host records second). Other providers have hooks, so they get None.
pub(crate) fn host_record_of(
    adapter: &swarm::adapter::Adapter,
    provider: Option<&str>,
    pane: &str,
    seen_work: bool,
) -> Option<(swarm::host::HookState, i64)> {
    if provider != Some("pi") {
        return None;
    }
    let body = adapter.status(&[("pane", pane)], std::time::Duration::from_millis(300))?;
    // A time ahead of this clock would prove every later ring, so such a record is no record.
    swarm::host::host_record(&body, seen_work)
        .filter(|(_, at)| unix_now().is_ok_and(|now| *at <= now))
}

/// Stores a pi agent's host record, so `ring_proof` reads a turn that the record shows. A failed
/// read or write proves nothing either way, so the next pass reads again.
pub(crate) fn refresh_host(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent_id: &str,
    provider: Option<&str>,
    pane: &str,
) {
    let Some((record, at)) = host_record_of(adapter, provider, pane, false) else {
        return;
    };
    if let Err(error) = store_state(connection, session_id, agent_id, &record, "host", at) {
        eprintln!("swarm: host record not stored: {error}");
    }
}

/// Wait until `provider`'s CLI draws its input box, for at most `RING_READY_TIMEOUT`. A question on
/// screen, such as a folder trust prompt, also ends the wait. True when the first read ends it, so
/// the CLI was not starting.
pub(crate) fn wait_for_input_box(provider: &str, read: &impl Fn() -> String) -> bool {
    let ready = std::time::Instant::now() + RING_READY_TIMEOUT;
    let mut first = true;
    while std::time::Instant::now() < ready {
        let rows = read();
        if swarm::screen::composer(provider, &rows).is_some()
            || swarm::screen::prompt(&rows).is_some()
        {
            return first;
        }
        first = false;
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

/// Type the owner's `text` into `agent_id`'s `pane`, for `swarm type`. A CLI that is still
/// starting loses text typed before its input box is drawn, or keeps it unsent in the box, so as a
/// ring does, the text waits for a box that swarm can read, and Enter is pressed again while the
/// box still holds it. A CLI whose box was already drawn takes the text and its Enter, so the
/// owner's send to it returns at once.
pub(crate) fn type_text(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    session_id: &str,
    agent_id: &str,
    pane: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let provider = swarm::store::provider_of(connection, session_id, agent_id)?
        .filter(|provider| swarm::screen::reads_composer(provider));
    let vars = [("pane", pane)];
    let read = || {
        adapter
            .capture_within(&vars, std::time::Duration::from_secs(1))
            .unwrap_or_default()
    };
    let started = provider
        .as_deref()
        .is_some_and(|provider| wait_for_input_box(provider, &read));
    // pi puts its queued messages back in its box on a stop, and the text would join them. An
    // unread box types as before.
    if provider.as_deref() == Some("pi")
        && swarm::screen::composer("pi", &read()).is_some_and(|held| !held.is_empty())
    {
        return Err("swarm: pi's input box holds text, and this send would join it".into());
    }
    adapter.run("ring", &[("pane", pane), ("text", text)])?;
    let Some(provider) = provider.filter(|_| !started && adapter.key.is_some()) else {
        return Ok(());
    };
    for _ in 0..RING_ENTER_RETRIES {
        std::thread::sleep(RING_POLL);
        if !swarm::screen::holds(&provider, &read(), text) {
            break;
        }
        adapter.run("key", &[("pane", pane), ("key", "Enter")])?;
    }
    Ok(())
}

/// Empty pi's input box in `pane`, for `swarm clear-box`. With `dequeue`, M-Up first puts pi's
/// queued messages in the box, as pi's own Alt+Up does. Each round of C-e, C-u, and BSpace takes
/// one line; Escape and C-c would stop pi's turn. The box is read before each round, so an unread
/// box is never cleared blind. Returns "empty" when the box held nothing, else "cleared" and the
/// text it held, which a message that pi took just before M-Up is not in.
pub(crate) fn clear_box(
    adapter: &swarm::adapter::Adapter,
    pane: &str,
    dequeue: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let key = |key: &str| adapter.run("key", &[("pane", pane), ("key", key)]);
    if dequeue {
        key("M-Up")?;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let mut first = None;
    for round in 0..=40 {
        let rows = adapter
            .capture_within(&[("pane", pane)], std::time::Duration::from_secs(1))
            .unwrap_or_default();
        match swarm::screen::composer("pi", &rows) {
            None => return Err("swarm: pi's input box could not be read".into()),
            Some(held) if held.is_empty() => {
                return Ok(first.map_or("empty".into(), |held| format!("cleared\n{held}")));
            }
            Some(_) if round == 40 => break,
            Some(held) => {
                first.get_or_insert(held);
            }
        }
        for name in ["C-e", "C-u", "BSpace"] {
            key(name)?;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err("swarm: pi's input box still holds text after 40 rounds".into())
}

/// The pane's screen for `ring_proof`: the `screen` verb's output, or the capture `rows` when the
/// adapter has no `screen` verb. None when the `screen` verb failed.
pub(crate) fn screen_of(
    adapter: &swarm::adapter::Adapter,
    vars: &[(&str, &str)],
    rows: &str,
) -> Option<String> {
    match adapter.screen {
        Some(_) => adapter.screen(vars, std::time::Duration::from_secs(1)),
        None => Some(rows.to_string()),
    }
}

/// What the store and the pane prove now about a ring typed at `rung_at`, if anything. The
/// caller checks first that the input box no longer holds the ring. A `screen` of None, a failed
/// read, proves nothing, so the pane is not taken as one with no reader. A store error is
/// returned, not read as no proof, so a late ring is not stored as unconfirmed because of it.
pub(crate) fn ring_proof(
    connection: &rusqlite::Connection,
    session_id: &str,
    agent_id: &str,
    provider: Option<&str>,
    rows: &str,
    screen: Option<&str>,
    rung_at: i64,
) -> Result<Option<Delivery>, Box<dyn std::error::Error>> {
    if provider.is_none()
        && screen.is_some_and(|screen| swarm::screen::herdr_state(screen).is_none())
    {
        return Ok(Some(Delivery::Unchecked));
    }
    if swarm::store::turn_started(connection, session_id, agent_id, rung_at)? {
        return Ok(Some(Delivery::Hook));
    }
    if let Some((swarm::screen::ScreenState::Working | swarm::screen::ScreenState::Waiting, _)) =
        swarm::screen::read_pane(provider, screen.unwrap_or(rows), || Some(rows.to_string()))
    {
        return Ok(Some(Delivery::Screen));
    }
    Ok(
        swarm::store::seen_since(connection, session_id, agent_id, rung_at)?
            .then_some(Delivery::Seen),
    )
}

/// Ring `agent` for the messages `seqs`, stored as rung at `rung_at`, and store what the ring
/// proved on them. The bell is a hint (R9), so a failure only warns, and a failed ring is
/// unconfirmed. With `Proof::Later` a ring, typed or failed, stores nothing and returns None, and
/// so does a ring whose proof read failed in the store; `settle_rings` stores its proof, so the
/// chair's own failed ring is still left to the sweep line. A result that this call did not store,
/// because the write failed or another pass stored first, is None too, so only the pass that
/// stores a ring's result reports it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ring_and_record(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    adapter: Result<swarm::adapter::Adapter, Box<dyn std::error::Error>>,
    session_id: &str,
    agent: &str,
    pane: &str,
    seqs: &[i64],
    rung_at: i64,
    proof: Proof,
) -> Option<Delivery> {
    let delivery = adapter
        .and_then(|adapter| ring_pane(connection, &adapter, root, session_id, agent, pane, proof))
        .unwrap_or_else(|error| {
            eprintln!("swarm: ring failed: {error}");
            (proof == Proof::Wait).then_some(Delivery::Unconfirmed)
        })?;
    if delivery == Delivery::Unconfirmed {
        eprintln!(
            "swarm: {agent} started no turn within {} s; ring unconfirmed",
            RING_TIMEOUT.as_secs()
        );
    }
    match swarm::store::set_delivery(connection, session_id, seqs, rung_at, delivery.as_str()) {
        Ok(stored) => stored.then_some(delivery),
        Err(error) => {
            eprintln!("swarm: ring result not stored: {error}");
            None
        }
    }
}

#[cfg(test)]
#[path = "ring/tests_1.rs"]
mod tests_1;

#[cfg(test)]
#[path = "ring/tests_2.rs"]
mod tests_2;

#[cfg(test)]
#[path = "ring/tests_type.rs"]
mod tests_type;
