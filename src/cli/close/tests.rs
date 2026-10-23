//! Tests for `swarm close`.

use super::*;
use crate::cli::test_support::*;

#[test]
fn a_wrap_up_reply_needs_every_heading() {
    let root = std::env::temp_dir().join(format!("swarm-wrap-up-{}", std::process::id()));
    let mut connection = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
    let session = swarm::store::create_session(
        &connection,
        "lane",
        std::path::Path::new("/test"),
        None,
        None,
    )
    .unwrap();
    swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
    swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
    let full = WRAP_UP.map(|(h, _)| format!("{h}\nnone\n")).concat();
    let check = |connection: &rusqlite::Connection, body: &str| {
        check_wrap_up_reply(connection, &session, CODER, body).map_err(|e| e.to_string())
    };

    assert!(check(&connection, "a plain reply").is_ok());
    let send = |connection: &mut rusqlite::Connection, sender, recipient, kind| {
        swarm::store::send_message(connection, &root, &session, sender, recipient, kind, "x")
            .unwrap();
    };
    send(&mut connection, ORCHESTRATOR, CODER, "wrap-up");
    let refused = check(&connection, &full.replace("## Why\n", "")).unwrap_err();
    assert!(refused.contains("needs the line ## Why;"), "{refused}");
    assert!(
        check(&connection, "a plain reply")
            .unwrap_err()
            .contains("## Done, ## Why")
    );
    assert!(check(&connection, &full).is_ok());
    send(&mut connection, CODER, ORCHESTRATOR, "summary");
    assert!(check(&connection, "a plain reply").is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

/// A file store with the orchestrator on `%1` and the coder on `%2`, whose fake adapter's
/// `capture` prints `screen text`.
fn closing(name: &str) -> (std::path::PathBuf, rusqlite::Connection, String) {
    let root = std::env::temp_dir().join(format!("swarm-{name}-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(root.join("adapters")).unwrap();
    std::fs::write(
        root.join("adapters/fake.conf"),
        "self = true\nspawn = true\nring = true\nlist = echo %1 %2\nclose = true\n\
         capture = echo screen text\n",
    )
    .unwrap();
    let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
    let session = swarm::store::create_session(&connection, "lane", &root, None, None).unwrap();
    swarm::store::add_agent(&connection, &session, ORCHESTRATOR, "orchestrator").unwrap();
    swarm::store::add_agent(&connection, &session, CODER, "coder").unwrap();
    swarm::store::set_pane(&connection, &session, ORCHESTRATOR, "%1").unwrap();
    swarm::store::set_pane(&connection, &session, CODER, "%2").unwrap();
    (root, connection, session)
}

/// A pane that will not close is reported, and the panes after it still close.
#[test]
fn a_failed_close_does_not_stop_the_others() {
    let (root, connection, session) = closing("close-failed");
    std::fs::write(
        root.join("adapters/fake.conf"),
        "self = true\nspawn = true\nring = true\nlist = echo %1 %2 %3\ncapture = true\nclose = test \"$SWARM_PANE\" != %2\n",
    )
    .unwrap();
    swarm::store::add_agent(&connection, &session, "tester", "tester").unwrap();
    swarm::store::set_pane(&connection, &session, "tester", "%3").unwrap();
    let mut connection = connection;
    let ids = [CODER.to_string(), "tester".to_string()];

    let error = close_agents(&mut connection, &root, "fake", &session, &ids, None).unwrap_err();

    assert!(error.to_string().contains("coder: not closed"), "{error}");
    assert_eq!(
        swarm::store::pane_of(&connection, &session, CODER).unwrap(),
        Some("%2".into())
    );
    assert_eq!(
        swarm::store::pane_of(&connection, &session, "tester").unwrap(),
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn close(
    connection: &mut rusqlite::Connection,
    root: &std::path::Path,
    session: &str,
    wait: Option<u64>,
) {
    // The same id twice gets one ask and one close.
    let ids = [CODER.to_string(), CODER.to_string()];
    let wait = wait.map(std::time::Duration::from_secs);
    close_agents(connection, root, "fake", session, &ids, wait).unwrap();
    assert_eq!(
        swarm::store::pane_of(connection, session, CODER).unwrap(),
        None
    );
}

/// The child answers the wrap-up from its own process, as `swarm finish` does.
#[test]
fn close_saves_the_childs_wrap_up_then_closes_its_pane() {
    let (root, mut connection, session) = closing("close-wrap-up");
    let body = WRAP_UP.map(|(h, _)| format!("{h}\nnone\n")).concat();
    let child = {
        let (root, session, body) = (root.clone(), session.clone(), body.clone());
        std::thread::spawn(move || {
            let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !swarm::store::owes_wrap_up(&connection, &session, CODER).unwrap() {
                assert!(std::time::Instant::now() < deadline, "no wrap-up was sent");
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            check_wrap_up_reply(&connection, &session, CODER, &body).unwrap();
            swarm::store::send_message(
                &mut connection,
                &root,
                &session,
                CODER,
                ORCHESTRATOR,
                "summary",
                &body,
            )
            .unwrap();
        })
    };

    close(&mut connection, &root, &session, Some(30));

    child.join().unwrap();
    let asks = swarm::store::messages(&connection, &session, -1).unwrap();
    assert_eq!(asks.iter().filter(|m| m.kind == "wrap-up").count(), 1);
    let saved = root.join(format!("runs/{session}/{CODER}.summary.md"));
    assert_eq!(std::fs::read_to_string(saved).unwrap(), body);
    std::fs::remove_dir_all(root).unwrap();
}

/// Wait until `close` has asked the coder for a wrap-up, then send each text as a summary.
fn reply(root: &std::path::Path, session: &str, texts: Vec<String>) -> std::thread::JoinHandle<()> {
    let (root, session) = (root.to_path_buf(), session.to_string());
    std::thread::spawn(move || {
        let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !swarm::store::owes_wrap_up(&connection, &session, CODER).unwrap() {
            assert!(std::time::Instant::now() < deadline, "no wrap-up was sent");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        for text in texts {
            swarm::store::send_message(
                &mut connection,
                &root,
                &session,
                CODER,
                ORCHESTRATOR,
                "summary",
                &text,
            )
            .unwrap();
        }
    })
}

/// A summary whose body file is gone is skipped, and the wrap-up after it is still saved.
#[test]
fn a_missing_summary_file_does_not_stop_close() {
    let (root, mut connection, session) = closing("close-missing");
    let body = WRAP_UP.map(|(h, _)| format!("{h}\nnone\n")).concat();
    let child = {
        let (root, session) = (root.clone(), session.clone());
        let first = reply(&root, &session, vec!["working".into()]);
        let body = body.clone();
        std::thread::spawn(move || {
            first.join().unwrap();
            let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
            let working = swarm::store::messages(&connection, &session, -1)
                .unwrap()
                .into_iter()
                .rfind(|message| message.kind == "summary")
                .unwrap()
                .body_path;
            std::fs::remove_file(root.join(&working)).unwrap();
            swarm::store::send_message(
                &mut connection,
                &root,
                &session,
                CODER,
                ORCHESTRATOR,
                "summary",
                &body,
            )
            .unwrap();
        })
    };

    close(&mut connection, &root, &session, Some(30));

    child.join().unwrap();
    let saved = root.join(format!("runs/{session}/{CODER}.summary.md"));
    assert_eq!(std::fs::read_to_string(saved).unwrap(), body);
    std::fs::remove_dir_all(root).unwrap();
}

/// A wrap-up that cannot be written fails the close, and the pane still closes.
#[test]
fn a_wrap_up_that_cannot_be_saved_fails_the_close() {
    let (root, mut connection, session) = closing("close-unsaved");
    std::fs::create_dir_all(root.join(format!("runs/{session}/{CODER}.summary.md/x"))).unwrap();
    let body = WRAP_UP.map(|(h, _)| format!("{h}\nnone\n")).concat();
    let child = reply(&root, &session, vec![body]);
    let ids = [CODER.to_string()];

    let result = close_agents(
        &mut connection,
        &root,
        "fake",
        &session,
        &ids,
        Some(WRAP_UP_WAIT),
    );

    child.join().unwrap();
    let error = result.unwrap_err().to_string();
    assert!(error.contains("coder: wrap-up not saved"), "{error}");
    assert_eq!(
        swarm::store::pane_of(&connection, &session, CODER).unwrap(),
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Hung screen reads share one limit, so many silent children do not hold `close` long.
#[test]
fn hung_captures_share_one_limit() {
    let (root, mut connection, session) = closing("close-hung-many");
    std::fs::write(
        root.join("adapters/fake.conf"),
        "self = true\nspawn = true\nring = true\nlist = echo %1 %2 %3 %4 %5\nclose = true\n\
         capture = sleep 60\n",
    )
    .unwrap();
    let mut ids = vec![CODER.to_string()];
    for (id, pane) in [("c3", "%3"), ("c4", "%4"), ("c5", "%5")] {
        swarm::store::add_agent(&connection, &session, id, "coder").unwrap();
        swarm::store::set_pane(&connection, &session, id, pane).unwrap();
        ids.push(id.to_string());
    }
    let started = std::time::Instant::now();

    close_agents(
        &mut connection,
        &root,
        "fake",
        &session,
        &ids,
        Some(std::time::Duration::from_secs(1)),
    )
    .unwrap();

    // Four reads of CAPTURE_WAIT each would take 40 s; the shared limit ends them after
    // CAPTURES_WAIT, and the rest is time for the rings and closes on a loaded machine.
    assert!(
        started.elapsed() < 4 * CAPTURE_WAIT,
        "{:?}",
        started.elapsed()
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// A screen capture that hangs does not hold `close` past its limit.
#[test]
fn a_hung_capture_does_not_hold_close() {
    let (root, mut connection, session) = closing("close-hung");
    std::fs::write(
        root.join("adapters/fake.conf"),
        "self = true\nspawn = true\nring = true\nlist = echo %1 %2\nclose = true\n\
         capture = sleep 60\n",
    )
    .unwrap();
    let started = std::time::Instant::now();

    close(&mut connection, &root, &session, Some(1));

    // The capture hangs for 60 s.
    assert!(started.elapsed() < std::time::Duration::from_secs(50));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_child_with_no_wrap_up_in_time_leaves_its_screen_and_a_note() {
    let (root, mut connection, session) = closing("close-silent");

    close(&mut connection, &root, &session, Some(1));

    let log = root.join(format!("runs/{session}/{CODER}.log"));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "screen text\n");
    let inbox = swarm::store::inbox(&connection, &session, ORCHESTRATOR).unwrap();
    let note = std::fs::read_to_string(root.join(&inbox[0].body_path)).unwrap();
    assert_eq!(
        note,
        format!(
            "agent coder closed without a wrap-up; screen log: {}",
            log.display()
        )
    );
    assert!(swarm::store::answered(&connection, &session, CODER).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

/// A child that dies during the wait gets the fallback note, which is not its wrap-up.
#[test]
fn a_death_note_is_not_saved_as_the_wrap_up() {
    let (root, mut connection, session) = closing("close-dead");
    // The pane outlives the child, as a Herdr shell pane does, so `close` must still close it.
    let closed = root.join("closed");
    std::fs::write(
        root.join("adapters/fake.conf"),
        format!(
            "self = true\nspawn = true\nring = true\nlist = echo %1 %2\n\
             close = touch '{}'\ncapture = echo screen text\n",
            closed.display()
        ),
    )
    .unwrap();
    let child = {
        let (root, session) = (root.clone(), session.clone());
        std::thread::spawn(move || {
            let mut connection = swarm::store::open(&root.join("swarm.db")).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !swarm::store::owes_wrap_up(&connection, &session, CODER).unwrap() {
                assert!(std::time::Instant::now() < deadline, "no wrap-up was sent");
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            report_dead(
                &mut connection,
                &root,
                "fake",
                &session,
                CODER,
                "agent coder died",
            )
            .unwrap();
        })
    };
    let started = std::time::Instant::now();

    close(&mut connection, &root, &session, Some(30));

    child.join().unwrap();
    // The wait for the wrap-up is 30 s.
    assert!(started.elapsed() < std::time::Duration::from_secs(25));
    assert!(
        !root
            .join(format!("runs/{session}/{CODER}.summary.md"))
            .exists()
    );
    assert!(closed.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn close_now_sends_no_wrap_up() {
    let (root, mut connection, session) = closing("close-now");

    close(&mut connection, &root, &session, None);

    assert!(
        swarm::store::messages(&connection, &session, -1)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}
