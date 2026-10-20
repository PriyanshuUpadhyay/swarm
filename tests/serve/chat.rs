// Tests for chat.

use super::*;

#[test]
fn chat_posts_are_guarded_before_any_action_runs() {
    let server = Server::with_actions(true);
    let body =
        json!({"session": server.session, "agent": "chair", "text": "must not run"}).to_string();
    for (headers, auth, expected) in [
        ("Content-Type: application/json\r\n", false, 401),
        (
            "Content-Type: application/json\r\nOrigin: https://evil.test\r\n",
            true,
            403,
        ),
        ("Content-Type: text/plain\r\n", true, 415),
        // Only the image route takes an image body.
        ("Content-Type: image/png\r\n", true, 415),
    ] {
        assert_eq!(
            server
                .request("POST", "/api/chat/send", headers, &body, auth)
                .0,
            expected
        );
    }
    for (headers, auth, expected) in [
        ("Content-Type: image/png\r\n", false, 401),
        (
            "Content-Type: image/png\r\nOrigin: https://evil.test\r\n",
            true,
            403,
        ),
        ("Content-Type: text/plain\r\n", true, 415),
        ("Content-Type: image/png\r\n", true, 200),
    ] {
        assert_eq!(
            server
                .request("POST", "/api/chat/image", headers, "PNG", auth)
                .0,
            expected
        );
    }
    let oversized =
        json!({"session":server.session,"agent":"chair","text":"x".repeat(65536)}).to_string();
    assert_eq!(
        server
            .request(
                "POST",
                "/api/chat/send",
                "Content-Type: application/json\r\n",
                &oversized,
                true
            )
            .0,
        413
    );
    assert!(!server.dir.join("home/actions").exists());
}

#[test]
fn chat_files_list_only_the_chat_folder_for_the_at_menu() {
    let server = Server::with_actions(true);
    let repo = server.dir.join("repo");
    fs::create_dir_all(repo.join("sub")).unwrap();
    for (path, text) in [
        ("a.txt", "a"),
        ("sub/b.rs", "b"),
        ("ignored.log", "x"),
        (".gitignore", "*.log\n"),
    ] {
        fs::write(repo.join(path), text).unwrap();
    }
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&["init", "-q"]);
    git(&["add", "a.txt", ".gitignore"]);
    fs::write(repo.join("new.md"), "untracked").unwrap();
    let connection = swarm::store::open(&server.dir.join("home/.swarm/swarm.db")).unwrap();
    let set_cwd = |cwd: &std::path::Path| {
        connection
            .execute(
                "UPDATE session SET cwd = ?1 WHERE id = ?2",
                (cwd.to_string_lossy().as_ref(), &server.session),
            )
            .unwrap();
    };
    let files = |session: &str| {
        let (status, _, body) = server.request(
            "GET",
            &format!("/api/chat/files?session={session}&agent=chair"),
            "",
            "",
            true,
        );
        (
            status,
            serde_json::from_str::<Value>(&body).unwrap_or(Value::Null),
        )
    };
    set_cwd(&repo);
    let (status, listed) = files(&server.session);
    assert_eq!(status, 200, "{listed}");
    assert_eq!(
        listed,
        json!({"files": [".gitignore", "a.txt", "new.md", "sub/b.rs"], "note": null})
    );
    // A folder git cannot list gives an empty menu with the reason, not an error.
    set_cwd(&server.dir.join("missing"));
    let (status, listed) = files(&server.session);
    assert_eq!(status, 200);
    assert_eq!(listed["files"], json!([]));
    assert!(listed["note"].as_str().unwrap().contains("Git cannot list"));
    assert_eq!(files("no-such-session").0, 404);
}
