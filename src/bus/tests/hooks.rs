// Hook registration and Codex hook-trust tests.
use super::*;

mod tests {
    use super::*;

    #[test]
    fn codex_hook_trust_matches_the_hashes_a_real_codex_reports() {
        // `codex app-server` 0.159.0 `hooks/list`, empty CODEX_HOME, 2026-09-29, for these `-c`
        // hooks with the command below.
        let reported = [
            (
                "user_prompt_submit",
                "6ada9e2b38032d391c688b4b57e23d57dc704db197c59846631231b6de096fa3",
            ),
            (
                "pre_tool_use",
                "1aae45a70c770d9f20e8b9bb0606f189958d43d2cfd087e63bb2a929c354c43f",
            ),
            (
                "post_tool_use",
                "8e869f2203b834c487812e07bb16ef48cab40dd79e4e4c8b5be9970e3e6a3c27",
            ),
            (
                "permission_request",
                "fb62179a34ddeb588a7474baeb6c99f976299120c70f7b5c495e76d904e7321a",
            ),
            (
                "stop",
                "64466f2454b871f7b36673eaea4506ac61f374a030bf4b162cc432ae54612cc5",
            ),
            (
                "interrupt",
                "e24a3286d74cee9090a79c7dcf48e828202cfcccb8a511ecfadc4d0786234a8c",
            ),
        ];
        assert_eq!(
            codex_hook_trust("'/opt/homebrew/bin/swarm' hook codex"),
            reported
                .map(|(label, hash)| (
                    format!("/<session-flags>/config.toml:{label}:1:0"),
                    format!("sha256:{hash}")
                ))
                .to_vec()
        );
    }

    #[test]
    fn the_shared_codex_hook_has_the_hashes_a_real_codex_reports() {
        // `codex app-server` 0.159.0 `hooks/list`, empty CODEX_HOME, 2026-10-01, for the `-c`
        // hooks that `argv` gives a Codex agent; each was `trusted` after `swarm setup hooks`.
        let reported = [
            (
                "user_prompt_submit",
                "3e695398bce1010ee0eebd574c64c8294ace860963b23ca648d56bc51f9e27c6",
            ),
            (
                "pre_tool_use",
                "dfa165f23090cec590f7881a0c22425aef660ddc3e7eb8a0f00e950d369b6ae6",
            ),
            (
                "post_tool_use",
                "35e581b33733e40e0cdf4a03e0b024f53f58e855cb33a9b3bb1c2799a82f077d",
            ),
            (
                "permission_request",
                "cf69969f87fb6625bca8375eeaf360217099bd09aed0a4ebf2041da02e442322",
            ),
            (
                "stop",
                "72d67131393317eb54b3bcc89528b7459b44e5133ef18e3abf6a54fa392144b9",
            ),
            (
                "interrupt",
                "d08c10f1e58597bc86da11ae4e0e209d094137ce0bd4811c54c21bbfa102e547",
            ),
        ];
        assert_eq!(
            codex_hook_trust(&shared_hook_command("codex")),
            reported
                .map(|(label, hash)| (
                    format!("/<session-flags>/config.toml:{label}:1:0"),
                    format!("sha256:{hash}")
                ))
                .to_vec()
        );
    }

