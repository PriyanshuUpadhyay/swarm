//! Started-frame deadline policy (TRIAL-V4), checker-owned. Two tests, no production hooks.
//! 1. `started_frame_finishes_once_then_original_deadline_refuses_next` drives the REAL
//!    `write_frame_at_boundary` through a writer that stalls mid-frame across the work deadline.
//! 2. `late_buffered_commit_is_rejected_by_the_original_observation` drives the REAL Root client
//!    `collect` over a child whose whole payload is already buffered after the observation ended.
//!    It proves the Root client fence only; it says nothing about what the authority sends.
//!
//! Expected bytes below are hand-authored from the fixed wire contract (4-byte big-endian length
//! then the JSON object); nothing is produced by `write_frame` or `payload`.
#![cfg(feature = "serve")]

#[path = "transcript_caller_trial/client.rs"]
pub mod client;
#[path = "transcript_caller_trial/fixtures.rs"]
pub mod fixtures;
#[path = "../examples/transcript_caller_trial/protocol.rs"]
pub mod protocol;

use std::cell::RefCell;
use std::io;
use std::pin::{Pin, pin};
use std::process::Command;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use client::{Entry, Kind, Obs, Session};
use protocol::{Budget, FrameError, Reply, Tag, write_frame_at_boundary};
use serde_json::json;
use swarm::view::transcript::worker_protocol::Binding;
use tokio::io::AsyncWrite;
use tokio::time::{Instant, sleep_until, timeout};

/// Wire bytes of the in-flight frame: length 115, then the Begin object, as literals.
const BEGIN_PREFIX: [u8; 4] = [0, 0, 0, 115];
const BEGIN_BODY: &[u8] = br#"{"op":"begin","tag":{"binding":{"source_id":"u06-deadline-source","generation":1},"request_id":7,"transport_id":3}}"#;
/// The writer accepts the whole prefix and 10 body bytes, then stalls.
const STALL_AFTER: usize = 14;

#[derive(Default)]
struct Shared {
    wire: Vec<u8>,
    released: bool,
    waker: Option<Waker>,
}

/// An `AsyncWrite` that stalls after `STALL_AFTER` bytes until the test releases it.
struct StallingWriter(Rc<RefCell<Shared>>);

