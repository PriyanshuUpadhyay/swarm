//! Caller isolation trial, checker-owned tests (parent contract TRIAL-V1 + TRIAL-V2).
//! Two tests through a real child CLI entry point. Expected values are the frozen OLD native
//! captures in `fixtures`; nothing is computed from the code under test.
//! `SWARM_CALLER_ENTRY=old-worker` drives the pre-trial `swarm _transcript-worker` (baseline);
//! the default drives the built example. Run with `--nocapture` to see the `reached:` markers.
#![cfg(feature = "serve")]

#[path = "transcript_caller_trial/client.rs"]
pub mod client;
#[path = "transcript_caller_trial/fixtures.rs"]
pub mod fixtures;

use client::{Entry, Obs, Session};
use fixtures::*;

fn must<T>(what: &str, r: Result<T, String>) -> T {
    match r {
        Ok(v) => {
            println!("reached: {what}");
            v
        }
        Err(e) => panic!("PRE-CANCEL/SETUP step failed ({what}): {e}"),
    }
}

/// Initial complete native message, retained partial offsets, saved read A, no-growth advance,
/// rename plus replacement, saved read B. Everything here happens before any cancellation.
fn establish(entry: Entry, tree: &Tree) -> Session {
    // Startup and hello share one observation.
    let obs = Obs::new();
    let mut s = must("start", Session::start(entry, &tree.root, &obs));
    must("hello", s.hello(&obs));
    must("open", s.open(&tree.original()));
    let bound = must("bind at zero", s.bind(reader()));
    assert_eq!(
        bound,
        update(READ_END, END1, vec![line1_entry()]),
        "initial complete native message"
    );
    let advanced = must("advance", s.advance());
    assert_eq!(
        advanced,
        update(READ_END, END1, vec![]),
        "partial line retained, no replay"
    );
    let a = must("saved read A", s.read(END1));
    assert_eq!(
        a,
        snapshot(END1, vec![line1_entry()]),
        "saved read A is native and complete"
    );
    let again = must("no-growth advance", s.advance());
    assert_eq!(again, advanced, "saved read does not consume the partial");
    tree.rename_and_replace();
    let b = must("saved read B after replace", s.read(END1));
    assert_eq!(
        b,
        snapshot(END1, vec![line1_entry()]),
        "old inode text, not the replacement"
    );
    s
}

/// The completed-line continuation: only the old partial line, no replay, no replacement text.
fn continuation(s: &mut Session, tree: &Tree) {
    tree.complete_partial();
    let update_now = s
        .advance()
        .unwrap_or_else(|e| panic!("R2 COMPLETED-LINE CONTINUATION failed: {e}"));
    assert_eq!(
        update_now,
        update(END2, END2, vec![line2_entry()]),
        "completed old partial line only"
    );
    let all = must("further saved read", s.read(END2));
    assert_eq!(
        all,
        snapshot(END2, vec![line1_entry(), line2_entry()]),
        "authority still usable"
    );
}

#[test]
fn main_path() {
    let entry = Entry::from_env();
    let tree = Tree::create();
    let mut s = establish(entry, &tree);
    continuation(&mut s, &tree);
    must(
        "close and authority exit 0 with both EOFs",
        s.close(None, &Obs::new()),
    );
}

#[test]
fn cancel_guard() {
    let entry = Entry::from_env();
    let tree = Tree::create();
    let mut s = establish(entry, &tree);
    if entry == Entry::OldWorker {
        // Baseline: the only cancel OLD has stops the one worker, so its retained state is lost.
        s.kill_and_reap()
            .unwrap_or_else(|e| panic!("CLEANUP INCOMPLETE, not a RED: {e}"));
        continuation(&mut s, &tree);
        return;
    }
    let code0 = serde_json::json!({"kind": "code", "code": 0});
    let signal = serde_json::json!({"kind": "signal"});

    // C1: late Commit is consumed and discarded; the executor exits 0 without a kill.
    let c1 = must("C1 staged", s.start_hold("exit", &tree.gate("g1")));
    must("C1 cancel", s.cancel(&c1, code0, &Obs::new()));

    // C2: live work is unblocked while staged; ps shows the executor alive, then gone.
    let c2 = must("C2 staged", s.start_hold("wait_for_kill", &tree.gate("g2")));
    let held = must("no-growth advance while staged", s.advance());
    assert_eq!(
        held,
        update(READ_END, END1, vec![]),
        "live state unchanged while a saved job is staged"
    );
    must("ps positive control", s.ps(c2.pid, true, &Obs::new()));
    // One cancellation observation covers cancel, late Commit, Fenced, exit, drains and ps absence.
    let cancel_obs = Obs::new();
    must("C2 cancel", s.cancel(&c2, signal, &cancel_obs));
    must("ps absence after C2", s.ps(c2.pid, false, &cancel_obs));
    continuation(&mut s, &tree);

    // Close while a further job is staged.
    let c3 = must("C3 staged", s.start_hold("wait_for_kill", &tree.gate("g3")));
    must(
        "ps positive control before close",
        s.ps(c3.pid, true, &Obs::new()),
    );
    // One Close observation covers Closed, stdin drop, exit, EOFs and ps absence.
    let close_obs = Obs::new();
    must("close with staged job", s.close(Some(&c3), &close_obs));
    must("ps absence after close", s.ps(c3.pid, false, &close_obs));
}
