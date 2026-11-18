// Tests for handoff.

use super::*;

#[test]
fn menu_http_matches_cli_json_and_keeps_bearer_guards() {
    let server = Server::with_actions(true);
    let home = server.dir.join("home");
    for (route, args) in [
        ("/api/roles", vec!["roles", "--json"]),
        ("/api/providers", vec!["providers", "--json"]),
        (
            "/api/models?provider=claude",
            vec!["models", "--provider", "claude", "--json"],
        ),
        (
            "/api/accounts?provider=agy",
            vec!["accounts", "--provider", "agy", "--json"],
        ),
    ] {
        let (status, _, body) = server.request("GET", route, "", "", true);
        assert_eq!(status, 200, "{body}");
        let cli = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &home)
            .env("SWARM_HOME", &home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .args(args)
            .output()
            .unwrap();
        assert!(
            cli.status.success(),
            "{}",
            String::from_utf8_lossy(&cli.stderr)
        );
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            serde_json::from_slice::<Value>(&cli.stdout).unwrap()
        );
        assert_eq!(server.request("GET", route, "", "", false).0, 401);
    }
    for route in ["/api/models", "/api/accounts?provider=unknown"] {
        assert_eq!(server.request("GET", route, "", "", true).0, 400);
    }
    for route in ["/api/chat/continue", "/api/chat/continue/cancel"] {
        assert_eq!(
            server
                .request(
                    "POST",
                    route,
                    "Content-Type: application/json\r\n",
                    "{}",
                    false
                )
                .0,
            401
        );
        assert_eq!(
            server
                .request(
                    "POST",
                    route,
                    "Content-Type: application/json\r\n",
                    "{}",
                    true
                )
                .0,
            400
        );
    }
}
