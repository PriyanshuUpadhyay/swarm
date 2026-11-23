//! The owner's text typed into a pane, and pi's input box emptied.

use super::*;

/// How long `clear_box` waits for pi to show the messages that M-Up moved into its box.
const CLEAR_BOX_DEQUEUE_WAIT: std::time::Duration = std::time::Duration::from_millis(300);
/// How long `clear_box` waits after a round of keys for pi to redraw its box.
const CLEAR_BOX_REDRAW_WAIT: std::time::Duration = std::time::Duration::from_millis(50);
/// How many rounds of C-e, C-u, and BSpace `clear_box` tries before it gives up on the box.
const CLEAR_BOX_ROUNDS: usize = 40;

/// Type the owner's `text` into `agent_id`'s `pane`, for `swarm type`. A CLI that is still
/// starting loses text typed before its input box is drawn, or keeps it unsent in the box, so as a
/// ring does, the text waits for a box that swarm can read, and Enter is pressed again while the
/// box still holds it. A CLI whose box was already drawn takes the text and its Enter, so the
/// owner's send to it returns at once.
pub(crate) fn type_text(
    connection: &rusqlite::Connection,
    adapter: &swarm::adapter::Adapter,
    root: &std::path::Path,
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
            .capture_within(&vars, PANE_READ_TIMEOUT)
            .unwrap_or_default()
    };
    let started = provider
        .as_deref()
        .is_some_and(|provider| wait_for_input_box(provider, &read));
    if provider.as_deref() == Some("pi") {
        let rows = read();
        // pi draws its folder trust question in its box's place, and Enter there picks a choice.
        if swarm::screen::prompt(&rows).is_some() {
            return Err("swarm: pi's screen shows a question; answer it first".into());
        }
        // A ring that pi kept unsent is swarm's own text, so Enter sends it before the owner's.
        let rows = send_held_ring(adapter, root, pane, rows, &read)?;
        // pi puts its queued messages back in its box on a stop, and the text would join them. An
        // unread box types as before.
        if swarm::screen::composer("pi", &rows).is_some_and(|held| !held.is_empty()) {
            return Err("swarm: pi's input box holds text, and this send would join it".into());
        }
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

/// While pi's box holds only the ring text, press Enter, up to `RING_ENTER_RETRIES` times, and
/// return the last rows read. A box with other text is left as it is.
fn send_held_ring(
    adapter: &swarm::adapter::Adapter,
    root: &std::path::Path,
    pane: &str,
    mut rows: String,
    read: &impl Fn() -> String,
) -> Result<String, Box<dyn std::error::Error>> {
    let squash = |text: &str| text.split_whitespace().collect::<String>();
    let ring = squash(&ring_text(root));
    for _ in 0..RING_ENTER_RETRIES {
        let only_ring =
            swarm::screen::composer("pi", &rows).is_some_and(|held| squash(&held) == ring);
        if !only_ring || adapter.key.is_none() {
            break;
        }
        adapter.run("key", &[("pane", pane), ("key", "Enter")])?;
        std::thread::sleep(RING_POLL);
        rows = read();
    }
    Ok(rows)
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
        std::thread::sleep(CLEAR_BOX_DEQUEUE_WAIT);
    }
    let mut first = None;
    for round in 0..=CLEAR_BOX_ROUNDS {
        let rows = adapter
            .capture_within(&[("pane", pane)], PANE_READ_TIMEOUT)
            .unwrap_or_default();
        match swarm::screen::composer("pi", &rows) {
            None => return Err("swarm: pi's input box could not be read".into()),
            Some(held) if held.is_empty() => {
                return Ok(first.map_or("empty".into(), |held| format!("cleared\n{held}")));
            }
            Some(_) if round == CLEAR_BOX_ROUNDS => break,
            Some(held) => {
                first.get_or_insert(held);
            }
        }
        for name in ["C-e", "C-u", "BSpace"] {
            key(name)?;
        }
        std::thread::sleep(CLEAR_BOX_REDRAW_WAIT);
    }
    Err(format!("swarm: pi's input box still holds text after {CLEAR_BOX_ROUNDS} rounds").into())
}
