//! Tests for coordinator agent snapshots.

use super::*;
use crate::all_agent_listings;
use crate::serve::test_support::Fixture;

#[test]
fn a_missing_listing_keeps_the_last_live_prompt_until_a_new_listing() {
    let fixture = Fixture::new();
    let root = &fixture.app.root;
    std::fs::create_dir_all(root.join("adapters")).unwrap();
    let screen = swarm::adapter::shell_line(&[fixture.dir.join("screen").display().to_string()]);
    std::fs::write(
        fixture.dir.join("screen"),
        include_str!("../../../tests/fixtures/screens/claude-question.txt"),
    )
    .unwrap();
    let adapter = |list| {
        std::fs::write(
            root.join("adapters/fake.conf"),
            format!(
                "self = true\nspawn = true\nring = true\nlist = {list}\nclose = true\ncapture = true\nscreen = cat {screen}\n"
            ),
        )
        .unwrap();
    };
    adapter("printf pane");
    let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
    connection
        .execute(
            "UPDATE session SET adapter = 'fake' WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    swarm::store::set_pane(&connection, &fixture.session, "worker", "pane").unwrap();
    swarm::store::set_provider(&connection, &fixture.session, "worker", "claude").unwrap();
    let state_at = discovery::now();
    swarm::store::set_state(
        &connection,
        &fixture.session,
        "worker",
        "working",
        "hook",
        None,
        None,
        state_at,
    )
    .unwrap();
    let mut previous = BTreeMap::new();
    let listings = all_agent_listings(&mut connection, root, Duration::from_secs(2)).unwrap();
    let first = agent_snapshot(&connection, &listings, &mut previous);
    assert_eq!(first[&fixture.session][0]["alive"], true);
    assert_eq!(first[&fixture.session][0]["state"], "working");
    assert_eq!(first[&fixture.session][0]["state_at_s"], state_at);
    assert_eq!(
        first[&fixture.session][0]["prompt"]["question"],
        "☐ Color\nWhich color do you prefer?"
    );

    let listings = all_agent_listings(&mut connection, root, Duration::ZERO).unwrap();
    assert!(!listings.contains_key(&fixture.session));
    let failed = agent_snapshot(&connection, &listings, &mut previous);
    assert_eq!(failed[&fixture.session][0]["alive"], true);
    assert_eq!(failed, first);

    adapter("true");
    let listings = all_agent_listings(&mut connection, root, Duration::from_secs(2)).unwrap();
    let recovered = agent_snapshot(&connection, &listings, &mut previous);
    assert_eq!(recovered[&fixture.session][0]["alive"], false);
    assert!(recovered[&fixture.session][0]["prompt"].is_null());
    let failed_again = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert_eq!(failed_again, recovered);
}

#[test]
fn a_session_without_a_good_listing_keeps_reading_stored_agents() {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    let mut previous = BTreeMap::new();
    let first = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert_eq!(first[&fixture.session].as_array().unwrap().len(), 1);
    assert!(first[&fixture.session][0]["alive"].is_null());
    assert!(first[&fixture.session][0]["prompt"].is_null());

    swarm::store::add_agent(&connection, &fixture.session, "new-worker", "code").unwrap();
    let next = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert_eq!(next[&fixture.session].as_array().unwrap().len(), 2);
    assert!(previous.is_empty());
}

fn cached_waiting_worker() -> (Fixture, rusqlite::Connection, BTreeMap<String, Value>) {
    let fixture = Fixture::new();
    let connection = swarm::store::open(&fixture.app.root.join("swarm.db")).unwrap();
    swarm::store::set_pane(&connection, &fixture.session, "worker", "pane").unwrap();
    swarm::store::set_state(
        &connection,
        &fixture.session,
        "worker",
        "waiting",
        "hook",
        Some("Choose"),
        None,
        100,
    )
    .unwrap();
    let mut rows = stored_agents(&connection, &fixture.session).unwrap();
    rows[0]["alive"] = json!(true);
    rows[0]["prompt"] = json!({"question":"Choose", "id":"old-prompt"});
    let previous = BTreeMap::from([(fixture.session.clone(), rows)]);
    (fixture, connection, previous)
}

#[test]
fn a_missed_listing_shows_new_agents_and_drops_removed_agents() {
    let (fixture, connection, mut previous) = cached_waiting_worker();
    swarm::store::add_agent(&connection, &fixture.session, "new-worker", "code").unwrap();
    let snapshot = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    let rows = snapshot[&fixture.session].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let new = rows.iter().find(|row| row["id"] == "new-worker").unwrap();
    assert!(new["alive"].is_null());
    assert!(new["prompt"].is_null());
    connection
        .execute(
            "DELETE FROM agent WHERE session_id = ?1 AND id = 'worker'",
            [&fixture.session],
        )
        .unwrap();
    let snapshot = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert_eq!(snapshot[&fixture.session].as_array().unwrap().len(), 1);
    assert_eq!(snapshot[&fixture.session][0]["id"], "new-worker");
}

#[test]
fn a_missed_listing_keeps_hook_changes_and_drops_the_old_prompt() {
    let (fixture, connection, mut previous) = cached_waiting_worker();
    swarm::store::set_state(
        &connection,
        &fixture.session,
        "worker",
        "waiting",
        "hook",
        Some("New detail"),
        Some("permission"),
        200,
    )
    .unwrap();
    let snapshot = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    let row = &snapshot[&fixture.session][0];
    assert_eq!(row["state_detail"], "New detail");
    assert_eq!(row["state_kind"], "permission");
    assert_eq!(row["state_at_s"], 200);
    assert_eq!(row["alive"], true);
    assert!(row["prompt"].is_null());
}

#[test]
fn a_degraded_listing_drops_cached_observations_after_every_pane_is_cleared() {
    let (fixture, mut connection, mut previous) = cached_waiting_worker();
    swarm::store::clear_pane(&connection, &fixture.session, "worker").unwrap();
    let adapter = swarm::adapter::parse(
        "fake",
        "self = true\nspawn = true\nring = true\nlist = true\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let listing = crate::cli::list_agents_from(
        &mut connection,
        &fixture.app.root,
        &fixture.session,
        &adapter,
        Some(String::new()),
        &mut Default::default(),
    )
    .unwrap();
    assert!(listing.agents.iter().all(|agent| agent.alive.is_none()));
    let listings = BTreeMap::from([(fixture.session.clone(), listing)]);
    let snapshot = agent_snapshot(&connection, &listings, &mut previous);
    let stored = stored_agents(&connection, &fixture.session).unwrap();
    assert_eq!(snapshot[&fixture.session][0]["alive"], stored[0]["alive"]);
    assert!(snapshot[&fixture.session][0]["prompt"].is_null());
}

#[test]
fn a_missed_listing_drops_cached_observations_for_a_replaced_pane() {
    let (fixture, connection, mut previous) = cached_waiting_worker();
    swarm::store::set_pane(&connection, &fixture.session, "worker", "new-pane").unwrap();
    let snapshot = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert_eq!(snapshot[&fixture.session][0]["pane"], "new-pane");
    assert!(snapshot[&fixture.session][0]["alive"].is_null());
    assert!(snapshot[&fixture.session][0]["prompt"].is_null());
}

#[test]
fn a_missed_listing_keeps_the_resolved_state_when_the_stored_timestamp_is_unchanged() {
    let (fixture, connection, mut previous) = cached_waiting_worker();
    swarm::store::set_state(
        &connection,
        &fixture.session,
        "worker",
        "working",
        "hook",
        None,
        None,
        100,
    )
    .unwrap();
    let stored = stored_agents(&connection, &fixture.session).unwrap();
    assert_eq!(stored[0]["state"], "working");
    assert_eq!(stored[0]["state_at_s"], 100);
    assert_eq!(previous[&fixture.session][0]["state"], "waiting");
    assert_eq!(previous[&fixture.session][0]["state_at_s"], 100);
    let snapshot = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert_eq!(snapshot[&fixture.session][0]["state"], "waiting");
    assert_eq!(snapshot[&fixture.session][0]["prompt"]["id"], "old-prompt");
    assert_eq!(snapshot[&fixture.session][0]["alive"], true);
}

#[test]
fn a_degraded_listing_keeps_the_last_good_prompt_and_cache() {
    let (fixture, mut connection, mut previous) = cached_waiting_worker();
    let adapter = swarm::adapter::parse(
        "fake",
        "self = true\nspawn = true\nring = true\nlist = false\nclose = true\ncapture = true\n",
    )
    .unwrap();
    let listing = crate::cli::list_agents_from(
        &mut connection,
        &fixture.app.root,
        &fixture.session,
        &adapter,
        None,
        &mut Default::default(),
    )
    .unwrap();
    assert!(listing.agents.iter().all(|agent| agent.alive.is_none()));
    let listings = BTreeMap::from([(fixture.session.clone(), listing)]);
    let good = previous.clone();
    let snapshot = agent_snapshot(&connection, &listings, &mut previous);
    assert_eq!(snapshot[&fixture.session][0]["prompt"]["id"], "old-prompt");
    assert_eq!(snapshot[&fixture.session][0]["alive"], true);
    assert_eq!(previous, good);
}

#[test]
fn an_answered_stored_state_drops_a_cached_prompt() {
    let (fixture, connection, mut previous) = cached_waiting_worker();
    swarm::store::set_state(
        &connection,
        &fixture.session,
        "worker",
        "working",
        "hook",
        None,
        None,
        200,
    )
    .unwrap();
    let snapshot = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert_eq!(snapshot[&fixture.session][0]["state"], "working");
    assert!(snapshot[&fixture.session][0]["prompt"].is_null());
    assert_eq!(snapshot[&fixture.session][0]["alive"], true);
}

#[test]
fn a_session_no_longer_listed_by_the_store_drops_its_cached_rows() {
    let (fixture, mut connection, mut previous) = cached_waiting_worker();
    swarm::store::archive_sessions(&mut connection, std::slice::from_ref(&fixture.session))
        .unwrap();
    let snapshot = agent_snapshot(&connection, &BTreeMap::new(), &mut previous);
    assert!(snapshot.get(&fixture.session).is_none());
    assert!(previous.is_empty());
}
