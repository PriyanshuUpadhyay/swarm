// S6b wire tests use only HAND values and two equal OLD captures.
use super::*;

fn fixture(name: &str) -> Value {
    serde_json::from_str(match name {
        "hand" => include_str!("../fixtures/s6b/hand.json"),
        "small" => include_str!("../fixtures/s6b/small.json"),
        "local" => include_str!("../fixtures/s6b/local.json"),
        "branch" => include_str!("../fixtures/s6b/branch.json"),
        _ => panic!("unknown fixture"),
    })
    .unwrap()
}

fn git(server: &Server, args: &[&str]) {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(server.dir.join("project"))
        .args(args)
        .env("GIT_CEILING_DIRECTORIES", &server.dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

pub(super) fn capped_display(mode: &str) -> Value {
    fixture(if mode == "local" { "local" } else { "branch" })
}

#[test]
fn s6b_t1_d_diff_display_uses_the_public_git_route() {
    let server = Server::configured(false, true);
    let project = server.dir.join("project");
    git(&server, &["config", "user.name", "Fixture"]);
    git(&server, &["config", "user.email", "fixture@example.com"]);
    fs::write(project.join("small.txt"), "keep\nold one\nold two\ntail\n").unwrap();
    git(&server, &["add", "."]);
    git(
        &server,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "base"],
    );
    git(&server, &["switch", "-qc", "feature"]);
    fs::write(project.join("small.txt"), "keep\nnew one\ntail\n").unwrap();
    git(&server, &["add", "."]);
    git(
        &server,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "new"],
    );
    fs::write(project.join("binary.bin"), b"a\0b").unwrap();
    std::os::unix::fs::symlink("small.txt", project.join("link.txt")).unwrap();
    let mut got = Vec::new();
    for (path, mode, layer) in [
        ("small.txt", "branch", "branch"),
        ("binary.bin", "local", "untracked"),
        ("link.txt", "local", "untracked"),
    ] {
        let route = format!(
            "/api/changes/diff?workspace={}&mode={mode}&base=main&path={path}&layer={layer}",
            project.display()
        );
        let (status, _, body) = server.request("GET", &route, "", "", true);
        got.push(json!({"status":status,"body":serde_json::from_str::<Value>(&body).unwrap()}));
    }
    let notice = |path: &str| json!({"status":200,"body":{"path":path,"notice":"Binary file or symbolic link","added":null,"removed":null}});
    assert_eq!(
        json!(got),
        json!([
            {"status":200,"body":fixture("small")},
            notice("binary.bin"),
            notice("link.txt")
        ])
    );
}

#[test]
fn s6b_t1_c_comment_operations_use_the_real_post_route() {
    let server = Server::configured(false, true);
    let hand = fixture("hand");
    let mut got = Vec::new();
    let mut expected = Vec::new();
    for case in hand["ranges"].as_array().unwrap() {
        let (status, _, body) = server.request(
            "POST",
            "/api/changes/comment-range",
            "Content-Type: application/json\r\n",
            &json!({"op":"range","patch":hand["patch"],"from":case["from"],"to":case["to"]})
                .to_string(),
            true,
        );
        got.push(json!({"status":status,"body":serde_json::from_str::<Value>(&body).unwrap_or(json!(body))}));
        expected.push(json!({"status":200,"body":case["reply"]}));
    }
    let (status, _, body) = server.request(
        "POST",
        "/api/changes/comment-range",
        "Content-Type: application/json\r\n",
        &json!({"op":"message","comments":hand["comments"]}).to_string(),
        true,
    );
    got.push(
        json!({"status":status,"body":serde_json::from_str::<Value>(&body).unwrap_or(json!(body))}),
    );
    expected.push(json!({"status":200,"body":{"message":hand["message"]}}));
    assert_eq!(json!(got), json!(expected));
}

pub(super) fn comment_guards(server: &Server) {
    let path = "/api/changes/comment-range";
    let headers = "Content-Type: application/json\r\n";
    let valid = r#"{"op":"range","patch":"@@ -1 +1 @@\n+x","from":1,"to":1}"#;
    let mut got = Vec::new();
    let (status, challenge, _) = server.request("POST", path, headers, valid, false);
    got.push(json!({"status":status,"challenge":challenge.to_ascii_lowercase().contains("www-authenticate: bearer")}));
    got.push(json!(
        server
            .request(
                "POST",
                path,
                &format!("{headers}Origin: https://evil.test\r\n"),
                valid,
                true
            )
            .0
    ));
    for body in [
        "{",
        r#"{"op":"range","patch":"x","from":0,"to":0,"extra":true}"#,
        r#"{"op":"range","patch":"x","from":-1,"to":0}"#,
        r#"{"op":"range","patch":"x","from":0.5,"to":0}"#,
        r#"{"op":"message","comments":[],"extra":true}"#,
        r#"{"op":"message","comments":[{"path":"p","lines":"line 1","quoted":[],"text":"x","extra":true}]}"#,
    ] {
        got.push(json!(server.request("POST", path, headers, body, true).0));
    }
    let large = json!({"op":"message","comments":[{"path":"p","lines":"line 1","quoted":[],"text":"x".repeat(65536)}]}).to_string();
    got.push(json!(server.request("POST", path, headers, &large, true).0));
    assert_eq!(
        json!(got),
        json!([{"status":401,"challenge":true},403,400,400,400,400,400,400,413])
    );
}
