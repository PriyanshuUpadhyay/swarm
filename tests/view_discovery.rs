mod sidebar_fixture;
use serde_json::{Value, json};
use sidebar_fixture::ChoicesHome;
use std::{fs, path::Path, process::Command};
use swarm::view::{
    choices::OwnerChoices,
    discovery::{self, Discovery},
    fields::FieldReader,
    github, tree,
};

fn chat_snapshot(cwd: &Path, log: &Path) -> Value {
    json!({
        "sessions": [{"id":"chat","cwd":cwd,"chair_log":log,"agents":1,"adapter":"tmux-swarm","chair_provider":"codex","created_at":1}],
        "agents": {"chat":[{"id":"orchestrator","alive":true,"pane":"fixture","state":"working"}]},
    })
}

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
    // The stand-ins are written here, in a process with no other test thread. A parallel test that
    // forks while the parent holds a script open for writing makes Linux exec fail with ETXTBSY.
    let git = r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/git-calls"
shift 2
case "$*" in
'rev-parse --path-format=absolute --git-common-dir') printf '%s\n' "$HOME/repo/.git" ;;
'worktree list --porcelain -z') printf 'worktree %s\000branch refs/heads/feature\000\000' "$HOME/repo" ;;
'symbolic-ref --quiet --short HEAD') printf 'feature\n' ;;
'rev-parse HEAD') if [ -f "$HOME/new-head" ]; then printf 'new\n'; else printf 'abc\n'; fi ;;
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
case "$*" in
*'--state merged'*) printf '[{"number":8,"state":"MERGED","baseRefOid":"def","headRefOid":"abc","headRefName":"feature","headRepository":{"nameWithOwner":"owner/repo"},"url":"https://github.com/owner/repo/pull/8"}]\n' ;;
*)
    if [ -f "$HOME/merged" ]; then printf '[]\n'; exit; fi
    url=https://github.com/Owner/Repo/pull/7
    if [ -f "$HOME/foreign" ]; then url=https://example.com/owner/repo/pull/7; fi
    pr="{\"number\":7,\"state\":\"OPEN\",\"isDraft\":true,\"baseRefOid\":\"def\",\"headRefOid\":\"abc\",\"headRefName\":\"feature\",\"headRepository\":{\"nameWithOwner\":\"owner/repo\"},\"url\":\"$url\",\"statusCheckRollup\":[{\"state\":\"SUCCESS\"}]}"
    if [ -f "$HOME/ambiguous" ]; then printf '[%s,%s]\n' "$pr" "$pr"; else printf '[%s]\n' "$pr"; fi ;;
