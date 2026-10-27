//! Board post tests: one per refusal, the append-only triggers, and relink.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::board_test_support::*;
    use super::super::tests::*;
    use super::*;

    #[test]
    fn a_finding_is_stored_with_its_body_and_role() {
        let (mut c, root) = fixture("store");
        let b1 = n(put(
            &mut c,
            &root,
            CODER,
            NewPost {
                body: "the body",
                ..finding("lock order")
            },
        ));
        assert_eq!(b1, 1);
        let all = posts(&c, PROJECT).unwrap();
        assert_eq!(all[0].author_role, "coder");
        assert_eq!(all[0].sources, vec![SRC.to_string()]);
        assert_eq!(
            std::fs::read_to_string(root.join(&all[0].body_path)).unwrap(),
            "the body"
        );
        assert!(posts(&c, "repository:/other/.git").unwrap().is_empty());
    }

    #[test]
    fn has_posted_names_only_the_author_in_its_session() {
        let (mut c, root) = fixture("posted");
        assert!(!has_posted(&c, SESSION, CODER).unwrap());
        n(put(&mut c, &root, CODER, finding("f")));
        assert!(has_posted(&c, SESSION, CODER).unwrap());
        assert!(!has_posted(&c, SESSION, ORCHESTRATOR).unwrap());
        assert!(!has_posted(&c, OTHER_SESSION, CODER).unwrap());
    }

    #[test]
    fn has_acked_names_only_the_reader_in_its_session() {
        let (c, _root) = fixture("acked");
        assert!(!has_acked(&c, SESSION, CODER).unwrap());
        c.execute(
            "INSERT INTO message (session_id, seq, sender_id, recipient_id, kind, body_path)
             VALUES (?1, 0, ?2, ?3, 'ask', 'runs/0.txt')",
            (SESSION, ORCHESTRATOR, CODER),
        )
        .unwrap();
        c.execute(
            "INSERT INTO read_mark (session_id, message_seq, agent_id) VALUES (?1, 0, ?2)",
            (SESSION, CODER),
        )
        .unwrap();
        assert!(has_acked(&c, SESSION, CODER).unwrap());
        assert!(!has_acked(&c, SESSION, ORCHESTRATOR).unwrap());
        assert!(!has_acked(&c, OTHER_SESSION, CODER).unwrap());
    }

    #[test]
    fn posts_are_append_only() {
        let (mut c, root) = fixture("append");
        n(put(&mut c, &root, CODER, finding("f")));
        let update = c
            .execute("UPDATE post SET subject = 'x'", [])
            .unwrap_err()
            .to_string();
        assert!(update.contains("append-only"), "{update}");
        let delete = c.execute("DELETE FROM post", []).unwrap_err().to_string();
        assert!(delete.contains("append-only"), "{delete}");
    }

    #[test]
    fn a_finding_decision_or_answer_needs_a_valid_source() {
        let (mut c, root) = fixture("source");
        for kind in ["finding", "decision"] {
            refused(
                put(
                    &mut c,
                    &root,
                    ORCHESTRATOR,
                    NewPost {
                        kind,
                        subject: "s",
                        ..Default::default()
                    },
                ),
                "needs --source",
            );
        }
        let ask = n(put(
            &mut c,
            &root,
            ORCHESTRATOR,
            NewPost {
                kind: "ask",
                subject: "q",
                ..Default::default()
            },
        ));
        refused(
            put(
                &mut c,
                &root,
                CODER,
                NewPost {
                    kind: "answer",
                    reply_to: Some(ask),
                    subject: "a",
                    ..Default::default()
                },
            ),
            "needs --source",
        );
        for bad in [
            "src/a.rs",
            "src/a.rs@xyz1234",
            "has space@3f2a1c9",
            "https://",
        ] {
            refused(
                put(
                    &mut c,
                    &root,
                    CODER,
                    NewPost {
                        sources: vec![bad.into()],
                        ..finding("f")
                    },
                ),
                "needs --source",
            );
        }
        n(put(
            &mut c,
            &root,
            CODER,
            NewPost {
                sources: vec!["https://example.com/x".into()],
                ..finding("f")
            },
        ));
    }

    #[test]
    fn a_superseded_post_cannot_be_superseded_again() {
        let (mut c, root) = fixture("supersede");
        let b1 = n(put(&mut c, &root, CODER, finding("v1")));
        let b2 = n(put(
            &mut c,
            &root,
            CODER,
            NewPost {
                supersedes: Some(b1),
                ..finding("v2")
            },
        ));
        refused(
            put(
                &mut c,
                &root,
                CODER,
                NewPost {
                    supersedes: Some(b1),
                    ..finding("v3")
                },
            ),
            &format!("B-{b1} is superseded by B-{b2}"),
        );
        let all = posts(&c, PROJECT).unwrap();
        assert_eq!(all[0].superseded_by, Some(b2));
        let ask = n(put(
            &mut c,
            &root,
            ORCHESTRATOR,
            NewPost {
                kind: "ask",
                subject: "q",
                ..Default::default()
            },
        ));
        refused(
            put(
                &mut c,
                &root,
                CODER,
                NewPost {
                    supersedes: Some(ask),
                    ..finding("x")
                },
            ),
            &format!("B-{ask} is a ask"),
        );
    }

    #[test]
    fn an_answer_replies_only_to_an_ask() {
        let (mut c, root) = fixture("answer");
        let b1 = n(put(&mut c, &root, CODER, finding("f")));
        refused(
            put(
                &mut c,
                &root,
                CODER,
                NewPost {
                    kind: "answer",
                    reply_to: Some(b1),
                    sources: vec![SRC.into()],
                    subject: "a",
                    ..Default::default()
                },
            ),
            &format!("B-{b1} is a finding, not an ask"),
        );
        refused(
            put(
                &mut c,
                &root,
                CODER,
                NewPost {
                    kind: "answer",
                    reply_to: Some(99),
                    sources: vec![SRC.into()],
                    subject: "a",
                    ..Default::default()
                },
            ),
            "no post B-99",
        );
    }

    #[test]
    fn only_the_lead_asks_everyone_decides_or_relays() {
        let (mut c, root) = fixture("lead");
        refused(
            put(
                &mut c,
                &root,
                CODER,
                NewPost {
                    kind: "ask",
                    subject: "q",
                    ..Default::default()
                },
            ),
            "only the lead asks everyone",
        );
        n(put(
            &mut c,
            &root,
            CODER,
            NewPost {
                kind: "ask",
                subject: "q",
                to: Some((SESSION, ORCHESTRATOR)),
                ..Default::default()
            },
        ));
        n(put(
            &mut c,
            &root,
            ORCHESTRATOR,
            NewPost {
                kind: "ask",
                subject: "q",
                ..Default::default()
            },
        ));
        let decision = |needs_owner, relayed| NewPost {
            kind: "decision",
            subject: "d",
            sources: vec![SRC.into()],
            needs_owner,
            relayed,
            ..Default::default()
        };
        refused(
            put(&mut c, &root, CODER, decision(false, false)),
            "ask the lead, or add --needs-owner",
        );
        n(put(&mut c, &root, CODER, decision(true, false)));
        refused(
            put(&mut c, &root, CODER, decision(true, true)),
            "only the lead relays the owner",
        );
        n(put(&mut c, &root, ORCHESTRATOR, decision(false, true)));
        refused(
            put(
                &mut c,
                &root,
                ORCHESTRATOR,
                NewPost {
                    relayed: true,
                    ..finding("f")
                },
            ),
            "--relayed fits only",
        );
    }

    #[test]
    fn an_ask_chain_counts_its_depth() {
        let (mut c, root) = fixture("depth");
        let ask = |reply_to| NewPost {
            kind: "ask",
            subject: "q",
            reply_to,
            ..Default::default()
        };
        let mut parent = n(put(&mut c, &root, ORCHESTRATOR, ask(None)));
        for _ in 0..3 {
            parent = n(put(&mut c, &root, ORCHESTRATOR, ask(Some(parent))));
        }
        let answer = NewPost {
            kind: "answer",
            reply_to: Some(parent),
            sources: vec![SRC.into()],
            subject: "a",
            ..Default::default()
        };
        n(put(&mut c, &root, CODER, answer));
        let depths: Vec<i64> = posts(&c, PROJECT)
            .unwrap()
            .iter()
            .map(|p| p.depth)
            .collect();
        assert_eq!(depths, [0, 1, 2, 3, 3]);
    }

    #[test]
    fn a_repeated_idem_key_prints_the_first_post_again() {
        let (mut c, root) = fixture("idem");
        let first = put(
            &mut c,
            &root,
            CODER,
            NewPost {
                idem: Some("k1"),
                ..finding("f")
            },
        )
        .unwrap();
        let again = put(
            &mut c,
            &root,
            CODER,
            NewPost {
                idem: Some("k1"),
                ..finding("other")
            },
        )
        .unwrap();
        assert!(matches!(first, Posted::New(1, _)));
        assert_eq!(again, Posted::Again(1));
        refused(
            put(
                &mut c,
                &root,
                ORCHESTRATOR,
                NewPost {
                    idem: Some("k1"),
                    ..finding("f")
                },
            ),
            "--idem k1 is used by B-1",
        );
        assert_eq!(posts(&c, PROJECT).unwrap().len(), 1);
    }

    #[test]
    fn a_subject_is_one_short_line() {
        let (mut c, root) = fixture("subject");
        for bad in ["", "two\nlines", &"x".repeat(201)] {
            refused(put(&mut c, &root, CODER, finding(bad)), "needs --subject");
        }
    }

    #[test]
    fn relink_copies_into_an_empty_board_only() {
        let (mut c, root) = fixture("relink");
        let b1 = n(put(&mut c, &root, CODER, finding("v1")));
        n(put(
            &mut c,
            &root,
            CODER,
            NewPost {
                supersedes: Some(b1),
                ..finding("v2")
            },
        ));
        n(put(&mut c, &root, CODER, claim("auth", None)));
        let moved = "repository:/moved/.bare";
        assert_eq!(relink(&mut c, PROJECT, moved).unwrap(), 3);
        let copy = posts(&c, moved).unwrap();
        assert_eq!(copy[0].superseded_by, Some(2));
        assert_eq!(claims(&c, moved).unwrap().len(), 1);
        assert_eq!(posts(&c, PROJECT).unwrap().len(), 3);
        let again = relink(&mut c, PROJECT, moved).unwrap_err().to_string();
        assert!(again.contains("already has posts"), "{again}");
        let none = relink(&mut c, "folder:/nothing", "folder:/x")
            .unwrap_err()
            .to_string();
        assert!(none.contains("no board at"), "{none}");
    }

    #[test]
    fn a_worker_reads_no_checker_post_but_one_sent_to_it() {
        let (mut c, root) = fixture("withheld");
        let test = checker_test(&mut c, &root, "auth");
        let ask = n(put(
            &mut c,
            &root,
            CODER,
            NewPost {
                kind: "ask",
                to: Some((SESSION, CHECKER)),
                subject: "q",
                ..Default::default()
            },
        ));
        let answer = n(put(
            &mut c,
            &root,
            CHECKER,
            NewPost {
                kind: "answer",
                reply_to: Some(ask),
                sources: vec![SRC.into()],
                subject: "a",
                ..Default::default()
            },
        ));
        let sent = n(put(
            &mut c,
            &root,
            CHECKER,
            NewPost {
                kind: "ask",
                to: Some((SESSION, CODER)),
                subject: "h",
                ..Default::default()
            },
        ));
        let to_lead = n(put(
            &mut c,
            &root,
            CHECKER,
            NewPost {
                kind: "ask",
                to: Some((SESSION, ORCHESTRATOR)),
                subject: "x",
                ..Default::default()
            },
        ));
        let seen = |reader| -> Vec<i64> {
            posts_for(&c, PROJECT, reader)
                .unwrap()
                .iter()
                .map(|p| p.n)
                .collect()
        };
        assert_eq!(seen(Some((SESSION, CODER))), [ask, answer, sent]);
        let all = vec![test, ask, answer, sent, to_lead];
        assert_eq!(seen(Some((SESSION, ORCHESTRATOR))), all);
        assert_eq!(seen(Some((SESSION, CHECKER))), all);
        assert_eq!(seen(None), all);
        // The wall is per task: a worker in a later session reads an earlier checker's findings.
        assert_eq!(seen(Some((OTHER_SESSION, OUTSIDER))), all);
        // A checker whose pane is still open may be checking a task that runs now.
        set_pane(&c, SESSION, CHECKER, "%9").unwrap();
        assert_eq!(seen(Some((OTHER_SESSION, OUTSIDER))), [ask]);
        assert_eq!(seen(Some((SESSION, ORCHESTRATOR))), all);
        clear_pane(&c, SESSION, CHECKER).unwrap();
        // A checker reads only named posts, so it may not list the board; the others may.
        let err = may_list(&c, Some((SESSION, CHECKER)))
            .unwrap_err()
            .to_string();
        assert!(err.contains("swarm board read B-n"), "{err}");
        for reader in [Some((SESSION, CODER)), Some((SESSION, ORCHESTRATOR)), None] {
            assert!(may_list(&c, reader).is_ok());
        }
    }
}
