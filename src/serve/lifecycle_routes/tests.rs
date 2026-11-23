//! Tests for server lifecycle routes.

#[test]
fn lifecycle_routes_are_exercised_without_sockets() {
    let root = std::env::temp_dir().join(format!("swarm-life-routes-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(
        root.join(".gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir(root.join("bin")).unwrap();
    let git = root.join("bin/git");
    std::fs::write(
        &git,
        r#"#!/bin/sh
if [ "$3" = init ] && [ -f "$HOME/fail-init" ]; then
    if [ -f "$HOME/fail-rollback" ]; then
        store=$(/bin/cat "$HOME/fail-rollback")
        /bin/mv "$store/choices.json" "$store/choices.saved"
        /bin/mkdir "$store/choices.json"
    fi
    printf 'Fixture git init refused\n' >&2
    exit 1
fi
exec /usr/bin/git "$@"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Worktrunk removes a chat worktree, so the child gets the real binary. `wt_program` may find
    // a wrapper that reads the owner's home, often in ~/.local/bin, so this list leaves it out.
    // The child skips that part when Worktrunk is not installed.
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let real = [home.join(".cargo/bin/wt"), "/opt/homebrew/bin/wt".into()];
    if let Some(wt) = real.iter().find(|path| path.is_file()) {
        std::os::unix::fs::symlink(wt, root.join("bin/wt")).unwrap();
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "serve::tests::lifecycle_routes_child",
            "--nocapture",
        ])
        .env_clear()
        .env(
            "SWARM_TEST_WORKER",
            crate::serve::test_support::real_worker(),
        )
        .env("HOME", &root)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join(".gitconfig"))
        .env("GIT_CEILING_DIRECTORIES", &root)
        .env("SWARM_LIFECYCLE_ROUTES", "1")
        .output()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