esac ;;
*) exit 1 ;;
esac
"#;
    use std::os::unix::fs::PermissionsExt;
    for (name, body) in [("git", git), ("gh", gh)] {
        fs::write(format!("{home}/bin/{name}"), body).unwrap();
        fs::set_permissions(
            format!("{home}/bin/{name}"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let cwd = format!("{home}/repo");
    let log = format!("{home}/chair.jsonl");
    let mut snapshot = chat_snapshot(Path::new(&cwd), Path::new(&log));
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
    let mut d = FieldReader::default();
    assert!(d.refresh_fields(&t, &owner, 130));
    assert_eq!(
        serde_json::to_value(&d.fields[&cwd]).unwrap()["prFact"],
        json!({"number":7,"url":"https://github.com/Owner/Repo/pull/7","state":"draft","ci":"passed"})
    );
    assert!(d.fields[&cwd].dirty_count.is_none());
    owner.fields.workspace.push("dirty".into());
    assert!(d.refresh_fields(&t, &owner, 131));
    assert_eq!(d.fields[&cwd].dirty_count, Some(1));
    assert_eq!(d.fields[&cwd].pr.as_deref(), Some("PR #7"));
    assert_eq!(d.fields[&cwd].ci.as_deref(), Some("CI passed"));
    let ghcalls = fs::read_to_string(format!("{home}/gh-calls")).unwrap();
    d.refresh_fields(&t, &owner, 249);
    assert_eq!(
        fs::read_to_string(format!("{home}/gh-calls")).unwrap(),
        ghcalls
    );
    let pr_before_error = d.fields[&cwd].clone();
    let fail = format!("{home}/fail");
    fs::write(&fail, "").unwrap();
    d.refresh_fields(&t, &owner, 250);
    assert_eq!(d.fields[&cwd], pr_before_error);
    fs::remove_file(fail).unwrap();

    let after_error = fs::read_to_string(format!("{home}/gh-calls")).unwrap();
    d.refresh_fields(&t, &owner, 251);
    assert_eq!(
        fs::read_to_string(format!("{home}/gh-calls")).unwrap(),
        after_error
    );

    for (at, marker) in [(370, "ambiguous"), (490, "foreign")] {
        let flag = format!("{home}/{marker}");
        fs::write(&flag, "").unwrap();
        d.refresh_fields(&t, &owner, at);
        assert!(serde_json::to_value(&d.fields[&cwd]).unwrap()["prFact"].is_null());
        fs::remove_file(flag).unwrap();
    }
    assert!(
        !fs::read_to_string(format!("{home}/gh-calls"))
            .unwrap()
            .contains("--state merged")
    );
    fs::write(format!("{home}/merged"), "").unwrap();
    assert!(d.refresh_fields(&t, &owner, 610));
    assert_eq!(
        serde_json::to_value(&d.fields[&cwd]).unwrap()["prFact"],
        json!({"number":8,"url":"https://github.com/owner/repo/pull/8","state":"merged","ci":"unknown"})
    );
    assert!(github::lookup(&cwd).unwrap().is_null());
    fs::write(format!("{home}/new-head"), "").unwrap();
    assert!(d.refresh_fields(&t, &owner, 730));
    assert!(d.fields[&cwd].pr_fact.is_none());
}

#[test]
fn branch_totals_count_only_committed_changes_from_the_merge_base() {
    let home = ChoicesHome::new();
    let repo = home.root.join("repo");
    fs::create_dir(&repo).unwrap();
    let repo = fs::canonicalize(repo).unwrap();

    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };

    git(&["init", "-qb", "main"]);
    fs::write(repo.join("lines"), "keep\nremove\n").unwrap();
    git(&["add", "lines"]);
    git(&["commit", "-qm", "base"]);
    git(&["switch", "-qc", "feature"]);
    fs::write(repo.join("lines"), "keep\nadd one\nadd two\n").unwrap();
    git(&["commit", "-qam", "change"]);
    fs::write(repo.join("lines"), "uncommitted\n").unwrap();

    let log = home.root.join("chair.jsonl");
    fs::write(
        &log,
        "{\"type\":\"user\",\"message\":{\"content\":\"Change lines\"}}\n",
    )
    .unwrap();
    let mut snapshot = chat_snapshot(&repo, &log);
    let mut discovery = Discovery::default();
    let owner = OwnerChoices::default();
    discovery.augment(&mut snapshot, &owner, 100);
    let tree = tree::build(&snapshot, &owner);
    let mut reader = FieldReader::default();
    reader.select(repo.to_str());
    reader.refresh_fields(&tree, &owner, 100);

    assert_eq!(
        serde_json::to_value(reader.fields.get(repo.to_str().unwrap())).unwrap()["diff"],
        json!({"addedLines":2,"removedLines":1})
    );
}
#[test]
fn subprocess_failure_and_deadline_are_bounded_and_reaped() {
    assert!(discovery::run("/usr/bin/false", &[], 1).is_none());
    assert!(discovery::run("/bin/sleep", &["5"], 1).is_none());
    assert!(discovery::run("/bin/sh", &["-c", "sleep 5 &"], 1).is_none());
}

#[test]
fn project_id_is_shared_by_worktrees_and_falls_back_to_the_folder() {
    let root = std::env::temp_dir().join(format!("swarm-project-id-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let git = |args: &[&str]| {
        assert!(Command::new("git").args(args).status().unwrap().success());
    };
    let hub = root.join("hub");
    git(&["init", "-q", "--bare", hub.join(".bare").to_str().unwrap()]);
    let bare = hub.join(".bare");
    git(&[
        "-C",
        bare.to_str().unwrap(),
        "worktree",
        "add",
        "-q",
        "--orphan",
        "-b",
        "main",
        hub.join("main").to_str().unwrap(),
    ]);
    git(&[
        "-C",
        bare.to_str().unwrap(),
        "worktree",
        "add",
        "-q",
        "--orphan",
        "-b",
        "side",
        hub.join("side").to_str().unwrap(),
    ]);
    let workers = hub.join("main/.herdr/workers");
    fs::create_dir_all(&workers).unwrap();
    let expected = format!("repository:{}", bare.display());
    for path in [hub.join("main"), hub.join("side"), workers] {
        assert_eq!(discovery::project_id(path.to_str().unwrap()), expected);
    }
    let plain = root.join("plain");
    fs::create_dir(&plain).unwrap();
    assert_eq!(
        discovery::project_id(plain.to_str().unwrap()),
        format!("folder:{}", plain.display())
    );
    fs::remove_dir_all(&root).unwrap();
}