    #[test]
    fn codex_hook_trust_is_added_once_and_an_older_hash_is_a_conflict() {
        let home = std::env::temp_dir().join(format!("swarm-codex-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        assert!(!codex_hooks_trusted(&home, &entries));

        // A fresh Mac has no Codex home at all.
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(codex_hooks_trusted(&home, &entries));
        assert!(!set_up(codex_hook_plan(&home, &entries, &[])).unwrap());

        // Another tool's group-0 entry stays. An older hash at swarm's key is a conflict that
        // names both hashes, and the file stays as it was (ADR 0036).
        let other = "[hooks.state.\"/<session-flags>/config.toml:stop:0:0\"]\ntrusted_hash = \"sha256:other\"";
        let config = home.join("config.toml");
        let text = std::fs::read_to_string(&config).unwrap();
        let owners = format!("model = \"x\"\n{other}\n\n{text}");
        std::fs::write(&config, &owners).unwrap();
        assert!(!set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        let moved = codex_hook_trust("'/opt/swarm' hook codex");
        let plan = codex_hook_plan(&home, &moved, &[]).unwrap();
        assert_eq!(plan.conflicts.len(), moved.len());
        assert_eq!(plan.conflicts[0].found, entries[0].1);
        assert_eq!(plan.conflicts[0].wanted, moved[0].1);
        assert!(
            plan.conflicts[0]
                .fix
                .contains(&config.display().to_string())
        );
        assert_eq!(plan.after, owners);
        assert!(set_up(codex_hook_plan(&home, &moved, &[])).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), owners);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_codex_trust_entry_in_any_form_is_found_and_never_added_twice() {
        let home = std::env::temp_dir().join(format!("swarm-codex-comment-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        set_up(codex_hook_plan(&home, &entries, &[])).unwrap();
        let config = home.join("config.toml");
        let plain = std::fs::read_to_string(&config).unwrap();

        // The owner commented each header and hash; both still count and none is added again.
        let commented: String = plain
            .lines()
            .map(
                |line| match line.starts_with('[') || line.starts_with("trusted_hash") {
                    true => format!("{line}  # swarm #1"),
                    false => line.to_string(),
                },
            )
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&config, &commented).unwrap();
        assert!(codex_hooks_trusted(&home, &entries));
        assert!(!set_up(codex_hook_plan(&home, &entries, &[])).unwrap());

        // An entry in another form or key spelling is found where it is, so its other hash is
        // one conflict and the missing entries are added once.
        let (key, _) = &entries[0];
        let dotted = format!("[hooks.state]\n{key:?}.trusted_hash = \"sha256:x\"\n");
        let escaped = format!("[hooks.state.{key:?}]\ntrusted_hash = \"sha256:x\"\n");
        let inline =
            format!("hooks = {{ state = {{ {key:?} = {{ trusted_hash = \"sha256:x\" }} }} }}\n");
        // A string that holds a table header, and a nested array, are only values.
        let values =
            format!("note = \"\"\"\n[projects]\n\"\"\"\nlists = [[1, 2], [3]]\n\n{escaped}");
        for form in [
            dotted.clone(),
            dotted.replacen('/', "\\u002f", 1),
            escaped.replacen('/', "\\u002f", 1),
            inline.replacen('/', "\\u002f", 1),
            values.replacen("[hooks.state.\"/", "[hooks.state.\"\\u002f", 1),
        ] {
            std::fs::write(&config, &form).unwrap();
            let plan = codex_hook_plan(&home, &entries, &[]).unwrap();
            assert_eq!(plan.conflicts.len(), 1, "{form}");
            assert_eq!(plan.conflicts[0].found, "sha256:x", "{form}");
            let parsed: toml_edit::DocumentMut = plan.after.parse().unwrap();
            assert_eq!(
                parsed["hooks"]["state"].as_table_like().unwrap().len(),
                entries.len(),
                "{}",
                plan.after
            );
            assert!(set_up(Ok(plan)).is_err());
            assert_eq!(std::fs::read_to_string(&config).unwrap(), form);
        }
        // An escape in a table that is not a hooks table does not block setup.
        let project = "[projects.\"\\u002ftmp/project\"]\ntrust_level = \"trusted\"\n";
        std::fs::write(&config, project).unwrap();
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(codex_hooks_trusted(&home, &entries));
        assert!(
            std::fs::read_to_string(&config)
                .unwrap()
                .starts_with(project)
        );

        // A file that is not valid TOML, such as one with the same table twice, is refused and
        // stays as it was.
        let twice = format!("{escaped}{escaped}");
        std::fs::write(&config, &twice).unwrap();
        assert!(codex_hook_plan(&home, &entries, &[]).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), twice);
        // So is a file that is not UTF-8, which cannot be read as text.
        let latin1 = b"model = \"o3\"\n# caf\xe9\n".to_vec();
        std::fs::write(&config, &latin1).unwrap();
        assert!(codex_hook_plan(&home, &entries, &[]).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), latin1);

        // A commented-out old entry is not an entry, so the missing ones are added.
        let old: String = plain.lines().map(|line| format!("# {line}\n")).collect();
        std::fs::write(&config, &old).unwrap();
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(codex_hooks_trusted(&home, &entries));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn agy_hooks_keep_other_groups_and_refuse_a_broken_file_or_an_owners_swarm_group() {
        let root = std::env::temp_dir().join(format!("swarm-agy-hooks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let hooks = root.join("config/hooks.json");
        assert!(set_up(agy_hook_plan(&hooks, false)).unwrap());
        let created: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&hooks).unwrap()).unwrap();
        assert_eq!(created.as_object().unwrap().len(), 1);
        assert_eq!(
            created["swarm"]["Stop"][0]["command"],
            shared_hook_command("agy Stop")
        );
        assert_eq!(
            created["swarm"]["PostToolUse"][0]["hooks"][0]["command"],
            shared_hook_command("agy PostToolUse")
        );
        assert!(created["swarm"].get("PreToolUse").is_none());
        assert!(!set_up(agy_hook_plan(&hooks, false)).unwrap());

        let herdr = r#"{"herdr": {"PreInvocation": [{"command": "herdr-state session", "timeout": 10, "type": "command"}]}}"#;
        std::fs::write(&hooks, herdr).unwrap();
        assert!(set_up(agy_hook_plan(&hooks, false)).unwrap());
        let merged: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&hooks).unwrap()).unwrap();
        let before: serde_json::Value = serde_json::from_str(herdr).unwrap();
        assert_eq!(merged["herdr"], before["herdr"]);
        assert_eq!(merged["swarm"], created["swarm"]);

        // The owner's own group named `swarm` is a conflict, not replaced (ADR 0036).
        let owners = r#"{"herdr": {}, "swarm": {"Stop": []}}"#;
        std::fs::write(&hooks, owners).unwrap();
        let plan = agy_hook_plan(&hooks, false).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].found, r#"{"Stop":[]}"#);
        assert_eq!(plan.after, owners);
        assert!(apply(std::slice::from_ref(&plan)).is_err());
        assert!(set_up(Ok(plan)).is_err());
        assert_eq!(std::fs::read_to_string(&hooks).unwrap(), owners);

        // Consent covers the planned text too, so a swarm whose entries differ is refused.
        let planned = |after: &str| FilePlan {
            path: hooks.clone(),
            before: owners.into(),
            after: after.into(),
            conflicts: Vec::new(),
            edits: Vec::new(),
        };
        assert_ne!(
            crate::managed::digest(&[planned("{}").into()]),
            crate::managed::digest(&[planned("{\"swarm\": {}}").into()])
        );

        std::fs::write(&hooks, "{ not json").unwrap();
        assert!(agy_hook_plan(&hooks, false).is_err());
        assert_eq!(std::fs::read_to_string(&hooks).unwrap(), "{ not json");
    }
    #[test]
    fn the_guard_plan_keeps_an_owner_handler_and_refuses_a_bad_shape() {
        let root = std::env::temp_dir().join(format!("swarm-guard-plan-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        // The owner's own timeout for swarm's command stays; nothing is added beside it.
        let owned = r#"{"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "swarm guard claude PreToolUse", "timeout": 30}]}]}}"#;
        std::fs::write(&path, owned).unwrap();
        let plan = guard_hooks_plan(&path, "claude").unwrap();
        assert_eq!((plan.after == plan.before, plan.conflicts.len()), (true, 0));
        // A handler the CLI would time out before the runner answers is a conflict with its fix.
        for short in [r#", "timeout": 3"#, ""] {
            let text = format!(
                r#"{{"hooks": {{"PreToolUse": [{{"hooks": [{{"type": "command", "command": "swarm guard claude PreToolUse"{short}}}]}}]}}}}"#
            );
            std::fs::write(&path, text).unwrap();
            let plan = guard_hooks_plan(&path, "claude").unwrap();
            assert_eq!(plan.conflicts.len(), 1, "{short}");
            assert!(
                plan.conflicts[0]
                    .fix
                    .starts_with("set its timeout to 10 or more")
            );
        }
        for bad in [r#"{"hooks": []}"#, r#"{"hooks": {"PreToolUse": {}}}"#] {
            std::fs::write(&path, bad).unwrap();
            assert!(guard_hooks_plan(&path, "claude").is_err(), "{bad}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_guard_trust_follows_the_handler_as_the_file_holds_it() {
        // `codex app-server` 0.159.0 `hooks/list` on 2026-10-05 gave this key and hash for this
        // hooks.json: an owner group with a matcher, the guard second, with its own timeout.
        let hooks = r#"{"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": "echo other", "timeout": 3},
            {"type": "command", "command": "swarm guard codex PreToolUse", "timeout": 30}]}]}}"#;
        let home = std::path::Path::new("/h/.codex");
        assert_eq!(
            codex_guard_trust(home, hooks),
            [(
                "/h/.codex/hooks.json:pre_tool_use:0:1".to_string(),
                "sha256:3bca97397bb529c41561a72882fa330e8a68ca7d4fdee2101617c019f85d6faf"
                    .to_string()
            )]
        );
        let no_timeout = r#"{"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "swarm guard codex PreToolUse"}]}]}}"#;
        assert!(codex_guard_trust(home, no_timeout).is_empty());
    }
}
