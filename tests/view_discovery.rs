mod sidebar_fixture;
use serde_json::json;
use sidebar_fixture::ChoicesHome;
use std::{fs, process::Command, time::Instant};
use swarm::view::{
    choices::OwnerChoices,
    discovery::{self, Discovery},
    github, tree,
};
#[test]
fn github_remote_identity_rejects_foreign_or_ambiguous_urls() {
    for remote in [
        "git@github.com:Owner/Repo.git",
        "https://github.com/Owner/Repo",
        "ssh://git@github.com/Owner/Repo.git",
    ] {
        assert_eq!(github::repository(remote).as_deref(), Some("owner/repo"));
    }
    for remote in [
        "https://evil.com/owner/repo",
        "https://github.com/owner/repo?token=x",
        "https://github.com/owner/../repo",
        "https://github.com/owner/repo/extra",
        "git@github.com:owner/repo#fragment",
    ] {
        assert!(github::repository(remote).is_none());
    }
}
#[test]
fn standin_git_and_gh_refresh_titles_catalog_and_requested_fields() {
    let home = ChoicesHome::new();
    let bin = home.root.join("bin");
    fs::create_dir(&bin).unwrap();
    let git = r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/git-calls"
shift 2
case "$*" in
'rev-parse --path-format=absolute --git-common-dir') printf '%s\n' "$HOME/repo/.git" ;;
'worktree list --porcelain -z') printf 'worktree %s\000branch refs/heads/feature\000\000' "$HOME/repo" ;;
'symbolic-ref --quiet --short HEAD') printf 'feature\n' ;;
'rev-parse HEAD') printf 'abc\n' ;;
'remote') printf 'origin\n' ;;
'remote get-url --push --all -- origin'|'remote get-url -- origin') printf 'git@github.com:owner/repo.git\n' ;;
'status --porcelain=v1 -z') printf ' M changed\000' ;;
*) exit 1 ;;
esac
"#;
    let gh = r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/gh-calls"
case "$1 $2" in
'repo view') printf '{"nameWithOwner":"owner/repo","parent":null}\n' ;;
'pr list') if [ -f "$HOME/fail" ]; then exit 1; fi
printf '[{"number":7,"state":"OPEN","baseRefOid":"def","headRefOid":"abc","headRefName":"feature","headRepository":{"nameWithOwner":"owner/repo"},"url":"https://github.com/owner/repo/pull/7","statusCheckRollup":[{"state":"SUCCESS"}]}]\n' ;;
*) exit 1 ;;
esac
"#;
    use std::os::unix::fs::PermissionsExt;
    for (name, body) in [("git", git), ("gh", gh)] {
        fs::write(bin.join(name), body).unwrap();
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::create_dir(home.root.join("repo")).unwrap();
    fs::write(
        home.root.join("chair.jsonl"),
        "{\"type\":\"user\",\"message\":{\"content\":\"# AGENTS.md instructions\"}}\n",
    )
    .unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "discovery_standin_child", "--nocapture"])
        .env_clear()
        .env("HOME", &home.root)
        .env("PATH", &bin)
        .env("SWARM_DISCOVERY_FIXTURE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn discovery_standin_child() {
    if std::env::var("SWARM_DISCOVERY_FIXTURE").is_err() {
        return;
    }
    let home = std::env::var("HOME").unwrap();
    let cwd = format!("{home}/repo");
    let log = format!("{home}/chair.jsonl");
    let mut snapshot = json!({"sessions":[{"id":"chat","cwd":cwd,"chair_log":log,"agents":1,"chair_provider":"codex","created_at":1}],"agents":{"chat":[{"id":"orchestrator","alive":true,"pane":"fixture","state":"working"}]}});
    let mut d = Discovery::default();
    let mut owner = OwnerChoices::default();
    owner.project_paths.push(cwd.clone());
    d.augment(&mut snapshot, &owner, 100);
    assert_eq!(snapshot["projects"].as_array().unwrap().len(), 1);
    let calls = fs::read_to_string(format!("{home}/git-calls")).unwrap();
    d.augment(&mut snapshot, &owner, 109);
    assert_eq!(
        fs::read_to_string(format!("{home}/git-calls")).unwrap(),
        calls
    );
    d.augment(&mut snapshot, &owner, 110);
    assert_ne!(
        fs::read_to_string(format!("{home}/git-calls")).unwrap(),
        calls
    );
    fs::write(
        &log,
        "{\"type\":\"user\",\"message\":{\"content\":\"Fix the sidebar\\nwith tests\"}}\n",
    )
    .unwrap();
    // A chat with no title reads its changed log at once.
    d.augment(&mut snapshot, &owner, 129);
    assert_eq!(snapshot["sessions"][0]["title"], "Fix the sidebar");
    let mut file = fs::OpenOptions::new().append(true).open(&log).unwrap();
    std::io::Write::write_all(
        &mut file,
        b"{\"type\":\"ai-title\",\"aiTitle\":\"Sidebar fix\"}\n",
    )
    .unwrap();
    // A titled chat reads it again only 30 s later.
    d.augment(&mut snapshot, &owner, 130);
    assert_eq!(snapshot["sessions"][0]["title"], "Fix the sidebar");
    d.augment(&mut snapshot, &owner, 159);
    assert_eq!(snapshot["sessions"][0]["title"], "Sidebar fix");
    let t = tree::build(&snapshot, &owner);
    assert!(!d.refresh_fields(&t, &owner, 130));
    assert!(!std::path::Path::new(&format!("{home}/gh-calls")).exists());
    owner.fields.workspace.extend(["dirty".into(), "ci".into()]);
    assert!(d.refresh_fields(&t, &owner, 131));
    assert_eq!(d.fields[&cwd].dirty_count, Some(1));
    assert_eq!(d.fields[&cwd].pr.as_deref(), Some("PR #7"));
    assert_eq!(d.fields[&cwd].ci.as_deref(), Some("CI passed"));
    let ghcalls = fs::read_to_string(format!("{home}/gh-calls")).unwrap();
    d.refresh_fields(&t, &owner, 250);
    assert_eq!(
        fs::read_to_string(format!("{home}/gh-calls")).unwrap(),
        ghcalls
    );
    fs::write(format!("{home}/fail"), "").unwrap();
    assert!(d.refresh_fields(&t, &owner, 251));
    assert!(d.fields[&cwd].pr.is_none());
    assert!(d.fields[&cwd].ci.is_none());
}
#[test]
fn subprocess_failure_and_deadline_are_bounded_and_reaped() {
    assert!(discovery::run("/usr/bin/false", &[], 1).is_none());
    let before = Instant::now();
    assert!(discovery::run("/bin/sleep", &["5"], 1).is_none());
    assert!(before.elapsed().as_secs_f64() < 2.0);
    let before = Instant::now();
    assert!(discovery::run("/bin/sh", &["-c", "sleep 5 &"], 1).is_none());
    assert!(before.elapsed().as_secs_f64() < 2.0);
}
