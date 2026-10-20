// Trust-target and concurrent trust-write tests.
use super::*;

mod tests {
    use super::*;

    #[test]
    fn trust_writes_keep_other_keys_order_and_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-claude-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join(".claude.json");
        std::fs::write(
            &config,
            r#"{"zeta":1,"projects":{"/other":{"allowedTools":[]}},"alpha":2}"#,
        )
        .unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();

        assert!(
            ensure_claude_trust(&config, std::path::Path::new("/repo/.herdr/workers")).unwrap()
        );
        assert!(
            !ensure_claude_trust(&config, std::path::Path::new("/repo/.herdr/workers")).unwrap()
        );
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(
            value.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["zeta", "projects", "alpha"]
        );
        assert_eq!(
            value["projects"]["/repo/.herdr/workers"]["hasTrustDialogAccepted"],
            true
        );
        assert_eq!(
            value["projects"]["/other"]["allowedTools"],
            serde_json::json!([])
        );
        assert_eq!(
            std::fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let settings = root.join("agy/settings.json");
        assert!(ensure_agy_trust(&settings, std::path::Path::new("/repo")).unwrap());
        assert!(!ensure_agy_trust(&settings, std::path::Path::new("/repo")).unwrap());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(value["trustedWorkspaces"], serde_json::json!(["/repo"]));

        std::fs::write(&config, "[]").unwrap();
        assert!(ensure_claude_trust(&config, std::path::Path::new("/repo")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trust_reaches_only_a_git_root_or_a_scratch_dir_that_is_closed_to_others() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("swarm-trust-scope-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (home, repo, scratch) = (
            base.join("home"),
            base.join("home/repo"),
            base.join("scratch"),
        );
        let (seat, open) = (scratch.join("run/seat"), scratch.join("open"));
        for dir in [&repo.join("sub"), &seat, &open] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for dir in [&base, &home, &repo, &scratch, &scratch.join("run"), &seat] {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let roots = [scratch.clone()];

        assert_eq!(
            trust_target(&repo.join("sub"), Some(&repo), &home, &roots),
            Ok(repo.clone())
        );
        assert_eq!(trust_target(&seat, None, &home, &roots), Ok(seat.clone()));
        assert!(trust_target(&home, None, &home, &roots).is_err());
        assert!(trust_target(&home, Some(&home), &home, &roots).is_err());
        assert!(trust_target(&scratch, None, &home, &roots).is_err());
        assert!(trust_target(&open, None, &home, &roots).is_err());
        std::fs::set_permissions(repo.join("sub"), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(trust_target(&repo.join("sub"), Some(&repo), &home, &roots).is_err());
        std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert!(trust_target(&seat, None, &home, &roots).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn locked_trust_writes_keep_every_entry_when_launches_race() {
        let root = std::env::temp_dir().join(format!("swarm-trust-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (settings, lock) = (root.join("settings.json"), root.join("trust.lock"));
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let (settings, lock) = (settings.clone(), lock.clone());
                std::thread::spawn(move || {
                    let dir = std::path::PathBuf::from(format!("/seat-{index}"));
                    with_lock(&lock, || ensure_agy_trust(&settings, &dir)).unwrap();
                })
            })
            .collect();
        threads
            .into_iter()
            .for_each(|thread| thread.join().unwrap());
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(value["trustedWorkspaces"].as_array().unwrap().len(), 8);
        std::fs::remove_dir_all(root).unwrap();
    }
}
