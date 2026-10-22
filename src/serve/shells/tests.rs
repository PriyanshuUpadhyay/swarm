use super::*;

/// The ids `parse` finds for workspace `cwd`, which holds the workspaces `nested`.
fn ids(text: &str, cwd: &str, nested: &[&str]) -> Vec<String> {
    let nested: Vec<String> = nested.iter().map(|n| n.to_string()).collect();
    parse(text, cwd, &nested)
        .into_iter()
        .map(|shell| shell.id)
        .collect()
}

/// `n` shells that run `zsh`.
fn shells(n: usize) -> Vec<Shell> {
    (0..n)
        .map(|i| Shell {
            id: format!("shell-{i:08x}"),
            command: "zsh".into(),
        })
        .collect()
}

#[test]
fn a_folder_lists_only_its_own_shells_oldest_first() {
    let text = "shell-0000000b|20|vim|/w/this\n\
                shell-0000000a|10|zsh|/w/this\n\
                shell-0000000c|5|zsh|/w/other\n\
                shell-0000000d|5|zsh|/w/this|b\n\
                swarm_x_orchestrator|1|claude|/w/this\n\
                shell-../etc|1|zsh|/w/this\n";
    assert_eq!(
        parse(text, "/w/this", &[]),
        [
            Shell {
                id: "shell-0000000a".into(),
                command: "zsh".into()
            },
            Shell {
                id: "shell-0000000b".into(),
                command: "vim".into()
            },
        ]
    );
    assert!(!valid_id("shell-0000000"));
    assert!(!valid_id("=shell-0000000a"));
}

#[test]
fn a_workspace_lists_the_shells_of_the_folders_inside_it() {
    let text = "shell-0000000a|1|zsh|/w/repo/sub\n\
                shell-0000000b|2|zsh|/w/repo\n\
                shell-0000000c|3|zsh|/w/repo-two\n";
    assert_eq!(
        ids(text, "/w/repo", &[]),
        ["shell-0000000a", "shell-0000000b"]
    );
    assert_eq!(ids(text, "/w/repo/sub", &[]), ["shell-0000000a"]);
}

#[test]
fn a_shell_belongs_to_the_deepest_workspace_that_holds_its_folder() {
    let text = "shell-0000000a|1|zsh|/w/repo/wt/feature\n\
                shell-0000000b|2|zsh|/w/repo/wt/feature/deep\n\
                shell-0000000c|3|zsh|/w/repo\n\
                shell-0000000d|4|zsh|/w/repo/other\n";
    assert_eq!(
        ids(text, "/w/repo", &["/w/repo/wt/feature"]),
        ["shell-0000000c", "shell-0000000d"]
    );
    assert_eq!(
        ids(text, "/w/repo/wt/feature", &[]),
        ["shell-0000000a", "shell-0000000b"]
    );
}

#[test]
fn two_creates_cannot_both_pass_the_cap() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static MADE: AtomicUsize = AtomicUsize::new(0);
    let threads: Vec<_> = (0..MAX_SHELLS * 2)
        .map(|_| {
            std::thread::spawn(|| {
                create_with_room(
                    || Ok(()),
                    || Ok(shells(MADE.load(Ordering::SeqCst))),
                    || {
                        // A real create takes time, so a second request counts before it ends.
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        MADE.fetch_add(1, Ordering::SeqCst);
                        Ok("shell-00000000".into())
                    },
                )
                .is_ok()
            })
        })
        .collect();
    let passed = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert_eq!(passed, MAX_SHELLS);
    assert_eq!(MADE.load(Ordering::SeqCst), MAX_SHELLS);
}

