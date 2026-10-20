//! Tests for coordinator agent snapshots.

use super::*;
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
    let mut previous = BTreeMap::new();
    let listings = all_agent_listings(&mut connection, root, Duration::from_secs(2)).unwrap();
    let first = agent_snapshot(&connection, &listings, &mut previous);
    assert_eq!(first[&fixture.session][0]["alive"], true);
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
