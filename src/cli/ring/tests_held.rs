//! Tests for a ring held while the recipient waits on a question (ADR 0160).

use super::*;

mod cases {
    use super::*;
    use crate::cli::test_support::*;

    /// The owner's question took the ring's Enter as its answer (the chair's review question, 2026-11-24).
    #[test]
    fn a_ring_waits_for_an_open_question_and_goes_once_it_closes() {
        let (root, mut connection, session) = ring_session(
            "held-ring",
            Some("pi"),
            "ring = printf '%s\\n' \"$SWARM_TEXT\" >> '{screen}.rings'",
            include_str!("../../../tests/fixtures/screens/pi-question-select.txt"),
        );
        let rings = root.join("screen.rings");
        let seq = send_task(&root, &mut connection, &session);

        assert!(!rings.exists(), "a ring typed into the open question");
        assert!(swarm::store::ring_held(&connection, &session, CODER).unwrap());
        assert_eq!(delivery_of(&connection, &session, seq), None);

        std::fs::write(
            root.join("screen"),
            include_str!("../../../tests/fixtures/screens/pi-working.txt"),
        )
        .unwrap();
        let adapter = swarm::adapter::load(&root, "fake").unwrap();
        rering_if_due(
            &mut connection,
            &root,
            &adapter,
            &session,
            CODER,
            "%2",
            Proof::Later,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&rings).unwrap(),
            format!("{}\n", ring_text(&root))
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
