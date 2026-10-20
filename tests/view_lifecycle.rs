use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use swarm::view::{
    choices::{OwnerChoices, ViewState},
    discovery::{self, Discovery},
    lifecycle::{self, FirstCommit},
    tree::{self, Project},
};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    repo: String,
    owner: OwnerChoices,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::var("HOME").unwrap()).unwrap();
        let home = root.join(".swarm");
        fs::create_dir(&home).unwrap();
        fs::write(home.join("swarm-home"), "fixture").unwrap();
        let repo = root.join("project").to_string_lossy().into_owned();
        fs::create_dir(&repo).unwrap();
        lifecycle::check_git(&repo, &["init", "-q", "-b", "main"]).unwrap();
        let mut owner = OwnerChoices::default();
        owner.project_paths.push(repo.clone());
        Self {
            root,
            home,
            repo,
            owner,
        }
    }
    fn git(&self, args: &[&str]) -> String {
        lifecycle::check_git(&self.repo, args).unwrap()
    }
    fn commit(&self) {
        self.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "base",
        ]);
    }
    fn project(&self) -> Project {
        let mut snapshot = json!({"sessions":[],"agents":{}});
        Discovery::default().augment(&mut snapshot, &self.owner, 100);
        tree::build(&snapshot, &self.owner).projects.remove(0)
    }
    fn request(&self, name: &str, extra: Value) -> lifecycle::NewWorkspace {
        let mut value = json!({"project":self.repo,"name":name,"folder":"../workspaces","prefix":"swarm/","mode":"new-branch","base":"main"});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }
    fn create(&self, name: &str, extra: Value) -> String {
        lifecycle::create_workspace(
            &self.home,
            &self.project(),
            &self.owner,
            &self.request(name, extra),
        )
        .unwrap()
        .0
    }
}
macro_rules! isolated {
    ($name:ident, |$fixture:ident| $body:block) => {
        #[test]
        fn $name() {
            if std::env::var("SWARM_LIFECYCLE_CASE").ok().as_deref() != Some(stringify!($name)) {
                let root = std::env::temp_dir().join(format!("swarm-lifecycle-{}",uuid::Uuid::now_v7()));
                fs::create_dir_all(root.join("bin")).unwrap();
                use std::os::unix::fs::PermissionsExt;
                fs::write(root.join("bin/gh"), "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/gh-calls\"\nif [ -f \"$HOME/gh-fail\" ]; then exit 1; fi\nprintf '{\"number\":7}\\n'\n").unwrap();
                fs::set_permissions(root.join("bin/gh"),fs::Permissions::from_mode(0o755)).unwrap();
                let output = Command::new(std::env::current_exe().unwrap()).args(["--exact",stringify!($name),"--nocapture"])
                    .env_clear().env("HOME",&root).env("PATH",format!("{}:/usr/bin:/bin",root.join("bin").display()))
                    .env("GIT_CEILING_DIRECTORIES",fs::canonicalize(&root).unwrap()).env("GIT_CONFIG_NOSYSTEM","1").env("GIT_CONFIG_GLOBAL",root.join(".gitconfig"))
                    .env("SWARM_LIFECYCLE_CASE",stringify!($name)).output().unwrap();
                fs::remove_dir_all(root).unwrap();
                assert!(output.status.success(),"{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
                return;
            }
            let mut $fixture = Fixture::new();
            let _ = &mut $fixture;
            $body
        }
    }
}

#[test]
fn validates_branch_names() {
    assert_eq!(
        lifecycle::branch_name("Fix Sidebar!", "swarm/").as_deref(),
        Some("swarm/fix-sidebar")
    );
    assert_eq!(lifecycle::branch_name("!!!", "").as_deref(), Some("task"));
    assert!(lifecycle::branch_name(" ", "swarm/").is_none());
    assert_eq!(
        lifecycle::branch_name(&"x".repeat(60), "").unwrap(),
        "x".repeat(40)
    );
    for prefix in [
        "swarm/",
        "",
        "-",
        "/",
        "bad//",
        ".hidden/",
        "name.lock/",
        "bad../",
        "bad@{/",
        "bad /",
        "bad\\/",
        "bad~/",
        "bad^/",
        "bad:/",
        "bad?/",
        "bad*/",
        "bad[/",
        "bad\n/",
        "é/",
    ] {
        let status = Command::new("git")
            .args([
                "check-ref-format",
                "--branch",
                &format!("{prefix}fix-sidebar"),
            ])
            .output()
            .unwrap()
            .status;
        assert_eq!(
            lifecycle::branch_name("Fix Sidebar", prefix).is_some(),
            status.success(),
            "{prefix}"
        );
    }
    assert_eq!(
        lifecycle::branch_name("A", " fix/ ").as_deref(),
        Some("fix/a")
    );
}

#[test]
fn listing_preserves_paths_branches_and_worktree_states() {
    let entries = discovery::parse_worktrees(b"worktree /hub/.bare\0bare\0\0worktree /tasks/space and\nnewline\0HEAD deadbeef\0branch refs/heads/fix/sidebar\0locked explanation\0\0worktree /tasks/missing\0detached\0prunable gone\0worktree /tasks/other\0branch refs/heads/main\0");
    assert_eq!(entries.len(), 4);
    assert!(entries[0].bare);
    assert_eq!(entries[1].path, "/tasks/space and\nnewline");
    assert_eq!(entries[1].branch.as_deref(), Some("fix/sidebar"));
    assert!(entries[1].locked);
    assert!(entries[2].detached && entries[2].missing);
    assert_eq!(entries[3].branch.as_deref(), Some("main"));
    for bytes in [b"".as_slice(), b"\0\0", b"HEAD deadbeef\0"] {
        assert!(discovery::parse_worktrees(bytes).is_empty());
    }
}

#[test]
fn first_commit_notices_include_the_required_next_action() {
    assert!(FirstCommit::Made.notice().is_none());
    assert_eq!(
        FirstCommit::SkippedNoIdentity.notice().unwrap(),
        "Created without a first commit, because git has no user.name and user.email. Set them, then commit once; until then workspaces start orphan branches."
    );
    assert_eq!(
        FirstCommit::Failed("Hook refused".into()).notice().unwrap(),
        "Created without a first commit. Hook refused"
    );
}
include!("view_lifecycle/workspace_creation.rs");
isolated!(initialization_and_shared_history, |f| {
    fs::write(
        f.root.join(".gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
    )
    .unwrap();
    assert_eq!(
        lifecycle::initialize_project(&f.repo).unwrap(),
        FirstCommit::Made
    );
    assert_eq!(f.git(&["log", "--format=%s"]), "Create project");
    assert_eq!(f.git(&["ls-tree", "--name-only", "HEAD"]), ".gitignore");
    fs::create_dir(PathBuf::from(&f.repo).join("tmp")).unwrap();
    fs::write(PathBuf::from(&f.repo).join("tmp/task"), "task").unwrap();
    assert!(f.git(&["status", "--porcelain"]).is_empty());
    let head = f.git(&["rev-parse", "HEAD"]);
    for name in ["First", "Second"] {
        let path = f.create(name, json!({}));
        assert_eq!(
            lifecycle::check_git(&path, &["rev-parse", "HEAD"]).unwrap(),
            head
        );
    }
});
isolated!(missing_identity_and_ignore_rules, |f| {
    for identity in ["", "name = Fixture", "email = fixture@example.com"] {
        fs::write(f.root.join(".gitconfig"), format!("[user]\n{identity}\n")).unwrap();
        assert_eq!(
            lifecycle::initialize_project(&f.repo).unwrap(),
            FirstCommit::SkippedNoIdentity
        );
        assert_eq!(
            fs::read_to_string(PathBuf::from(&f.repo).join(".gitignore")).unwrap(),
            "tmp/\n"
        );
        assert!(lifecycle::check_git(&f.repo, &["rev-parse", "--verify", "HEAD"]).is_err());
    }
    for original in ["build/", "build/\n", "build/\ntmp/\n", "tmp/\r\nbuild/\r\n"] {
        let path = PathBuf::from(&f.repo).join(".gitignore");
        fs::write(&path, original).unwrap();
        lifecycle::initialize_project(&f.repo).unwrap();
        let actual = fs::read_to_string(&path).unwrap();
        assert!(actual.starts_with(original));
        assert_eq!(actual.lines().filter(|s| *s == "tmp/").count(), 1);
    }
});
isolated!(rejected_first_commit_keeps_project, |f| {
    fs::write(
        f.root.join(".gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n",
    )
    .unwrap();
    let hook = PathBuf::from(&f.repo).join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'Hook refused' >&2\nexit 1\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(lifecycle::initialize_project(&f.repo).unwrap(),FirstCommit::Failed(message) if message.contains("Hook refused"))
    );
    assert!(PathBuf::from(&f.repo).join(".gitignore").exists());
});
include!("view_lifecycle/workspace_removal.rs");
#[test]
fn archive_reads_all_sessions_before_any_close_and_orders_children_first() {
    let sessions = vec![
        json!({"id":"current","adapter":"fixture","agents":2,"chair_id":"lead"}),
        json!({"id":"older","adapter":"fixture","agents":1}),
    ];
    let batch = json!({"current":[{"id":"lead","alive":true},{"id":"worker","alive":true},{"id":"dead","alive":false}]});
    let targets = lifecycle::live_targets(&sessions, Some(&batch), |s| {
        assert_eq!(s["id"], "older");
        Ok(vec![json!({"id":"orchestrator","alive":true})])
    })
    .unwrap();
    assert_eq!(
        targets,
        vec![
            ("current".into(), "worker".into()),
            ("current".into(), "lead".into()),
            ("older".into(), "orchestrator".into())
        ]
    );
    assert!(
        lifecycle::live_targets(&sessions, Some(&batch), |_| Err("unreadable".into()))
            .unwrap_err()
            .to_string()
            .contains("could not read")
    );
    let mut reads = 0;
    let targets = lifecycle::live_targets(&sessions, None, |_| {
        reads += 1;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(reads, 2);
    assert!(targets.is_empty());
    let zero = vec![
        json!({"id":"empty","adapter":"fixture","agents":0}),
        json!({"id":"legacy","agents":1}),
    ];
    let batch = json!({});
    let targets =
        lifecycle::live_targets(&zero, Some(&batch), |_| panic!("should not read")).unwrap();
    assert!(targets.is_empty());
    let mut reads = 0;
    lifecycle::live_targets(&zero, None, |_| {
        reads += 1;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(reads, 1);
    assert!(lifecycle::live_targets(&zero, None, |_| Err("failed".into())).is_err());
}
#[test]
fn raw_git_timeout_reaps_process() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 5 &"]);
    let start = std::time::Instant::now();
    assert!(discovery::run_command(&mut command, 1).is_err());
    assert!(start.elapsed().as_secs_f64() < 2.0);
}

#[test]
fn command_timeout_kills_the_waiting_child() {
    let root = std::env::temp_dir().join(format!("swarm-timeout-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&root).unwrap();
    let pid_file = root.join("pid");
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf '%s' \"$$\" > \"$1\"; exec sleep 5", "fixture"]);
    command.arg(&pid_file);
    let start = std::time::Instant::now();
    let error = discovery::run_command(&mut command, 1).unwrap_err();
    assert!(
        error.contains("timed out") || error.contains("Timeout"),
        "{error}"
    );
    assert!(start.elapsed().as_secs_f64() < 2.0);
    let pid: i32 = fs::read_to_string(pid_file).unwrap().parse().unwrap();
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // SAFETY: signal zero checks the recorded child without sending a signal.
    assert_eq!(unsafe { kill(pid, 0) }, -1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn command_keeps_both_outputs_and_exit_status() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf output; printf error >&2; exit 7"]);
    let output = discovery::run_command(&mut command, 1).unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"output");
    assert_eq!(output.stderr, b"error");
}
