//! Board claim tests: the lease, the generation fence, the attempt cap, and a race.

use super::*;

#[cfg(test)]
mod tests {
    use super::super::board_test_support::*;
    use super::super::tests::*;
    use super::*;

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
        // A later run's tries start again, because the cap is per session.
        post(
            &mut c,
            &root,
            PROJECT,
            OTHER_SESSION,
            OUTSIDER,
            NOW + 400,
            &claim("auth", None),
        )
        .unwrap();
        let row = &claims(&c, PROJECT).unwrap()[0];
        assert_eq!((row.holder.as_str(), row.attempts), (OUTSIDER, 1));
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
    fn a_worker_hands_off_only_after_the_checker_records_a_test() {
        let (mut c, root) = fixture("checker-test");
        n(put(&mut c, &root, CODER, claim("auth", None)));
        refused(
            put(&mut c, &root, CODER, handoff("auth", 1)),
            "no checker test recorded for auth",
        );
        checker_test(&mut c, &root, "other");
        refused(
            put(&mut c, &root, CODER, handoff("auth", 1)),
            "no checker test recorded for auth",
        );
        checker_test(&mut c, &root, "auth");
        n(put(&mut c, &root, CODER, handoff("auth", 1)));
    }
}
