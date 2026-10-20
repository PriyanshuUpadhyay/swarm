// Link ownership, state, and undo tests.

    #[test]
    fn a_link_recreated_by_the_owner_after_undo_is_taken() {
        let fixture = LinkFixture::new("owner-recreated-after-undo");
        let other_links: Vec<_> = [".claude/skills", ".agents/skills", ".gemini/config/skills"]
            .into_iter()
            .flat_map(|root| ["council", "flow", "pair"].map(move |skill| (root, skill)))
            .skip(1)
            .map(|(root, skill)| fixture.root.join("home").join(root).join(skill))
            .collect();
        let mut plans = vec![fixture.plan()];
        plans.extend(other_links.iter().map(|path| {
            LinkPlan::create(&fixture.store, path.clone(), fixture.target.clone())
                .unwrap()
                .into()
        }));
        apply(&fixture.store, &plans).unwrap();
        let owner = Target::Ids(vec![fixture.edit().id()]);
        revert(
            &fixture.store,
            &revert_plan(&fixture.store, &[], &owner).unwrap(),
        )
        .unwrap();
        // A repeated undo of an absent off row still changes no file.
        assert!(
            revert(
                &fixture.store,
                &revert_plan(&fixture.store, &[], &owner).unwrap()
            )
            .unwrap()
            .is_empty()
        );
        std::os::unix::fs::symlink(&fixture.target, &fixture.link).unwrap();
        let plan = fixture.plan();
        assert_eq!(plan.conflicts()[0].kind, ConflictKind::Taken);
        assert!(apply(&fixture.store, &[plan]).is_err());
        assert_eq!(std::fs::read_link(&fixture.link).unwrap(), fixture.target);
        let entries = list(&fixture.store, &[]).unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.edit.id() == fixture.edit().id())
            .unwrap();
        assert_eq!(entry.state, State::Changed(fixture.edit().wrote));
        let plans = revert_plan(&fixture.store, &[], &Target::All).unwrap();
        assert_eq!(plans.len(), other_links.len());
        let removed = revert(&fixture.store, &plans).unwrap();
        assert_eq!(removed.len(), other_links.len());
        assert!(
            other_links
                .iter()
                .all(|path| std::fs::symlink_metadata(path).is_err())
        );
        assert_eq!(std::fs::read_link(&fixture.link).unwrap(), fixture.target);
    }

    #[test]
    fn a_catalog_target_change_replaces_only_the_recorded_live_link() {
        let fixture = LinkFixture::new("catalog-target-change");
        let original = fixture.edit();
        apply(&fixture.store, &[fixture.plan()]).unwrap();
        let target = fixture.root.join("swarm/new-catalog-skill");
        std::fs::create_dir(&target).unwrap();
        let plan: Plan = LinkPlan::create(&fixture.store, fixture.link.clone(), target.clone())
            .unwrap()
            .into();
        assert!(plan.conflicts().is_empty());
        assert!(plan.changes());
        assert_eq!(plan.edits()[0].id(), original.id());
        assert_eq!(
            state(&plan.edits()[0], false),
            State::Changed(original.wrote.clone())
        );
        let proof = digest(std::slice::from_ref(&plan));
        assert_eq!(
            proof,
            digest(&[
                LinkPlan::create(&fixture.store, fixture.link.clone(), target.clone())
                    .unwrap()
                    .into()
            ])
        );
        assert_eq!(
            apply(&fixture.store, &[plan]).unwrap(),
            vec![fixture.link.clone()]
        );
        assert_eq!(std::fs::read_link(&fixture.link).unwrap(), target);
        let rows = list(&fixture.store, &[]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].edit.id(), original.id());
        assert_eq!(rows[0].state, State::Present);
        assert_eq!(rows[0].edit.wrote.as_str(), target.to_str());
        revert(
            &fixture.store,
            &revert_plan(&fixture.store, &[], &Target::All).unwrap(),
        )
        .unwrap();
        assert!(fixture.target.is_dir() && target.is_dir());
    }

    #[test]
    fn a_catalog_target_change_refuses_a_changed_or_replaced_live_link() {
        for role in ["owner-changed-target", "changed-after-catalog-plan"] {
            let fixture = LinkFixture::new(role);
            apply(&fixture.store, &[fixture.plan()]).unwrap();
            let target = fixture.root.join("swarm/new-catalog-skill");
            let plan: Plan = LinkPlan::create(&fixture.store, fixture.link.clone(), target.clone())
                .unwrap()
                .into();
            std::fs::remove_file(&fixture.link).unwrap();
            let owner = fixture.root.join("owner-target");
            std::os::unix::fs::symlink(&owner, &fixture.link).unwrap();
            if role == "owner-changed-target" {
                let changed =
                    LinkPlan::create(&fixture.store, fixture.link.clone(), target).unwrap();
                assert_eq!(changed.conflicts[0].kind, ConflictKind::Changed);
            } else {
                assert!(apply(&fixture.store, &[plan]).is_err());
            }
            assert_eq!(std::fs::read_link(&fixture.link).unwrap(), owner);
        }
    }

    #[test]
    fn a_recorded_link_uses_leaf_state_and_undo_keeps_the_target() {
        let fixture = LinkFixture::new("states-and-undo");
        let edit = fixture.edit();
        assert_eq!(state(&edit, false), State::Gone);
        assert_eq!(state(&edit, true), State::Off);
        let plan = fixture.plan();
        assert!(plan.conflicts().is_empty());
        assert_eq!(
            apply(&fixture.store, &[plan]).unwrap(),
            vec![fixture.link.clone()]
        );
        assert_eq!(std::fs::read_link(&fixture.link).unwrap(), fixture.target);
        assert_eq!(state(&edit, false), State::Present);
        let rows = list(&fixture.store, &[]).unwrap();
        assert!(rows[0].recorded);
        assert_eq!(rows[0].json()["wrote"], fixture.target.to_str().unwrap());
        assert!(apply(&fixture.store, &[fixture.plan()]).unwrap().is_empty());
        // A missing target does not change ownership of the exact recorded link.
        std::fs::remove_dir(&fixture.target).unwrap();
        assert_eq!(state(&edit, false), State::Present);
        std::fs::create_dir(&fixture.target).unwrap();
        let target_file = fixture.target.join("SKILL.md");
        std::fs::write(&target_file, "owner bytes").unwrap();
        let plans = revert_plan(&fixture.store, &[], &Target::Ids(vec![edit.id()])).unwrap();
        revert(&fixture.store, &plans).unwrap();
        assert!(std::fs::symlink_metadata(&fixture.link).is_err());
        assert_eq!(std::fs::read_to_string(target_file).unwrap(), "owner bytes");
        assert!(fixture.link.parent().unwrap().is_dir());
        assert_eq!(list(&fixture.store, &[]).unwrap()[0].state, State::Off);
        apply(&fixture.store, &[fixture.plan()]).unwrap();
        std::fs::remove_file(&fixture.link).unwrap();
        assert_eq!(list(&fixture.store, &[]).unwrap()[0].state, State::Gone);
        revert(
            &fixture.store,
            &revert_plan(&fixture.store, &[], &Target::All).unwrap(),
        )
        .unwrap();
        let plans = revert_plan(&fixture.store, &[], &Target::Ids(vec![edit.id()])).unwrap();
        revert(&fixture.store, &plans).unwrap();
        assert_eq!(list(&fixture.store, &[]).unwrap()[0].state, State::Off);
    }

    #[test]
    fn foreign_leaf_types_and_links_are_taken_even_with_the_same_target() {
        for role in [
            "foreign-file",
            "foreign-dir",
            "identical-link",
            "broken-link",
            "foreign-link",
        ] {
            let fixture = LinkFixture::new(role);
            match role {
                "foreign-file" => std::fs::write(&fixture.link, "owner bytes").unwrap(),
                "foreign-dir" => std::fs::create_dir(&fixture.link).unwrap(),
                "identical-link" => {
                    std::os::unix::fs::symlink(&fixture.target, &fixture.link).unwrap()
                }
                _ => std::os::unix::fs::symlink(fixture.root.join(role), &fixture.link).unwrap(),
            }
            let plan = fixture.plan();
            assert_eq!(plan.conflicts()[0].kind, ConflictKind::Taken, "{role}");
            assert!(apply(&fixture.store, &[plan]).is_err());
            assert!(std::fs::symlink_metadata(&fixture.link).is_ok());
            assert!(
                list(&fixture.store, &[fixture.edit()])
                    .unwrap()
                    .iter()
                    .all(|entry| entry.recorded)
            );
        }
    }

    #[test]
    fn changed_and_unreadable_links_refuse_undo() {
        use std::os::unix::ffi::OsStrExt;
        for role in [
            "changed-target",
            "changed-file",
            "changed-dir",
            "unreadable-target",
        ] {
            let fixture = LinkFixture::new(role);
            apply(&fixture.store, &[fixture.plan()]).unwrap();
            std::fs::remove_file(&fixture.link).unwrap();
            match role {
                "changed-file" => std::fs::write(&fixture.link, "owner bytes").unwrap(),
                "changed-dir" => std::fs::create_dir(&fixture.link).unwrap(),
                "unreadable-target" => {
                    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(b"\xff"), &fixture.link)
                        .unwrap()
                }
                _ => std::os::unix::fs::symlink(fixture.root.join("owner-target"), &fixture.link)
                    .unwrap(),
            }
            let entries = list(&fixture.store, &[]).unwrap();
            let plans =
                revert_plan(&fixture.store, &[], &Target::Ids(vec![fixture.edit().id()])).unwrap();
            assert_eq!(plans[0].conflicts().len(), 1);
            assert!(revert(&fixture.store, &plans).is_err());
            assert!(std::fs::symlink_metadata(&fixture.link).is_ok());
            if role == "unreadable-target" {
                assert!(matches!(entries[0].state, State::Unreadable(_)));
            } else {
                assert!(matches!(entries[0].state, State::Changed(_)));
            }
            assert!(entries[0].recorded);
            assert!(
                !fixture
                    .store
                    .query_row("SELECT off FROM managed_edit", [], |row| row
                        .get::<_, bool>(0))
                    .unwrap()
            );
        }
    }

