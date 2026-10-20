// Link write failures and preflight tests.

    #[test]
    fn a_link_insert_failure_leaves_the_leaf_absent() {
        let fixture = LinkFixture::new("insert-failure");
        fixture.store.execute_batch("CREATE TRIGGER refuse_link BEFORE INSERT ON managed_edit BEGIN SELECT RAISE(ABORT, 'fixture insert failure'); END;").unwrap();
        let error = apply(&fixture.store, &[fixture.plan()]).unwrap_err();
        assert!(error.contains("fixture insert failure"), "{error}");
        assert!(std::fs::symlink_metadata(&fixture.link).is_err());
        assert!(list(&fixture.store, &[]).unwrap().is_empty());
    }

    #[test]
    fn a_link_commit_failure_reports_the_written_leaf_and_never_adopts_it() {
        let fixture = LinkFixture::new("commit-failure");
        fixture.store.execute_batch("CREATE TABLE commit_fault (edit TEXT REFERENCES managed_edit(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fail_commit AFTER INSERT ON managed_edit BEGIN INSERT INTO commit_fault VALUES ('missing-edit'); END;").unwrap();
        let original = fixture.plan();
        let error = apply(&fixture.store, std::slice::from_ref(&original)).unwrap_err();
        assert!(error.contains("could not record"), "{error}");
        assert!(
            error.contains(&format!("already wrote {}", fixture.link.display())),
            "{error}"
        );
        assert_eq!(std::fs::read_link(&fixture.link).unwrap(), fixture.target);
        assert!(list(&fixture.store, &[]).unwrap().is_empty());
        fixture
            .store
            .execute_batch("DROP TRIGGER fail_commit")
            .unwrap();
        assert_eq!(fixture.plan().conflicts()[0].kind, ConflictKind::Taken);
        assert!(apply(&fixture.store, &[original]).is_err());
        assert!(list(&fixture.store, &[]).unwrap().is_empty());
    }

    #[test]
    fn a_later_link_write_failure_reports_the_first_written_destination() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = LinkFixture::new("partial-write");
        let refused_parent = fixture.root.join("home/refused-skills");
        std::fs::create_dir(&refused_parent).unwrap();
        let refused_link = refused_parent.join("second-link");
        let second =
            LinkPlan::create(&fixture.store, refused_link.clone(), fixture.target.clone()).unwrap();
        std::fs::set_permissions(&refused_parent, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = apply(&fixture.store, &[fixture.plan(), second.into()]);
        std::fs::set_permissions(&refused_parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = result.unwrap_err();
        assert!(
            error.contains(&refused_link.display().to_string()),
            "{error}"
        );
        assert!(
            error.contains(&format!("already wrote {}", fixture.link.display())),
            "{error}"
        );
        assert_eq!(list(&fixture.store, &[]).unwrap().len(), 1);
        assert!(std::fs::symlink_metadata(refused_link).is_err());
    }

    #[test]
    fn link_conflicts_are_preflighted_and_a_replaced_parent_is_refused() {
        let fixture = LinkFixture::new("preflight");
        let foreign = fixture.root.join("home/skills/foreign-link");
        std::fs::write(&foreign, "owner bytes").unwrap();
        let conflict = LinkPlan::create(&fixture.store, foreign, fixture.target.clone()).unwrap();
        assert!(apply(&fixture.store, &[fixture.plan(), conflict.into()]).is_err());
        assert!(std::fs::symlink_metadata(&fixture.link).is_err());
        let parent_alias = fixture.root.join("home/skill-alias");
        std::os::unix::fs::symlink(fixture.link.parent().unwrap(), &parent_alias).unwrap();
        let alias_link = parent_alias.join("skill-link");
        let plan =
            LinkPlan::create(&fixture.store, alias_link.clone(), fixture.target.clone()).unwrap();
        let previous_digest = digest(&[plan.clone().into()]);
        let other_parent = fixture.root.join("home/other-skills");
        std::fs::create_dir(&other_parent).unwrap();
        std::fs::remove_file(&parent_alias).unwrap();
        std::os::unix::fs::symlink(&other_parent, &parent_alias).unwrap();
        let next = LinkPlan::create(&fixture.store, alias_link, fixture.target.clone()).unwrap();
        assert_ne!(previous_digest, digest(&[next.into()]));
        assert!(apply(&fixture.store, &[plan.into()]).is_err());
        assert!(std::fs::symlink_metadata(other_parent.join("skill-link")).is_err());
    }

    #[test]
    fn an_unreadable_parent_is_unreadable_and_a_changed_parent_refuses_undo() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = LinkFixture::new("undo-parent-checks");
        apply(&fixture.store, &[fixture.plan()]).unwrap();
        let parent = fixture.link.parent().unwrap();
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = state(&fixture.edit(), false);
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(unreadable, State::Unreadable(_)), "{unreadable:?}");
        let undo = revert_plan(&fixture.store, &[], &Target::All).unwrap();
        let saved_parent = fixture.root.join("home/saved-skills");
        let owner_parent = fixture.root.join("home/owner-skills");
        std::fs::create_dir(&owner_parent).unwrap();
        std::os::unix::fs::symlink(&fixture.target, owner_parent.join("skill-link")).unwrap();
        std::fs::rename(parent, &saved_parent).unwrap();
        std::os::unix::fs::symlink(&owner_parent, parent).unwrap();
        assert!(
            revert(&fixture.store, &undo)
                .unwrap_err()
                .contains("parent")
        );
        assert!(std::fs::read_link(owner_parent.join("skill-link")).is_ok());
        assert!(std::fs::read_link(saved_parent.join("skill-link")).is_ok());
    }

    #[test]
    fn a_later_insert_failure_and_a_read_only_store_never_claim_missing_links() {
        let fixture = LinkFixture::new("later-insert-failure");
        let second_path = fixture.root.join("home/skills/second-link");
        let second =
            LinkPlan::create(&fixture.store, second_path.clone(), fixture.target.clone()).unwrap();
        let readonly = rusqlite::Connection::open_with_flags(
            fixture.root.join("swarm/managed.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert!(apply(&readonly, &[fixture.plan()]).is_err());
        assert!(std::fs::symlink_metadata(&fixture.link).is_err());
        fixture.store.execute_batch("CREATE TRIGGER refuse_second BEFORE INSERT ON managed_edit WHEN NEW.file LIKE '%second-link' BEGIN SELECT RAISE(ABORT, 'later insert failure'); END;").unwrap();
        let error = apply(&fixture.store, &[fixture.plan(), second.into()]).unwrap_err();
        assert!(error.contains("later insert failure"), "{error}");
        assert!(
            error.contains(&format!("already wrote {}", fixture.link.display())),
            "{error}"
        );
        assert!(std::fs::symlink_metadata(second_path).is_err());
        assert_eq!(list(&fixture.store, &[]).unwrap().len(), 1);
    }

