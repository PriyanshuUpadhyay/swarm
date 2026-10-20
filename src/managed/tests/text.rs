// Lock waits and managed text state and removal tests.


    /// A launch whose trust lock another swarm holds fails after the wait and names the lock,
    /// so a git stalled on a hung mount under the lock cannot stall every launch (SRV-18).
    #[test]
    fn a_held_trust_lock_fails_after_the_wait_and_names_the_lock() {
        let dir = std::env::temp_dir().join(format!("swarm-lock-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("trust.lock");
        let holder = std::fs::File::create(&lock).unwrap();
        holder.lock().unwrap();
        let wait = std::time::Duration::from_millis(200);
        let started = std::time::Instant::now();
        let error = with_lock_within(&lock, wait, || Ok(())).unwrap_err();
        assert!(started.elapsed() >= wait, "{:?}", started.elapsed());
        assert!(error.contains(&lock.display().to_string()), "{error}");
        assert!(error.contains("another swarm holds"), "{error}");
        drop(holder);
        assert_eq!(with_lock_within(&lock, wait, || Ok(7)), Ok(7));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A TOML bool keeps its type: a writer that turns `ui.sound.enabled` from true to false, as
    /// the Herdr switch will, is present while the file says false, and a revert puts true back
    /// with every other byte as it was.
    #[test]
    fn a_typed_toml_value_is_recorded_compared_and_set_back() {
        let dir = std::env::temp_dir().join(format!("swarm-managed-bool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        let owners = "# my herdr\n[ui.sound]\nenabled = true # loud\nvolume = 3\n";
        std::fs::write(&file, owners).unwrap();
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        let edit = Edit {
            before: Some(true.into()),
            ..Edit::new(
                Writer::Herdr,
                &file,
                Kind::TomlKey,
                &["ui", "sound", "enabled"],
                false.into(),
            )
        };
        let quiet = owners.replace("enabled = true # loud", "enabled = false # loud");
        let plan = FilePlan {
            path: file.clone(),
            before: owners.into(),
            after: quiet.clone(),
            conflicts: Vec::new(),
            edits: vec![edit.clone()],
        };
        apply(&store, &[plan.into()]).unwrap();
        assert_eq!(state(&edit, false), State::Present);
        let entries = list(&store, &[]).unwrap();
        assert_eq!(entries[0].json()["writer"], "herdr");
        assert_eq!(entries[0].json()["wrote"], false);
        assert_eq!(entries[0].json()["before"], true);

        let plans = revert_plan(&store, &[], &Target::Ids(vec![edit.id()])).unwrap();
        assert!(plans[0].conflicts().is_empty());
        revert(&store, &plans).unwrap();
        let back = std::fs::read_to_string(&file).unwrap();
        assert_eq!(back, owners);
        assert_eq!(state(&edit, true), State::Off);

        // A string where swarm wrote a bool is another value, not swarm's.
        std::fs::write(&file, quiet.replace("false", "\"false\"")).unwrap();
        assert_eq!(state(&edit, false), State::Changed("false".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An array where the path needs an object is a file swarm cannot read, not an absent item or
    /// another value, so list says `unreadable` with the reason and a revert refuses it and the
    /// row stays on (R4).
    #[test]
    fn an_array_in_the_middle_of_a_json_path_is_an_error_not_gone() {
        let dir = std::env::temp_dir().join(format!("swarm-managed-array-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        let group = serde_json::json!({"matcher": "", "hooks": []});
        let with_group = serde_json::json!({"hooks": {"PreToolUse": [group]}}).to_string();
        std::fs::write(&file, &with_group).unwrap();
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        let edit = Edit::new(
            Writer::HooksGuard,
            &file,
            Kind::JsonArrayItem,
            &["hooks", "PreToolUse"],
            group,
        );
        let plan = FilePlan {
            path: file.clone(),
            before: with_group.clone(),
            after: with_group,
            conflicts: Vec::new(),
            edits: vec![edit.clone()],
        };
        apply(&store, &[plan.into()]).unwrap();

        std::fs::write(&file, r#"{"hooks": []}"#).unwrap();
        let entry = Entry {
            edit: edit.clone(),
            state: state(&edit, false),
            recorded: true,
            at_s: None,
        };
        assert!(matches!(entry.state, State::Unreadable(_)));
        assert_eq!(entry.json()["state"], "unreadable");
        assert!(
            entry.json()["error"]
                .as_str()
                .unwrap()
                .contains("cannot read into")
        );
        assert!(entry.json()["found"].is_null());
        let plans = revert_plan(&store, &[], &Target::Ids(vec![edit.id()])).unwrap();
        assert_eq!(plans[0].conflicts().len(), 1);
        assert!(revert(&store, &plans).is_err());
        let off: bool = store
            .query_row("SELECT off FROM managed_edit", [], |row| row.get(0))
            .unwrap();
        assert!(!off);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Only an item that must stay last, as the Codex guard group, has an order conflict. Another
    /// writer's array item in a file that is also named `hooks.json` goes with items after it.
    #[test]
    fn only_an_item_that_must_stay_last_has_an_order_conflict() {
        let dir = std::env::temp_dir().join(format!("swarm-managed-last-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hooks.json");
        let swarms = serde_json::json!({"hooks": [{"command": "swarm"}]});
        let owners = serde_json::json!({"hooks": [{"command": "owner"}]});
        let text = serde_json::json!({"hooks": {"Stop": [swarms, owners]}}).to_string();
        std::fs::write(&file, &text).unwrap();
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        let edit = Edit::new(
            Writer::HooksState,
            &file,
            Kind::JsonArrayItem,
            &["hooks", "Stop"],
            swarms,
        );
        let plan = FilePlan {
            path: file.clone(),
            before: text.clone(),
            after: text,
            conflicts: Vec::new(),
            edits: vec![edit.clone()],
        };
        apply(&store, &[plan.into()]).unwrap();

        let plans = revert_plan(&store, &[], &Target::Ids(vec![edit.id()])).unwrap();
        assert!(
            plans[0].conflicts().is_empty(),
            "{:?}",
            plans[0].conflicts()
        );
        let after: serde_json::Value = serde_json::from_str(&match &plans[0] {
            Plan::File(plan) => plan.after.clone(),
            _ => panic!("expected file plan"),
        })
        .unwrap();
        assert_eq!(after["hooks"]["Stop"], serde_json::json!([owners]));

        let entry = Entry {
            edit: Edit { last: true, ..edit },
            state: State::Present,
            recorded: true,
            at_s: None,
        };
        let plan = removal(&file, &[&entry]).unwrap();
        assert_eq!(plan.conflicts[0].kind, ConflictKind::Order);
        assert_eq!(plan.after, plan.before);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A stored Codex guard group must stay last also when a link gives its `hooks.json` another
    /// name, because the row's own handler names Codex. A Claude guard group need not.
    #[test]
    fn a_stored_codex_guard_group_stays_last_under_any_file_name() {
        let dir = std::env::temp_dir().join(format!("swarm-managed-name-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        let event = crate::guard::EVENTS[0];
        let group = |provider: &str, file: &std::path::Path| {
            let command = crate::bus::guard_command(provider, event);
            let wrote = serde_json::json!({"hooks": [{"type": "command", "command": command}]});
            let text = serde_json::json!({"hooks": {event: [wrote]}}).to_string();
            std::fs::write(file, &text).unwrap();
            let edit = Edit::new(
                Writer::HooksGuard,
                file,
                Kind::JsonArrayItem,
                &["hooks", event],
                wrote,
            );
            FilePlan {
                path: file.to_path_buf(),
                before: text.clone(),
                after: text,
                conflicts: Vec::new(),
                edits: vec![edit],
            }
        };
        let codex_file = dir.join("codex-hooks.json");
        let claude_file = dir.join("claude-settings.json");
        apply(
            &store,
            &[
                group("codex", &codex_file).into(),
                group("claude", &claude_file).into(),
            ],
        )
        .unwrap();

        let entries = list(&store, &[]).unwrap();
        let last = |file: &std::path::Path| {
            let name = file.file_name().unwrap();
            let entry = entries.iter().find(|entry| entry.edit.file.ends_with(name));
            entry.unwrap().edit.last
        };
        assert!(last(&codex_file));
        assert!(!last(&claude_file));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A value that changed after `list` read it as swarm's is a conflict in the text that the
    /// revert edits, so the revert refuses it and never removes the new value.
    #[test]
    fn a_revert_refuses_a_value_that_changed_after_the_list_read() {
        let dir = std::env::temp_dir().join(format!("swarm-managed-reread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        let owners = "[hooks.state.\"stop\"]\ntrusted_hash = \"owner's\"\n";
        std::fs::write(&file, owners).unwrap();
        let edit = Edit::new(
            Writer::HooksState,
            &file,
            Kind::TomlKey,
            &["hooks", "state", "stop", "trusted_hash"],
            "swarm's".into(),
        );
        let listed = Entry {
            edit,
            state: State::Present,
            recorded: true,
            at_s: Some(0),
        };
        let plan = removal(&file, &[&listed]).unwrap();
        assert_eq!(plan.after, owners);
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].found, "\"owner's\"");
        std::fs::remove_dir_all(&dir).unwrap();
    }
