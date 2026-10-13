use serde_json::json;
use std::{fs, path::PathBuf, process::Command};
use swarm::view::{
    changes::{self, Mode},
    choices::{self, OwnerChoices, ViewState},
    discovery::Discovery,
    lifecycle, tree, workspace_panel,
};

struct Fixture {
    root: PathBuf,
    repo: String,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
        let repo = root.join("project").to_string_lossy().into_owned();
        fs::create_dir(&repo).unwrap();
        lifecycle::check_git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        lifecycle::check_git(&repo, &["config", "user.name", "Fixture"]).unwrap();
        lifecycle::check_git(&repo, &["config", "user.email", "fixture@example.com"]).unwrap();
        lifecycle::check_git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/project.git",
            ],
        )
        .unwrap();
        Self { root, repo }
    }
    fn git(&self, args: &[&str]) -> String {
        lifecycle::check_git(&self.repo, args).unwrap()
    }
    fn write(&self, name: &str, contents: &str) {
        fs::write(PathBuf::from(&self.repo).join(name), contents).unwrap();
    }
    fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&["-c", "commit.gpgsign=false", "commit", "-qm", "fixture"]);
    }
}
macro_rules! isolated {
    ($name:ident, |$fixture:ident| $body:block) => {
        #[test]
        fn $name() {
            if std::env::var("SWARM_PANEL_CASE").ok().as_deref() != Some(stringify!($name)) {
                let root = std::env::temp_dir().join(format!("swarm-panel-{}", uuid::Uuid::now_v7()));
                fs::create_dir_all(root.join("bin")).unwrap();
                use std::os::unix::fs::PermissionsExt;
                fs::write(root.join("bin/gh"), r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/gh-calls"
case "$1 $2" in
 'repo view') printf '{"nameWithOwner":"example/project","parent":null}\n' ;;
 'pr list') cat "$HOME/pr-list" ;;
 'pr view') cat "$HOME/pr-view" ;;
 'issue list') printf '[{"number":7,"title":"Fix Sidebar","body":"Use less space","state":"OPEN"}]\n' ;;
 'issue view') printf '{"number":7,"title":"Fix Sidebar","body":"Use less space","state":"OPEN"}\n' ;;
 *) exit 1 ;;
esac
"#).unwrap();
                fs::set_permissions(root.join("bin/gh"), fs::Permissions::from_mode(0o755)).unwrap();
                let out = Command::new(std::env::current_exe().unwrap()).args(["--exact", stringify!($name), "--nocapture"])
                    .env_clear().env("HOME", &root).env("PATH", format!("{}:/usr/bin:/bin", root.join("bin").display()))
                    .env("GIT_CEILING_DIRECTORIES", fs::canonicalize(&root).unwrap()).env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_GLOBAL", root.join(".gitconfig")).env("SWARM_PANEL_CASE", stringify!($name))
                    .envs(std::env::var("SWARM_GIT_BASELINE").ok().map(|value| ("SWARM_GIT_BASELINE", value))).output().unwrap();
                fs::remove_dir_all(root).unwrap();
                print!("{}", String::from_utf8_lossy(&out.stdout));
                assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                return;
            }
            let $fixture = Fixture::new();
            $body
        }
    }
}

