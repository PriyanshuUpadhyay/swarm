// Tests for workspace.

use super::*;

#[test]
fn changes_routes_read_a_large_file_in_both_modes() {
    let server = Server::configured(false, true);
    let project = server.dir.join("project");
    let git = |args: &[&str]| {
        let output = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(&project)
            .args(args)
            .env("GIT_CEILING_DIRECTORIES", &server.dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
    };
    git(&["config", "user.name", "Fixture"]);
    git(&["config", "user.email", "fixture@example.com"]);
    fs::write(project.join("large.txt"), "base\n").unwrap();
    git(&["add", "."]);
    git(&["-c", "commit.gpgsign=false", "commit", "-qm", "base"]);
    git(&["switch", "-qc", "feature"]);
    fs::write(project.join("large.txt"), "new\n".repeat(20_000)).unwrap();
    git(&["add", "."]);
    git(&["-c", "commit.gpgsign=false", "commit", "-qm", "large"]);
    fs::write(project.join("large.txt"), "local\n".repeat(20_000)).unwrap();
    for (mode, layer) in [("local", "unstaged"), ("branch", "branch")] {
        for route in ["/api/changes", "/api/changes/diff"] {
            let query = format!(
                "{route}?workspace={}&mode={mode}&base=main&path=large.txt&layer={layer}",
                project.display(),
            );
            let (status, _, body) = server.request("GET", &query, "", "", true);
            assert_eq!(status, 200, "{mode} {route}: {body}");
            if route.ends_with("/diff") {
                let patch: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(patch["added"], 20_000);
                assert_eq!(patch["is_truncated"], true);
            }
        }
    }
}

#[test]
fn workspace_lifecycle_http_refuses_unlisted_paths_before_any_change() {
    let server = Server::configured(true, true);
    let outside = server.dir.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep").unwrap();
    let root = server.dir.join("home/.swarm");
    let before = fs::read(root.join("choices.json")).ok();
    for (route, key) in [("new-options", "project"), ("impact", "workspace")] {
        assert_eq!(
            server
                .request(
                    "GET",
                    &format!("/api/workspace/{route}?{key}={}", outside.display()),
                    "",
                    "",
                    true
                )
                .0,
            403
        );
    }
    for route in ["new", "archive", "restore", "delete", "prune"] {
        let body = if route == "new" {
            json!({"project":outside,"name":"Escape","mode":"new-branch"})
        } else {
            json!({"workspace":outside})
        };
        let (status, _, body) = server.request(
            "POST",
            &format!("/api/workspace/{route}"),
            "Content-Type: application/json\r\n",
            &body.to_string(),
            true,
        );
        assert_eq!(status, 403, "{route}: {body}");
    }
    assert_eq!(fs::read(root.join("choices.json")).ok(), before);
    assert_eq!(
        fs::read_to_string(outside.join("keep.txt")).unwrap(),
        "keep"
    );
    assert!(!server.dir.join("home/actions").exists());

    let shown = json!({"text":"Delete failed","page":"/"}).to_string();
    let (status, ..) = server.request(
        "POST",
        "/api/ui-error",
        "Content-Type: application/json\r\n",
        &shown,
        true,
    );
    assert_eq!(status, 204);
    let entries: Vec<Value> = fs::read_to_string(root.join("errors.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let delete = entries
        .iter()
        .find(|e| e["uri"] == "/api/workspace/delete")
        .unwrap();
    assert_eq!(delete["status"], 403);
    assert_eq!(delete["request"]["workspace"], json!(outside));
    assert!(delete["reply"]["error"].is_string());
    let last = entries.last().unwrap();
    assert_eq!(
        (&last["source"], &last["text"]),
        (&json!("ui"), &json!("Delete failed"))
    );
}

#[test]
fn workspace_panels_http_refuse_unlisted_paths_and_keep_existing_guards() {
    let server = Server::configured(false, true);
    let outside = server.dir.join("outside");
    fs::create_dir(&outside).unwrap();
    for route in [
        "/api/changes",
        "/api/changes/diff",
        "/api/changes/file",
        "/api/pr",
        "/api/issues",
    ] {
        let key = if route == "/api/issues" {
            "project"
        } else {
            "workspace"
        };
        let path = format!("{route}?{key}={}&path=keep.txt", outside.display());
        // Issues accept only their project field.
        let path = if route == "/api/issues" {
            format!("{route}?{key}={}", outside.display())
        } else {
            path
        };
        assert_eq!(server.request("GET", &path, "", "", true).0, 403, "{route}");
        assert_eq!(
            server.request("GET", &path, "", "", false).0,
            401,
            "{route}"
        );
        assert_eq!(
            server
                .request("GET", &path, "Origin: https://evil.test\r\n", "", true)
                .0,
            403,
            "{route}"
        );
    }
    for route in ["/api/usage", "/api/usage/refresh"] {
        let method = if route.ends_with("refresh") {
            "POST"
        } else {
            "GET"
        };
        assert_eq!(
            server
                .request(
                    method,
                    route,
                    "Content-Type: application/json\r\n",
                    r#"{"provider":"claude"}"#,
                    false
                )
                .0,
            401
        );
    }
    assert_eq!(
        server
            .request(
                "POST",
                "/api/workspace/new",
                "Content-Type: application/json\r\n",
                &json!({"project":outside,"mode":"issue","issue":7}).to_string(),
                true
            )
            .0,
        403
    );
    assert!(!server.dir.join("home/gh-calls").exists());
}

#[test]
fn project_remove_http_refuses_unlisted_path_and_keeps_folder() {
    let server = Server::configured(true, true);
    let outside = server.dir.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "keep").unwrap();
    let root = server.dir.join("home/.swarm");
    let before = fs::read(root.join("choices.json")).ok();
    let (status, _, body) = server.request(
        "POST",
        "/api/project/remove",
        "Content-Type: application/json\r\n",
        &json!({"project":outside}).to_string(),
        true,
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(fs::read(root.join("choices.json")).ok(), before);
    assert_eq!(
        fs::read_to_string(outside.join("keep.txt")).unwrap(),
        "keep"
    );
    assert!(!server.dir.join("home/actions").exists());
}

#[test]
fn project_create_import_http_refuse_swarm_home_symlink_and_keep_data() {
    let server = Server::configured(true, true);
    let root = server.dir.join("home/.swarm");
    let link = server.dir.join("home-link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    fs::write(root.join("keep.txt"), "keep").unwrap();
    let before = fs::read(root.join("choices.json")).ok();
    for (route, value) in [
        (
            "create",
            json!({"path":link.join("escape"),"name":"Escape"}),
        ),
        ("import", json!({"path":link,"as":"git"})),
    ] {
        let path = format!("/api/project/{route}");
        assert_eq!(
            server
                .request(
                    "POST",
                    &path,
                    "Content-Type: application/json\r\n",
                    &value.to_string(),
                    false
                )
                .0,
            401
        );
        assert_eq!(
            server
                .request(
                    "POST",
                    &path,
                    "Content-Type: application/json\r\nOrigin: https://evil.test\r\n",
                    &value.to_string(),
                    true
                )
                .0,
            403
        );
        let (status, _, body) = server.request(
            "POST",
            &path,
            "Content-Type: application/json\r\n",
            &value.to_string(),
            true,
        );
        assert_eq!(status, 403, "{body}");
    }
    assert_eq!(fs::read(root.join("choices.json")).ok(), before);
    assert_eq!(fs::read_to_string(root.join("keep.txt")).unwrap(), "keep");
    assert!(!root.join("escape").exists());
    assert!(!root.join(".git").exists());
    assert!(!server.dir.join("home/actions").exists());
}