#[test]
fn a_ninth_shell_in_a_workspace_is_refused() {
    assert!(check_room(&shells(MAX_SHELLS - 1)).is_ok());
    let full = check_room(&shells(MAX_SHELLS)).unwrap_err();
    assert_eq!(full.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_chat_in_a_folder_of_an_archived_workspace_starts_no_shell() {
    use crate::serve::test_support::Fixture;
    let fixture = Fixture::new();
    let inner = fixture.chat_in_archived_workspace();
    let request = ShellRequest {
        session: inner,
        id: None,
    };
    let refused = shells_create(State(fixture.app.clone()), Json(request))
        .await
        .unwrap_err();
    assert_eq!(refused.0, StatusCode::CONFLICT);
    assert!(
        refused.1.0["error"]
            .as_str()
            .unwrap()
            .contains("start a shell")
    );
}

#[tokio::test]
async fn an_archived_workspace_starts_no_shell() {
    use crate::serve::test_support::Fixture;
    let fixture = Fixture::new();
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"))
        .to_string_lossy()
        .into_owned();
    choices_store::update(&fixture.app.root, None, |owner, _| {
        owner.archived.insert(cwd);
        Ok(())
    })
    .unwrap();
    let request = ShellRequest {
        session: fixture.session.clone(),
        id: None,
    };
    let refused = shells_create(State(fixture.app.clone()), Json(request))
        .await
        .unwrap_err();
    assert_eq!(refused.0, StatusCode::CONFLICT);
    assert!(
        refused.1.0["error"]
            .as_str()
            .unwrap()
            .contains("start a shell")
    );
}

#[test]
fn a_failed_read_is_not_an_unknown_session() {
    use crate::serve::test_support::Fixture;
    let fixture = Fixture::new();
    let missing = session_cwd(&fixture.app, "no-such-session").unwrap_err();
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    swarm::store::open(&fixture.app.root.join("swarm.db"))
        .unwrap()
        .execute_batch("PRAGMA foreign_keys = OFF; ALTER TABLE session RENAME TO moved;")
        .unwrap();
    let failed = session_cwd(&fixture.app, &fixture.session).unwrap_err();
    assert_eq!(failed.0, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn the_read_only_check_runs_while_the_lock_is_held() {
    // An archive closes shells under the same lock, so the check and the create cannot straddle it.
    let made = create_with_room(
        || {
            assert!(SHELLS.try_lock().is_err());
            Ok(())
        },
        || Ok(Vec::new()),
        || Ok("shell-00000000".into()),
    );
    assert!(made.is_ok());
    let refused = create_with_room(
        || Err(error(StatusCode::CONFLICT, "read-only")),
        || panic!("a refused create counts no shells"),
        || panic!("a refused create makes no shell"),
    );
    assert_eq!(refused.unwrap_err().0, StatusCode::CONFLICT);
}

#[test]
fn a_tmux_call_that_hangs_is_killed_and_returns_an_error() {
    let started = std::time::Instant::now();
    let mut hang = std::process::Command::new("sleep");
    hang.arg("30");
    let err = run(hang, std::time::Duration::from_millis(200)).unwrap_err();
    assert!(err.contains("did not answer"), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let mut quick = std::process::Command::new("echo");
    quick.arg("fine");
    let output = run(quick, std::time::Duration::from_secs(5)).unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "fine");
}

#[test]
fn every_chat_of_a_workspace_uses_the_workspace_for_its_shells() {
    use crate::serve::test_support::Fixture;
    let fixture = Fixture::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .to_string_lossy()
        .into_owned();
    let inner = format!("{root}/src");
    assert_eq!(workspace(&fixture.app, &inner).unwrap().0, root);
    assert_eq!(workspace(&fixture.app, &root).unwrap().0, root);
    // A folder in no workspace is its own.
    assert_eq!(
        workspace(&fixture.app, "/no/such/folder").unwrap().0,
        "/no/such/folder"
    );
}

#[test]
fn an_error_reads_as_its_text_without_quote_marks() {
    let err = error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed");
    assert_eq!(api_text(&err), "database read failed");
}

#[test]
fn a_close_of_a_dropped_workspace_ends_only_its_own_shells() {
    let workspace = |path: &str| tree::Workspace {
        path: path.into(),
        name: path.into(),
        branch: None,
        missing: false,
        mark: None,
        removed: false,
        chats: Vec::new(),
        unlisted: Vec::new(),
        archived: Vec::new(),
    };
    // The worktree /w/repo/wt/feature is already gone from the tree; its parent is left.
    let tree = tree::Tree {
        projects: vec![tree::Project {
            id: "p".into(),
            path: "/w/repo".into(),
            name: "repo".into(),
            main_path: None,
            launch_directory: "/w/repo".into(),
            workspaces: vec![workspace("/w/repo")],
        }],
        importable: Vec::new(),
    };
    let text = "shell-0000000a|1|zsh|/w/repo/wt/feature\n\
                shell-0000000b|2|zsh|/w/repo\n";
    let (cwd, inner) = close_scope(&tree, "/w/repo/wt/feature");
    let inner: Vec<&str> = inner.iter().map(String::as_str).collect();
    assert_eq!(ids(text, &cwd, &inner), ["shell-0000000a"]);
    // A chat's list keeps the widening: the folder belongs to the deepest workspace that holds it.
    let (cwd, inner) = workspace_in(&tree, "/w/repo/wt/feature");
    let inner: Vec<&str> = inner.iter().map(String::as_str).collect();
    assert_eq!(
        ids(text, &cwd, &inner),
        ["shell-0000000a", "shell-0000000b"]
    );
}

/// Kills its shells when it drops, so a failed assert leaves none on the test socket.
struct KillOnDrop(Vec<String>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        for id in &self.0 {
            let _ = kill(id);
        }
    }
}

#[test]
fn close_all_ends_only_the_shells_of_the_folder_it_is_given() {
    if std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .is_err()
    {
        eprintln!("skipped: no tmux");
        return;
    }
    use crate::serve::test_support::Fixture;
    let fixture = Fixture::new();
    let root = env!("CARGO_MANIFEST_DIR");
    // The tree holds the crate root as a workspace and no workspace for `src`.
    let src = format!("{root}/src");
    let mut shells = KillOnDrop(Vec::new());
    let in_root = create(root).unwrap();
    shells.0.push(in_root.clone());
    let in_src = create(&src).unwrap();
    shells.0.push(in_src.clone());
    close_all(&fixture.app, &src);
    // Other tests share the socket, so this looks for its own ids.
    let left: Vec<String> = list_in(root, &[])
        .unwrap()
        .into_iter()
        .map(|shell| shell.id)
        .collect();
    assert!(left.contains(&in_root), "the root shell ended: {left:?}");
    assert!(!left.contains(&in_src), "the src shell is alive: {left:?}");
}