isolated!(
    changes_reads_only_selected_path_and_counts_git_spawns,
    |fixture| {
        use std::os::unix::fs::PermissionsExt;
        fixture.write("file.txt", "base\n");
        fixture.write("unchanged.txt", "unchanged\n");
        fixture.commit();
        fixture.git(&["switch", "-qc", "feature"]);
        fixture.write("file.txt", &"new\n".repeat(20_000));
        fixture.commit();
        fixture.write("file.txt", &"local\n".repeat(20_000));
        fs::create_dir(PathBuf::from(&fixture.repo).join("folder")).unwrap();
        fixture.write("folder/child", "");
        let wrapper = fixture.root.join("bin/git");
        fs::write(&wrapper, "#!/bin/sh\ncase \"$3\" in --literal-pathspecs) printf '%s\\n' \"$*\" >> \"$HOME/git-calls\" ;; esac\nexec /usr/bin/git \"$@\"\n").unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
        let calls = fixture.root.join("git-calls");
        let baseline = std::env::var_os("SWARM_GIT_BASELINE").is_some();
        for (mode, name, list_count, patch_count) in
            [(Mode::Local, "local", 6, 5), (Mode::Branch, "branch", 8, 8)]
        {
            for (patch, expected) in [(false, list_count), (true, patch_count)] {
                fs::write(&calls, "").unwrap();
                let start = std::time::Instant::now();
                let value = if patch {
                    changes::patch(&fixture.repo, "file.txt", &mode, Some("main"), None)
                } else {
                    changes::list(&fixture.repo, &mode, Some("main"))
                }
                .unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                if patch {
                    assert_eq!(value["added"], 20_000);
                }
                let commands = fs::read_to_string(&calls).unwrap();
                let count = commands.lines().count();
                println!("{name} patch={patch} elapsed_ms={elapsed:.3} git_spawns={count}");
                if !baseline {
                    assert_eq!(count, expected);
                }
                if patch && !baseline {
                    assert!(!commands.contains("--numstat -z --\n"));
                }
            }
        }
        for path in ["unchanged.txt", "missing.txt", "folder"] {
            assert!(changes::patch(&fixture.repo, path, &Mode::Local, None, None).is_err());
            assert!(
                changes::patch(&fixture.repo, path, &Mode::Branch, Some("main"), None).is_err()
            );
        }
        assert!(
            changes::patch(
                &fixture.repo,
                "file.txt",
                &Mode::Local,
                None,
                Some("branch")
            )
            .is_err()
        );
        assert!(
            changes::patch(
                &fixture.repo,
                "file.txt",
                &Mode::Branch,
                Some("main"),
                Some("unstaged")
            )
            .is_err()
        );
    }
);

isolated!(local_layers_counts_and_literal_paths, |fixture| {
    fixture.write("file.txt", "base\n");
    fixture.write("deleted.txt", "gone\n");
    fixture.commit();
    fixture.write("file.txt", "base\nstaged\n");
    fixture.git(&["add", "file.txt"]);
    fixture.write("file.txt", "base\nstaged\nunstaged\n");
    fs::remove_file(PathBuf::from(&fixture.repo).join("deleted.txt")).unwrap();
    fixture.write("untracked.txt", "first\nsecond\n");
    fixture.write(":(glob)*", "literal\n");
    let listing = changes::list(&fixture.repo, &Mode::Local, None).unwrap();
    let files = listing["files"].as_array().unwrap();
    for layer in ["staged", "unstaged"] {
        let row = files
            .iter()
            .find(|r| r["path"] == "file.txt" && r["layer"] == layer)
            .unwrap();
        assert_eq!(row["added"], 1);
        assert_eq!(row["removed"], 0);
        let patch =
            changes::patch(&fixture.repo, "file.txt", &Mode::Local, None, Some(layer)).unwrap();
        assert_eq!(patch["added"], 1);
        assert!(
            patch["patch"]
                .as_str()
                .unwrap()
                .contains(&format!("+{layer}"))
        );
    }
    assert!(changes::patch(&fixture.repo, "file.txt", &Mode::Local, None, None).is_err());
    assert_eq!(
        files.iter().find(|r| r["path"] == "deleted.txt").unwrap()["removed"],
        1
    );
    let literal = changes::patch(&fixture.repo, ":(glob)*", &Mode::Local, None, None).unwrap();
    assert!(literal["patch"].as_str().unwrap().contains("+literal"));
    assert!(!literal["patch"].as_str().unwrap().contains("first"));
    assert_eq!(
        files.iter().find(|r| r["path"] == "untracked.txt").unwrap()["added"],
        2
    );
    fixture.write("large-untracked", &"long line\n".repeat(40_000));
    let large = changes::patch(&fixture.repo, "large-untracked", &Mode::Local, None, None).unwrap();
    assert_eq!(large["added"], 40_000);
    assert_eq!(large["is_truncated"], true);
    assert_eq!(large["full_lines"], 40_004);
    assert!(large["full_bytes"].as_u64().unwrap() > 400_000);
    fixture.write("no-newline", "last");
    let last = changes::patch(&fixture.repo, "no-newline", &Mode::Local, None, None).unwrap();
    assert!(
        last["patch"]
            .as_str()
            .unwrap()
            .contains("\\ No newline at end of file")
    );
    assert_eq!(
        last["full_bytes"].as_u64().unwrap(),
        last["patch"].as_str().unwrap().len() as u64
    );
});

