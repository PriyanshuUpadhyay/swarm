// Trust-plan file handling tests.
use super::*;

mod tests {
    use super::*;

    #[test]
    fn codex_trust_is_appended_once_per_cwd() {
        let root = std::env::temp_dir().join(format!("swarm-trust-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        ensure_codex_trust(&root, std::path::Path::new("/one")).unwrap();
        let once = std::fs::read_to_string(&config).unwrap();
        ensure_codex_trust(&root, std::path::Path::new("/one")).unwrap();
        assert_eq!(std::fs::read_to_string(&config).unwrap(), once);
        ensure_codex_trust(&root, std::path::Path::new("/two")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&config)
                .unwrap()
                .matches("trust_level = \"trusted\"")
                .count(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The owner may have written the project in any TOML form. A second table for it would make
    /// the whole file unreadable to Codex, so swarm adds nothing (ADR 0036).
    #[test]
    fn codex_trust_keeps_a_project_in_any_form_and_a_broken_file() {
        let root = std::env::temp_dir().join(format!("swarm-trust-forms-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        for owner in [
            "[projects]\n\"/one\" = { trust_level = \"trusted\" }\n",
            "projects.\"/one\".trust_level = \"untrusted\"\n",
            "[projects.'/one']\ntrust_level = \"trusted\"\n",
            "projects = { \"/one\" = { trust_level = \"trusted\" } }\n",
            "not toml = = \n",
        ] {
            std::fs::write(&config, owner).unwrap();
            let result = ensure_codex_trust(&root, std::path::Path::new("/one"));
            assert_eq!(std::fs::read_to_string(&config).unwrap(), owner);
            assert_eq!(result.is_err(), owner.starts_with("not toml"), "{owner}");
        }
        std::fs::write(
            &config,
            "projects = { \"/one\" = { trust_level = \"trusted\" } }\n",
        )
        .unwrap();
        ensure_codex_trust(&root, std::path::Path::new("/two")).unwrap();
        let added: toml_edit::DocumentMut =
            std::fs::read_to_string(&config).unwrap().parse().unwrap();
        assert_eq!(
            added["projects"]["/one"]["trust_level"].as_str(),
            Some("trusted")
        );
        assert_eq!(
            added["projects"]["/two"]["trust_level"].as_str(),
            Some("trusted")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A Codex config that the owner links into dotfiles stays a link, and the change lands in
    /// the file it points to (ADR 0036: a linked file is judged by its target).
    #[test]
    fn a_linked_codex_config_stays_a_link_and_its_target_gets_the_change() {
        let root = std::env::temp_dir().join(format!("swarm-trust-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dotfiles = root.join("dotfiles/config.toml");
        let home = root.join("codex");
        std::fs::create_dir_all(dotfiles.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(&dotfiles, "model = \"o3\"\n").unwrap();
        let config = home.join("config.toml");
        std::os::unix::fs::symlink(&dotfiles, &config).unwrap();
        let linked = || {
            std::fs::symlink_metadata(&config)
                .unwrap()
                .file_type()
                .is_symlink()
        };

        ensure_codex_trust(&home, std::path::Path::new("/one")).unwrap();
        assert!(linked());
        assert!(
            std::fs::read_to_string(&dotfiles)
                .unwrap()
                .contains("[projects.\"/one\"]")
        );
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        assert!(set_up(codex_hook_plan(&home, &entries, &[])).unwrap());
        assert!(linked());
        assert!(codex_hooks_trusted(&home, &entries));

        // A link to a file that does not exist yet gets its target made, as an append through the
        // link did before.
        std::fs::remove_file(&dotfiles).unwrap();
        ensure_codex_trust(&home, std::path::Path::new("/two")).unwrap();
        assert!(linked());
        assert!(
            std::fs::read_to_string(&dotfiles)
                .unwrap()
                .contains("[projects.\"/two\"]")
        );

        // A chain of links to a missing file keeps every link and makes the file at its end.
        let middle = root.join("dotfiles/middle.toml");
        std::fs::remove_file(&config).unwrap();
        std::fs::remove_file(&dotfiles).unwrap();
        std::os::unix::fs::symlink(&dotfiles, &middle).unwrap();
        std::os::unix::fs::symlink(&middle, &config).unwrap();
        ensure_codex_trust(&home, std::path::Path::new("/three")).unwrap();
        assert!(linked());
        assert!(
            std::fs::symlink_metadata(&middle)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            std::fs::read_to_string(&dotfiles)
                .unwrap()
                .contains("[projects.\"/three\"]")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// An empty settings file may be one that another program is writing now, so it is refused,
    /// as before; only a missing file counts as `{}`.
    #[test]
    fn an_empty_json_file_is_refused_and_a_missing_one_is_made() {
        let root = std::env::temp_dir().join(format!("swarm-empty-json-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (empty, missing) = (root.join("empty.json"), root.join("missing.json"));
        std::fs::write(&empty, "").unwrap();
        assert!(ensure_agy_trust(&empty, std::path::Path::new("/x")).is_err());
        assert!(agy_hook_plan(&empty, false).is_err());
        assert_eq!(std::fs::read_to_string(&empty).unwrap(), "");
        assert!(ensure_agy_trust(&missing, std::path::Path::new("/x")).unwrap());
        assert!(set_up(agy_hook_plan(&root.join("hooks.json"), false)).unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_read_only_file_is_refused_and_stays_as_it_was() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-read-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (config, hooks) = (root.join("config.toml"), root.join("hooks.json"));
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        for (file, text) in [(&config, "model = \"o4\"\n"), (&hooks, "{}\n")] {
            std::fs::write(file, text).unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o444)).unwrap();
        }
        assert!(ensure_codex_trust(&root, std::path::Path::new("/one")).is_err());
        assert!(codex_hook_plan(&root, &entries, &[]).is_err());
        assert!(agy_hook_plan(&hooks, false).is_err());
        assert!(ensure_agy_trust(&hooks, std::path::Path::new("/one")).is_err());
        for (file, text) in [(&config, "model = \"o4\"\n"), (&hooks, "{}\n")] {
            assert_eq!(std::fs::read_to_string(file).unwrap(), text);
            let mode = std::fs::metadata(file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o444);
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_file_that_changes_after_it_was_read_is_not_written_over() {
        let root = std::env::temp_dir().join(format!("swarm-changed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        std::fs::write(&config, "model = \"o3\"\n").unwrap();
        let plan =
            codex_hook_plan(&root, &codex_hook_trust("'/bin/swarm' hook codex"), &[]).unwrap();
        let owners = "model = \"o3\"\napproval_policy = \"never\"\n";
        std::fs::write(&config, owners).unwrap();
        assert!(apply(std::slice::from_ref(&plan)).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), owners);
        let hooks = root.join("hooks.json");
        let plan = agy_hook_plan(&hooks, false).unwrap();
        std::fs::write(&hooks, "{\"other\": {}}\n").unwrap();
        assert!(apply(std::slice::from_ref(&plan)).is_err());
        assert_eq!(
            std::fs::read_to_string(&hooks).unwrap(),
            "{\"other\": {}}\n"
        );
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            2,
            "no temp file left"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_launch_edit_reads_a_changed_file_again_and_stops_on_any_other_error() {
        let changed = |path: &str| changed_error(std::path::Path::new(path));
        let mut tries = 0;
        let result = retried(|| {
            tries += 1;
            if tries < 3 {
                Err(changed("/c.json"))
            } else {
                Ok(tries)
            }
        });
        assert_eq!(result, Ok(3));
        let mut tries = 0;
        let result: Result<(), String> = retried(|| {
            tries += 1;
            Err("swarm: cannot parse /c.json".into())
        });
        assert_eq!((result.is_err(), tries), (true, 1));
        let mut tries = 0;
        let result: Result<(), String> = retried(|| {
            tries += 1;
            Err(changed("/c.json"))
        });
        assert_eq!((result, tries), (Err(changed("/c.json")), 4));
    }

    #[test]
    fn two_homes_that_link_to_one_config_both_set_up() {
        let root = std::env::temp_dir().join(format!("swarm-twin-homes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (home, spare) = (root.join(".codex"), root.join(".codex-work"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&spare).unwrap();
        std::fs::write(home.join("config.toml"), "model = \"o4\"\n").unwrap();
        std::os::unix::fs::symlink(home.join("config.toml"), spare.join("config.toml")).unwrap();
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        let plans = [
            codex_hook_plan(&home, &entries, &[]).unwrap(),
            codex_hook_plan(&spare, &entries, &[]).unwrap(),
        ];
        // Both plans have one write target, so their edits have one id and one row.
        assert_eq!(plans[0].edits[0].id(), plans[1].edits[0].id());
        assert_eq!(apply(&plans).unwrap().len(), 2);
        assert!(codex_hooks_trusted(&spare, &entries));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_hard_linked_config_keeps_one_file_for_both_names() {
        use std::os::unix::fs::MetadataExt;
        let root = std::env::temp_dir().join(format!("swarm-hard-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (config, dotfile) = (root.join("config.toml"), root.join("dotfiles-config.toml"));
        std::fs::write(&dotfile, "model = \"o4\"\n").unwrap();
        std::fs::hard_link(&dotfile, &config).unwrap();
        ensure_codex_trust(&root, std::path::Path::new("/one")).unwrap();
        assert!(
            set_up(codex_hook_plan(
                &root,
                &codex_hook_trust("'/bin/swarm' hook codex"),
                &[]
            ))
            .unwrap()
        );
        let (a, b) = (
            std::fs::metadata(&config).unwrap(),
            std::fs::metadata(&dotfile).unwrap(),
        );
        assert_eq!((a.ino(), a.nlink()), (b.ino(), 2));
        assert!(std::fs::read_to_string(&dotfile).unwrap().contains("/one"));
        // A JSON file is still replaced in one rename, as at the base, because a running CLI may
        // read `~/.claude.json` at any moment.
        let (claude, copy) = (root.join(".claude.json"), root.join("claude-copy.json"));
        std::fs::write(&copy, "{}\n").unwrap();
        std::fs::hard_link(&copy, &claude).unwrap();
        assert!(ensure_claude_trust(&claude, std::path::Path::new("/one")).unwrap());
        assert_eq!(std::fs::metadata(&claude).unwrap().nlink(), 1);
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "{}\n");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_locked_file_with_write_bits_is_refused_in_the_plan() {
        let root = std::env::temp_dir().join(format!("swarm-locked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (config, hooks) = (root.join("config.toml"), root.join("hooks.json"));
        std::fs::write(&config, "model = \"o4\"\n").unwrap();
        std::fs::write(&hooks, "{}\n").unwrap();
        let chflags = |flag: &str| {
            let status = std::process::Command::new("chflags")
                .args([flag])
                .args([&config, &hooks])
                .status()
                .unwrap();
            assert!(status.success());
        };
        // Finder's Locked box: the mode stays 0644, but no one can write the file.
        chflags("uchg");
        let (plan, agy) = (
            codex_hook_plan(&root, &codex_hook_trust("'/bin/swarm' hook codex"), &[]),
            agy_hook_plan(&hooks, false),
        );
        chflags("nouchg");
        assert!(plan.is_err());
        assert!(agy.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_config_in_a_read_only_folder_is_refused_in_the_plan() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-read-only-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("config.toml"), "model = \"o4\"\n").unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        let (plan, trust) = (
            codex_hook_plan(&root, &entries, &[]),
            ensure_codex_trust(&root, std::path::Path::new("/one")),
        );
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(plan.is_err());
        assert!(trust.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_link_into_a_missing_folder_makes_no_folder() {
        let root = std::env::temp_dir().join(format!("swarm-link-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let gone = root.join("dotfiles");
        std::os::unix::fs::symlink(gone.join("codex/config.toml"), root.join("config.toml"))
            .unwrap();
        std::os::unix::fs::symlink(gone.join("agy/hooks.json"), root.join("hooks.json")).unwrap();
        let entries = codex_hook_trust("'/bin/swarm' hook codex");
        assert!(ensure_codex_trust(&root, std::path::Path::new("/one")).is_err());
        assert!(codex_hook_plan(&root, &entries, &[]).is_err());
        assert!(agy_hook_plan(&root.join("hooks.json"), false).is_err());
        assert!(!gone.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