impl AsyncWrite for StallingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut s = self.0.borrow_mut();
        let room = if s.released {
            usize::MAX
        } else {
            STALL_AFTER.saturating_sub(s.wire.len())
        };
        if room == 0 {
            s.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        let n = buf.len().min(room);
        s.wire.extend_from_slice(&buf[..n]);
        Poll::Ready(Ok(n))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn tag() -> Tag {
    Tag {
        binding: Binding {
            source_id: "u06-deadline-source".into(),
            generation: 1,
        },
        request_id: 7,
        transport_id: 3,
    }
}

/// A frame starts, the original work deadline passes while bytes remain, the writer resumes and
/// the frame finishes once; the next payload frame is refused at the original boundary.
#[tokio::test]
async fn started_frame_finishes_once_then_original_deadline_refuses_next() {
    let deadline = Instant::now() + Duration::from_millis(100);
    let budget = Budget(Some(deadline));
    let shared = Rc::new(RefCell::new(Shared::default()));
    let mut writer = StallingWriter(shared.clone());
    let begin = Reply::Begin { tag: tag() };
    let part = Reply::Part {
        tag: tag(),
        part_index: 0,
        bytes: vec![1],
    };
    let mut expected = BEGIN_PREFIX.to_vec();
    expected.extend_from_slice(BEGIN_BODY);

    let mut first = pin!(write_frame_at_boundary(&mut writer, &begin, budget));
    // `biased` polls the write first, so a timer that fired at the deadline is seen by the write
    // before this test releases the writer. A now-ready future cannot hide the abort.
    let mut after_deadline = pin!(sleep_until(deadline + Duration::from_millis(50)));
    tokio::select! {
        biased;
        r = &mut first => {
            let wire = shared.borrow().wire.clone();
            // Only a frame that had started (prefix and part of the body written) can show the
            // mid-write abort. Anything else is setup, e.g. the deadline passed under load before
            // the first poll, and must not be read as the behavioral RED.
            if wire != expected[..STALL_AFTER] || !matches!(r, Err(FrameError::Transport("deadline"))) {
                panic!("SETUP, not RED: frame not stalled mid-write as arranged: {r:?}; wire {wire:?}");
            }
            panic!(
                "RED: the frame was already started but ended at the work deadline: {r:?}; wire {:?}",
                String::from_utf8_lossy(&wire)
            )
        }
        () = &mut after_deadline => {}
    }
    assert_eq!(
        shared.borrow().wire,
        expected[..STALL_AFTER],
        "setup: stalled mid-frame"
    );
    {
        let mut s = shared.borrow_mut();
        s.released = true;
        s.waker
            .take()
            .expect("writer was polled and stalled")
            .wake();
    }
    match timeout(Duration::from_millis(300), &mut first).await {
        Ok(Ok(())) => {}
        other => panic!("RED: the started frame did not finish once after the deadline: {other:?}"),
    }
    assert_eq!(
        shared.borrow().wire,
        expected,
        "frame bytes exactly once, no duplicate prefix"
    );

    let mut second_writer = StallingWriter(shared.clone());
    let next = write_frame_at_boundary(&mut second_writer, &part, budget).await;
    assert!(
        matches!(next, Err(FrameError::Boundary("deadline"))),
        "next payload frame must be refused at the original deadline: {next:?}"
    );
    assert_eq!(
        shared.borrow().wire,
        expected,
        "refused frame wrote no byte"
    );
}

/// One complete Live payload, `{}`, as hand-authored frames: Begin, Part [123,125], Commit.
fn payload_frames() -> Vec<u8> {
    let tag = r#"{"binding":{"source_id":"u06-seed-a-source","generation":1},"request_id":1,"transport_id":0}"#;
    let bodies = [
        format!(r#"{{"op":"begin","tag":{tag}}}"#),
        format!(r#"{{"op":"part","tag":{tag},"part_index":0,"bytes":[123,125]}}"#),
        format!(r#"{{"op":"commit","tag":{tag},"part_count":1,"payload_bytes":2}}"#),
    ];
    let lengths = [113u8, 145, 147];
    let mut out = Vec::new();
    for (body, len) in bodies.iter().zip(lengths) {
        assert_eq!(body.len(), usize::from(len), "authored length of {body}");
        out.extend_from_slice(&[0, 0, 0, len]);
        out.extend_from_slice(body.as_bytes());
    }
    out
}

/// A child that has already written the whole payload and exited, so every chunk is queued.
/// Spawn and wait share the one `obs` given; nothing here renews a budget.
fn buffered_session(tree: &fixtures::Tree, obs: &Obs) -> Session {
    let file = tree.root.join("payload.frames");
    std::fs::write(&file, payload_frames()).expect("frames file");
    let mut cmd = Command::new("/bin/cat");
    cmd.arg(&file);
    // `spawn` gives the stand-in the same isolated environment as the real entry point.
    let mut s = Session::spawn(Entry::Trial, cmd, &tree.root, obs).expect("stand-in child");
    s.wait_for_stand_in(obs)
        .expect("stand-in finished with output queued");
    s
}

/// The same buffered payload is accepted inside its observation and rejected after the original
/// deadline, although `recv_timeout(0)` would still hand every frame back.
#[test]
fn late_buffered_commit_is_rejected_by_the_original_observation() {
    let tree = fixtures::Tree::create();
    // Control: spawn, wait and collect share ONE original observation.
    let obs = Obs::new();
    let live = buffered_session(&tree, &obs).collect(1, Kind::Live, &obs);
    assert_eq!(live.expect("control: inside the observation"), json!({}));

    // The stand-in is prepared under its own bounded fixture observation. The result observation
    // is created before collection and is already spent: no budget is renewed for the check.
    let mut session = buffered_session(&tree, &Obs::new());
    let result_obs = Obs::with_budget(Duration::ZERO);
    let late = session.collect(1, Kind::Live, &result_obs);
    let err = late.expect_err("RED: a Commit buffered past the original observation was accepted");
    assert!(
        err.contains("late result rejected"),
        "rejected for the wrong reason: {err}"
    );
}

/// Owner gate (TRIAL-V5): Fault, Fenced, Cancelled and Closed keep their original writer deadline.
/// Each value starts its frame, stalls mid-frame across its finite deadline and must fail at that
/// deadline with the writer's own error. A started-frame allowance on any of them would still be
/// pending after the deadline. Expected bytes are literals: 4-byte big-endian length, then JSON.
#[tokio::test]
async fn cancellation_and_cleanup_replies_keep_their_original_writer_deadline() {
    let binding = || Binding {
        source_id: "u06-deadline-source".into(),
        generation: 1,
    };
    let cases: [(&str, Reply, u8, &[u8]); 4] = [
        (
            "fault",
            Reply::Fault {
                request_id: Some(7),
                binding: Some(binding()),
                code: "deadline".into(),
            },
            108,
            br#"{"op":"fault","request_id":7,"binding":{"source_id":"u06-deadline-source","generation":1},"code":"deadline"}"#,
        ),
        (
            "fenced",
            Reply::Fenced { tag: tag() },
            116,
            br#"{"op":"fenced","tag":{"binding":{"source_id":"u06-deadline-source","generation":1},"request_id":7,"transport_id":3}}"#,
        ),
        (
            "cancelled",
            Reply::Cancelled {
                request_id: 8,
                binding: binding(),
                target_request_id: 7,
                transport_id: 3,
                exit: protocol::Exit::Code { code: 0 },
            },
            165,
            br#"{"op":"cancelled","request_id":8,"binding":{"source_id":"u06-deadline-source","generation":1},"target_request_id":7,"transport_id":3,"exit":{"kind":"code","code":0}}"#,
        ),
        (
            "closed",
            Reply::Closed {
                request_id: 8,
                binding: binding(),
            },
            91,
            br#"{"op":"closed","request_id":8,"binding":{"source_id":"u06-deadline-source","generation":1}}"#,
        ),
    ];
    for (name, reply, length, body) in cases {
        let deadline = Instant::now() + Duration::from_millis(40);
        let shared = Rc::new(RefCell::new(Shared::default()));
        let mut writer = StallingWriter(shared.clone());
        let mut expected = vec![0, 0, 0, length];
        expected.extend_from_slice(body);

        let mut write = pin!(write_frame_at_boundary(
            &mut writer,
            &reply,
            Budget(Some(deadline))
        ));
        // `biased` polls the write first, so a timer that fired at the deadline is seen by the
        // write before this guard looks at the later timer.
        let mut after_deadline = pin!(sleep_until(deadline + Duration::from_millis(50)));
        tokio::select! {
            biased;
            r = &mut write => {
                let wire = shared.borrow().wire.clone();
                assert_eq!(
                    wire,
                    expected[..STALL_AFTER],
                    "{name}: SETUP, not RED: frame not stalled mid-write as arranged: {r:?}"
                );
                assert!(
                    matches!(r, Err(FrameError::Transport("deadline"))),
                    "{name}: the started frame must fail at its original deadline: {r:?}"
                );
            }
            () = &mut after_deadline => panic!(
                "RED: {name} was still writing after its original deadline; wire {:?}",
                String::from_utf8_lossy(&shared.borrow().wire)
            ),
        }
    }
}