isolated!(branch_comparison_caps_and_conflicts, |fixture| {
    fixture.write("file.txt", "base\n");
    fixture.commit();
    fixture.git(&["switch", "-qc", "feature"]);
    fixture.write("file.txt", &"new\n".repeat(1000));
    fixture.commit();
    let patch =
        changes::patch(&fixture.repo, "file.txt", &Mode::Branch, Some("main"), None).unwrap();
    assert_eq!(patch["added"], 1000);
    assert_eq!(patch["removed"], 1);
    assert_eq!(patch["is_truncated"], true);
    assert!(patch["full_lines"].as_u64().unwrap() > 1000);
    assert!(patch["patch"].as_str().unwrap().lines().count() <= 200);
    assert!(changes::list(&fixture.repo, &Mode::Branch, Some("--output=/tmp/escape")).is_err());
    fixture.git(&["switch", "-q", "main"]);
    fixture.write("file.txt", "other\n");
    fixture.commit();
    assert!(
        !lifecycle::git(&fixture.repo, &["merge", "feature"])
            .unwrap()
            .status
            .success()
    );
    let listing = changes::list(&fixture.repo, &Mode::Local, None).unwrap();
    assert_eq!(listing["files"][0]["layer"], "conflicted");
    let patch = changes::patch(
        &fixture.repo,
        "file.txt",
        &Mode::Local,
        None,
        Some("conflicted"),
    )
    .unwrap();
    assert!(patch["patch"].as_str().unwrap().contains("diff --cc"));
    assert_eq!(patch["added"], listing["files"][0]["added"]);
    assert_eq!(patch["removed"], listing["files"][0]["removed"]);
});

isolated!(
    changes_reject_head_and_base_changes_during_reads,
    |fixture| {
        use std::os::unix::fs::PermissionsExt;
        fixture.write("file.txt", "base\n");
        fixture.commit();
        fixture.git(&["switch", "-qc", "feature"]);
        fixture.write("file.txt", "feature\n");
        fixture.commit();
        fixture.git(&[
            "-c",
            "tag.gpgsign=false",
            "tag",
            "-a",
            "-m",
            "base",
            "tag-base",
            "main",
        ]);
        let tag = fixture.git(&["rev-parse", "tag-base"]);
        fixture.git(&["update-ref", "refs/remotes/tag-base", &tag]);
        let patch = changes::patch(
            &fixture.repo,
            "file.txt",
            &Mode::Branch,
            Some("refs/remotes/tag-base"),
            None,
        )
        .unwrap();
        assert_eq!(patch["added"], 1);
        let wrapper = fixture.root.join("bin/git");
        fs::write(&wrapper, "#!/bin/sh\n/usr/bin/git \"$@\"\nresult=$?\nif [ \"$4\" = diff ]; then /usr/bin/git -C \"$2\" update-ref refs/heads/main HEAD; fi\nexit \"$result\"\n").unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            changes::patch(&fixture.repo, "file.txt", &Mode::Branch, Some("main"), None).is_err()
        );
        fixture.write("file.txt", "local\n");
        fs::write(&wrapper, "#!/bin/sh\n/usr/bin/git \"$@\"\nresult=$?\nif [ \"$4\" = diff ]; then /usr/bin/git -C \"$2\" symbolic-ref HEAD refs/heads/main; fi\nexit \"$result\"\n").unwrap();
        assert!(changes::patch(&fixture.repo, "file.txt", &Mode::Local, None, None).is_err());
    }
);

