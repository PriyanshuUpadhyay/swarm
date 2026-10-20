// Tests for workspace.

use super::*;

#[test]
fn changes_routes_latency_and_git_spawn_counts() {
    use std::os::unix::fs::PermissionsExt;
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
    let wrapper = server.dir.join("home/bin/git");
    let trace = std::env::var_os("SWARM_GIT_TRACE").is_some();
    fs::write(
        &wrapper,
        if trace { r#"#!/usr/bin/perl
use Time::HiRes qw(time);
my $start = time;
system '/usr/bin/git', @ARGV;
my $status = $?;
if ($ARGV[2] eq '--literal-pathspecs') {
    open my $log, '>>', "$ENV{HOME}/git-calls" or die $!;
    printf $log "%.3f\t%s\n", (time - $start) * 1000, join(' ', @ARGV[3 .. $#ARGV]);
}
exit($status == -1 ? 127 : ($status & 127) ? 128 + ($status & 127) : $status >> 8);
"# } else { "#!/bin/sh\ncase \"$3\" in --literal-pathspecs) printf '0\\t%s\\n' \"$*\" >> \"$HOME/git-calls\" ;; esac\nexec /usr/bin/git \"$@\"\n" },
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let calls = server.dir.join("home/git-calls");
    let baseline = std::env::var_os("SWARM_GIT_BASELINE").is_some();
    for (mode, list_count, diff_count) in [("local", 6, 5), ("branch", 8, 8)] {
        for (route, expected) in [
            ("/api/changes", list_count),
            ("/api/changes/diff", diff_count),
        ] {
            let query = format!(
                "{route}?workspace={}&mode={mode}&base=main&path=large.txt&layer={}",
                project.display(),
                if mode == "local" {
                    "unstaged"
                } else {
                    "branch"
                }
            );
            let mut timings = Vec::new();
            let mut counts = Vec::new();
            let mut commands: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
            for _ in 0..20 {
                fs::write(&calls, "").unwrap();
                let start = Instant::now();
                let (status, _, body) = server.request("GET", &query, "", "", true);
                timings.push(start.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(status, 200, "{body}");
                if route.ends_with("/diff") {
                    let patch: Value = serde_json::from_str(&body).unwrap();
                    assert_eq!(patch["added"], 20_000);
                    assert_eq!(patch["is_truncated"], true);
                }
                let calls = fs::read_to_string(&calls).unwrap();
                let count = calls.lines().count();
                for call in calls.lines() {
                    let (elapsed, command) = call.split_once('\t').unwrap();
                    commands
                        .entry(command.into())
                        .or_default()
                        .push(elapsed.parse().unwrap());
                }
                counts.push(count);
                if !baseline {
                    assert_eq!(count, expected, "{mode} {route}");
                }
            }
            timings.sort_by(f64::total_cmp);
            println!(
                "{mode} {route} p95_ms={:.3} git_spawns={counts:?}",
                timings[18]
            );
            for (command, mut timings) in commands.into_iter().filter(|_| trace) {
                timings.sort_by(f64::total_cmp);
                println!(
                    "{mode} {route} command={command} samples={} p50_ms={:.3} p95_ms={:.3}",
                    timings.len(),
                    timings[timings.len() / 2],
                    timings[(timings.len() * 95).div_ceil(100) - 1]
                );
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
#[ignore = "runs the production 600-second setup deadline; requires loopback access"]
fn setup_timeout_http_keeps_workspace_and_kills_process_group() {
    let server = Server::configured(false, true);
    let source = server.dir.join("project");
    swarm::view::choices_store::update(&server.dir.join("home/.swarm"), None, |owner, _| {
        owner.project_defaults.insert(
            source.to_string_lossy().into_owned(),
            json!({
                "setup_script":"sleep 900 & printf '%s' \"$!\" > sleeper; wait"
            }),
        );
        Ok(())
    })
    .unwrap();
    let body = json!({"project":source,"name":"Timeout","mode":"new-branch","prefix":"","folder":"../workspaces"}).to_string();
    let mut stream = server.connection();
    stream
        .set_read_timeout(Some(Duration::from_secs(630)))
        .unwrap();
    write!(stream, "POST /api/workspace/new HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", server.port, server.token, body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    assert_eq!(headers.split_whitespace().nth(1), Some("200"));
    let result: Value = serde_json::from_str(body).unwrap();
    assert_eq!(result["setup"]["state"], "timeout");
    let workspace = PathBuf::from(result["workspace"].as_str().unwrap());
    assert!(workspace.is_dir());
    let pid = fs::read_to_string(workspace.join("sleeper")).unwrap();
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
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
