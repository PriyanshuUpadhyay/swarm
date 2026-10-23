//! Project board tests: one per refusal, the append-only triggers, and the claim fence.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    const PROJECT: &str = "repository:/hub/.bare";
    const NOW: i64 = 1_000_000;
    const SRC: &str = "src/a.rs@3f2a1c9";

    fn fixture(tag: &str) -> (Connection, std::path::PathBuf) {
        (seed(), temp_root(&format!("board-{tag}")))
    }

    fn put(
        connection: &mut Connection,
        root: &Path,
        author: &str,
        new: NewPost,
    ) -> Result<Posted, String> {
        post(connection, root, PROJECT, SESSION, author, NOW, &new).map_err(|e| e.to_string())
    }

    fn n(posted: Result<Posted, String>) -> i64 {
        match posted.unwrap() {
            Posted::New(n) | Posted::Again(n) => n,
            other => panic!("not a post: {other:?}"),
        }
    }

    fn finding(subject: &str) -> NewPost<'_> {
        NewPost {
            kind: "finding",
            subject,
            sources: vec![SRC.into()],
            ..Default::default()
        }
    }

    fn refused(result: Result<Posted, String>, start: &str) {
        let error = result.unwrap_err();
        assert!(error.starts_with(&format!("swarm: {start}")), "{error}");
    }

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
        assert_eq!(first, Posted::New(1));
        assert_eq!(again, Posted::Again(1));
        assert_eq!(posts(&c, PROJECT).unwrap().len(), 1);
    }

    #[test]
    fn a_subject_is_one_short_line() {
        let (mut c, root) = fixture("subject");
        for bad in ["", "two\nlines", &"x".repeat(201)] {
            refused(put(&mut c, &root, CODER, finding(bad)), "needs --subject");
        }
    }

    fn claim(work: &str, generation: Option<i64>) -> NewPost<'_> {
        NewPost {
            kind: "claim",
            work_key: Some(work),
            lease_secs: Some(60),
            generation,
            subject: "mine",
            ..Default::default()
        }
    }

    fn handoff(work: &str, generation: i64) -> NewPost<'_> {
        NewPost {
            kind: "handoff",
            work_key: Some(work),
            generation: Some(generation),
            subject: "done",
            ..Default::default()
        }
    }

    #[test]
    fn a_live_claim_refuses_another_holder_and_a_renewal_adds_no_attempt() {
        let (mut c, root) = fixture("claim");
        n(put(&mut c, &root, CODER, claim("auth", None)));
        refused(
            put(&mut c, &root, ORCHESTRATOR, claim("auth", None)),
            "held by coder for 60 s more, generation 1",
        );
        refused(
            put(&mut c, &root, CODER, claim("auth", None)),
            "you hold auth, generation 1",
        );
        assert_eq!(
            put(&mut c, &root, CODER, claim("auth", Some(1))).unwrap(),
            Posted::Renewed {
                generation: 1,
                lease_until: NOW + 60
            }
        );
        refused(
            put(&mut c, &root, CODER, claim("auth", Some(0))),
            "stale generation 0; current is 1",
        );
        let row = &claims(&c, PROJECT).unwrap()[0];
        assert_eq!((row.generation, row.attempts), (1, 1));
        assert_eq!(posts(&c, PROJECT).unwrap().len(), 1);
    }

    #[test]
    fn an_expired_lease_passes_with_a_new_generation_and_fences_the_old_holder() {
        let (mut c, root) = fixture("fence");
        n(put(&mut c, &root, CODER, claim("auth", None)));
        let later = NOW + 61;
        post(
            &mut c,
            &root,
            PROJECT,
            SESSION,
            ORCHESTRATOR,
            later,
            &claim("auth", None),
        )
        .unwrap();
        let row = &claims(&c, PROJECT).unwrap()[0];
        assert_eq!(
            (row.holder.as_str(), row.generation, row.attempts),
            (ORCHESTRATOR, 2, 2)
        );
        let old = post(
            &mut c,
            &root,
            PROJECT,
            SESSION,
            CODER,
            later,
            &handoff("auth", 1),
        )
        .unwrap_err();
        assert_eq!(old.to_string(), "swarm: stale generation 1; current is 2");
        let wrong = post(
            &mut c,
            &root,
            PROJECT,
            SESSION,
            CODER,
            later,
            &handoff("auth", 2),
        )
        .unwrap_err();
        assert_eq!(wrong.to_string(), "swarm: you do not hold auth");
        post(
            &mut c,
            &root,
            PROJECT,
            SESSION,
            ORCHESTRATOR,
            later,
            &handoff("auth", 2),
        )
        .unwrap();
        assert!(claims(&c, PROJECT).unwrap()[0].released_at.is_some());
    }

    #[test]
    fn a_fourth_holder_parks_the_work_for_the_owner() {
        let (mut c, root) = fixture("attempts");
        for (i, who) in [CODER, ORCHESTRATOR, CODER].into_iter().enumerate() {
            let at = NOW + 100 * i as i64;
            post(
                &mut c,
                &root,
                PROJECT,
                SESSION,
                who,
                at,
                &claim("auth", None),
            )
            .unwrap();
        }
        let fourth = post(
            &mut c,
            &root,
            PROJECT,
            SESSION,
            ORCHESTRATOR,
            NOW + 300,
            &claim("auth", None),
        )
        .unwrap_err();
        assert_eq!(
            fourth.to_string(),
            "swarm: 3 attempts used on auth; parked for owner"
        );
    }

    #[test]
    fn two_racing_claims_give_one_holder() {
        let root = temp_root("board-race");
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("swarm.db");
        open(&db).unwrap().execute_batch(&format!(
            "INSERT INTO session (id, talk_mode, cwd) VALUES ('{SESSION}', 'lane', '/test');
             INSERT INTO agent (id, session_id, role) VALUES ('{ORCHESTRATOR}', '{SESSION}', 'orchestrator'), ('{CODER}', '{SESSION}', 'coder');"
        )).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let racers: Vec<_> = [CODER, ORCHESTRATOR]
            .into_iter()
            .map(|who| {
                let (db, root, barrier) = (db.clone(), root.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let mut c = open(&db).unwrap();
                    barrier.wait();
                    post(
                        &mut c,
                        &root,
                        PROJECT,
                        SESSION,
                        who,
                        NOW,
                        &claim("auth", None),
                    )
                    .map_err(|e| e.to_string())
                })
            })
            .collect();
        let results: Vec<_> = racers.into_iter().map(|r| r.join().unwrap()).collect();
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "{results:?}"
        );
        assert!(
            results
                .iter()
                .any(|r| r.as_ref().is_err_and(|e| e.contains("held by"))),
            "{results:?}"
        );
    }

    #[test]
    fn a_handoff_needs_a_claim() {
        let (mut c, root) = fixture("handoff");
        refused(
            put(&mut c, &root, CODER, handoff("auth", 1)),
            "nobody claimed auth",
        );
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
}