isolated!(
    preview_refuses_symlinks_traversal_binary_and_large_files,
    |fixture| {
        fixture.write("text", "hello\n");
        let file = changes::file(&fixture.repo, "text").unwrap();
        assert_eq!(file["preview"]["text"], "hello\n");
        fs::write(fixture.root.join("secret"), "private").unwrap();
        std::os::unix::fs::symlink(
            fixture.root.join("secret"),
            PathBuf::from(&fixture.repo).join("escape"),
        )
        .unwrap();
        std::os::unix::fs::symlink(&fixture.root, PathBuf::from(&fixture.repo).join("parent"))
            .unwrap();
        for path in ["escape", "parent/secret"] {
            assert!(changes::file(&fixture.repo, path).unwrap()["notice"].is_string());
        }
        for path in ["../secret", "/etc/passwd", ".git/config", "a//b"] {
            assert!(changes::file(&fixture.repo, path).is_err());
        }
        fixture.write("binary", "a\0b");
        fixture.write("large", &"x".repeat(256 * 1024 + 1));
        for path in ["binary", "large"] {
            assert!(changes::file(&fixture.repo, path).unwrap()["notice"].is_string());
        }
    }
);

isolated!(
    github_panels_use_standin_and_reject_changed_identity,
    |fixture| {
        fixture.git(&[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "base",
        ]);
        let head = fixture.git(&["rev-parse", "HEAD"]);
        let pr = json!({"number":7,"title":"Fix Sidebar","state":"OPEN","isDraft":false,"baseRefName":"main","baseRefOid":head,
        "headRefName":"main","headRefOid":head,"headRepository":{"nameWithOwner":"example/project"},"reviewDecision":"APPROVED",
        "url":"https://github.com/example/project/pull/7","statusCheckRollup":[{"name":"tests","status":"COMPLETED","conclusion":"SUCCESS"}]});
        fs::write(fixture.root.join("pr-list"), json!([pr]).to_string()).unwrap();
        fs::write(fixture.root.join("pr-view"), pr.to_string()).unwrap();
        let panel = workspace_panel::pr(&fixture.repo).unwrap();
        assert_eq!(panel["pullRequest"]["number"], 7);
        assert_eq!(panel["checks"][0]["conclusion"], "SUCCESS");
        assert_eq!(panel["differsFromLocalHead"], false);
        let mut changed = pr;
        changed["headRefOid"] = "other".into();
        fs::write(fixture.root.join("pr-view"), changed.to_string()).unwrap();
        assert!(workspace_panel::pr(&fixture.repo).is_err());
        assert_eq!(
            workspace_panel::issues(&fixture.repo).unwrap()["issues"][0]["number"],
            7
        );
        assert_eq!(
            workspace_panel::issue(&fixture.repo, 7).unwrap()["body"],
            "Use less space"
        );
        assert!(workspace_panel::issue(&fixture.repo, 8).is_err());
        assert!(workspace_panel::issue(&fixture.repo, 0).is_err());
        assert!(
            fs::read_to_string(fixture.root.join("gh-calls"))
                .unwrap()
                .contains("pr view 7 --repo example/project --json")
        );
    }
);

isolated!(
    setup_choice_preserves_defaults_and_workspace_creation_keeps_script,
    |fixture| {
        let home = fixture.root.join(".swarm");
        fs::create_dir(&home).unwrap();
        fs::write(home.join("swarm-home"), "fixture").unwrap();
        let mut owner = OwnerChoices::default();
        owner.project_paths.push(fixture.repo.clone());
        owner
            .project_defaults
            .insert(fixture.repo.clone(), json!({"custom":"keep"}));
        let mut snapshot = json!({"sessions":[],"agents":{}});
        Discovery::default().augment(&mut snapshot, &owner, 100);
        let tree = tree::build(&snapshot, &owner);
        let mut state = ViewState::default();
        let op = serde_json::from_value(
            json!({"op":"setup_script","project":fixture.repo,"setup_script":"printf ready"}),
        )
        .unwrap();
        choices::apply(&mut owner, &mut state, &tree, op, 100).unwrap();
        let request = serde_json::from_value(json!({"project":fixture.repo,"name":"task","mode":"new-branch","prefix":"","folder":"../workspaces"})).unwrap();
        let (_, defaults) =
            lifecycle::create_workspace(&home, &tree.projects[0], &owner, &request).unwrap();
        assert_eq!(defaults["custom"], "keep");
        assert_eq!(defaults["setup_script"], "printf ready");
        let unknown = serde_json::from_value(
            json!({"op":"setup_script","project":"/unlisted","setup_script":"true"}),
        )
        .unwrap();
        assert!(choices::apply(&mut owner, &mut state, &tree, unknown, 100).is_err());
    }
);
