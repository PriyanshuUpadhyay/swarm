//! Tests for server panels.

#[test]
fn workspace_panels_and_issue_setup_are_exercised_without_sockets() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("swarm-panel-routes-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(
        root.join("bin/gh"),
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/gh-calls"
case "$1 $2" in
 'issue list') printf '[{"number":7,"title":"Fix Sidebar","body":"Use less space"}]\n' ;;
 'issue view') printf '{"number":7,"title":"Fix Sidebar","body":"Use less space"}\n' ;;
 *) exit 1 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(root.join("bin/gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "serve::tests::workspace_panel_routes_child",
            "--nocapture",
        ])
        .env_clear()
        .env("HOME", &root)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join(".gitconfig"))
        .env("GIT_CEILING_DIRECTORIES", &root)
        .env("SWARM_PANEL_ROUTES", "1")
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
