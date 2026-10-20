// Link output, stored wire fields, and identity tests.

    #[test]
    fn link_diffs_and_digests_use_target_strings_and_never_target_bytes() {
        let fixture = LinkFixture::new("plan-output");
        let first = fixture.plan();
        let proof = digest(std::slice::from_ref(&first));
        let json = plan_json(std::slice::from_ref(&first));
        assert_eq!(json["files"][0]["path"], fixture.link.to_str().unwrap());
        assert!(
            json["files"][0]["diff"]
                .as_str()
                .unwrap()
                .contains(fixture.target.to_str().unwrap())
        );
        assert!(
            plan_text(std::slice::from_ref(&first), "apply", "unchanged").contains("Plan only")
        );
        std::fs::write(fixture.target.join("SKILL.md"), "target bytes change").unwrap();
        assert_eq!(proof, digest(&[fixture.plan()]));
        apply(&fixture.store, &[first]).unwrap();
        let undo = revert_plan(&fixture.store, &[], &Target::All).unwrap();
        assert!(diff(&undo[0]).contains(fixture.target.to_str().unwrap()));
        std::fs::remove_file(&fixture.link).unwrap();
        std::os::unix::fs::symlink(fixture.root.join("other-target"), &fixture.link).unwrap();
        assert_ne!(proof, digest(&[fixture.plan()]));
    }

    #[test]
    fn equivalent_target_paths_are_still_different_link_text() {
        let fixture = LinkFixture::new("exact-target-text");
        apply(&fixture.store, &[fixture.plan()]).unwrap();
        let undo = revert_plan(&fixture.store, &[], &Target::All).unwrap();
        std::fs::remove_file(&fixture.link).unwrap();
        let different = format!("{}/.", fixture.target.display());
        std::os::unix::fs::symlink(&different, &fixture.link).unwrap();
        assert!(matches!(state(&fixture.edit(), false), State::Changed(_)));
        assert!(revert(&fixture.store, &undo).is_err());
        assert_eq!(fixture.plan().conflicts()[0].kind, ConflictKind::Changed);
        let next =
            revert_plan(&fixture.store, &[], &Target::Ids(vec![fixture.edit().id()])).unwrap();
        assert_eq!(next[0].conflicts()[0].kind, ConflictKind::Changed);
        assert!(revert(&fixture.store, &next).is_err());
        assert_eq!(
            std::fs::read_link(&fixture.link).unwrap().as_os_str(),
            std::ffi::OsStr::new(&different)
        );
    }

    #[test]
    fn a_skill_link_row_keeps_the_existing_columns_and_wire_fields() {
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        let example = serde_json::json!({
            "writer": "skills", "kind": "symlink",
            "file": "/home/u/.agents/skills/flow", "path": [],
            "wrote": "/home/u/.swarm/skills/kit/skills/flow", "before": null,
            "state": "present", "recorded": true,
        });
        store
            .execute(
                "INSERT INTO managed_edit
             (id, writer, file, kind, path, wrote, before, created, with_id, at_s, off)
             VALUES ('skill-link', 'skills', ?1, 'symlink', '[]', ?2, NULL, 0, NULL, 0, 0)",
                rusqlite::params![
                    example["file"].as_str().unwrap(),
                    example["wrote"].to_string()
                ],
            )
            .unwrap();
        let mut entry = list(&store, &[]).unwrap().remove(0);
        assert_eq!(entry.edit.kind, Kind::Symlink);
        assert_eq!(entry.edit.writer, Writer::Skills);
        assert_eq!(entry.edit.created, 0);
        entry.state = State::Present;
        let row = entry.json();
        for (field, value) in example.as_object().unwrap() {
            assert_eq!(&row[field], value, "{field}");
        }
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&row.to_string()).unwrap(),
            row
        );
        let plan = Plan::Link(LinkPlan {
            path: entry.edit.file.clone(),
            before: LinkState::Absent,
            after: LinkState::Link(entry.edit.wrote.as_str().unwrap().into()),
            conflicts: Vec::new(),
            edits: vec![entry.edit],
        });
        assert!(matches!(plan, Plan::Link(_)));
    }

    #[test]
    fn a_link_id_keeps_the_parent_and_leaf_when_the_target_changes() {
        let fixture =
            std::env::temp_dir().join(format!("swarm-link-identity-{}", std::process::id()));
        std::fs::create_dir_all(&fixture).unwrap();
        let link = fixture.join("skill-link");
        let target_a = fixture.join("first-target");
        let target_b = fixture.join("second-target");
        std::fs::create_dir_all(&target_a).unwrap();
        std::fs::create_dir_all(&target_b).unwrap();
        let edit = |target: &std::path::Path| {
            Edit::new(
                Writer::Skills,
                &link,
                Kind::Symlink,
                &[],
                target.to_str().unwrap().into(),
            )
        };
        let absent = edit(&target_a);
        std::os::unix::fs::symlink(&target_a, &link).unwrap();
        let first = edit(&target_a);
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&target_b, &link).unwrap();
        let second = edit(&target_b);
        assert_eq!(absent.id(), first.id());
        assert_eq!(first.id(), second.id());
        assert_eq!(
            second.file,
            std::fs::canonicalize(&fixture).unwrap().join("skill-link")
        );
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn text_edit_ids_keep_the_existing_hash_and_follow_file_links() {
        use sha2::Digest;
        let fixture =
            std::env::temp_dir().join(format!("swarm-text-identity-{}", std::process::id()));
        std::fs::create_dir_all(&fixture).unwrap();
        let file = fixture.join("settings.json");
        let alias = fixture.join("settings-link.json");
        std::fs::write(&file, "{}").unwrap();
        std::os::unix::fs::symlink(&file, &alias).unwrap();
        for kind in [Kind::JsonKey, Kind::TomlKey, Kind::JsonArrayItem] {
            let edit = Edit::new(
                Writer::HooksState,
                &file,
                kind,
                &["hooks", "Stop"],
                "group".into(),
            );
            let alias_edit = Edit::new(
                Writer::HooksState,
                &alias,
                kind,
                &["hooks", "Stop"],
                "group".into(),
            );
            let mut digest = sha2::Sha256::new();
            digest.update(
                std::fs::canonicalize(&file)
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes(),
            );
            digest.update([0]);
            digest.update(wire(&kind).as_bytes());
            digest.update(b"\0hooks\0Stop");
            if kind == Kind::JsonArrayItem {
                digest.update(b"\0\"group\"");
            }
            let expected: String = digest.finalize()[..6]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(edit.id(), expected);
            assert_eq!(alias_edit.id(), expected);
        }
        std::fs::remove_dir_all(fixture).unwrap();
    }
