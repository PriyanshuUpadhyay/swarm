// Tests for the Skills page routes (ADR 0111).

use super::*;

/// The server's PATH has only its `bin` and the system folders, so this test points `python3`
/// there at the one this test runs with, which is new enough for the kit.
fn link_kit(server: &Server) {
    let home = server.dir.join("home");
    std::os::unix::fs::symlink(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills/kit"),
        home.join(".swarm/kit"),
    )
    .unwrap();
    let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("python3"))
        .find(|path| path.is_file())
        .unwrap();
    let shim = home.join("bin/python3");
    fs::write(
        &shim,
        format!("#!/bin/sh\nexec '{}' \"$@\"\n", python.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
}

const JSON: &str = "Content-Type: application/json\r\n";
const DRAFT: &str = "## Preset\n\nStop: chat\n\n| File | Needs | Holds | Block | Why |\n|---|---|---|---|---|\n\
| 01-read | none | the sources | local-research, requirements | the answer rests on them |\n\
| 02-reply | read | the answer | voice-reply | the owner reads it |\n";

#[test]
fn the_kit_routes_give_python_facts_and_checks_and_keep_the_guard() {
    let server = Server::new();
    link_kit(&server);
    assert_eq!(server.request("GET", "/api/kit", "", "", false).0, 401);
    let check = json!({"text": DRAFT}).to_string();
    assert_eq!(
        server
            .request("POST", "/api/kit/check", JSON, &check, false)
            .0,
        401
    );

    let (status, _, body) = server.request("GET", "/api/kit", "", "", true);
    assert_eq!(status, 200, "{body}");
    let kit: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(kit["schema"], "swarm.kit/1");
    assert!(
        kit["presets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["preset"] == "implement")
    );

    let (status, _, body) = server.request("POST", "/api/kit/check", JSON, &check, true);
    assert_eq!(status, 200, "{body}");
    let passed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(passed["ok"], true);
    assert_eq!(passed["preset"]["steps"][1]["uses"], json!(["01-read"]));

    let piped = json!({"text": DRAFT.replace("the answer |", "the | answer |")}).to_string();
    let (status, _, body) = server.request("POST", "/api/kit/check", JSON, &piped, true);
    assert_eq!(status, 200, "{body}");
    let failed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(failed["ok"], false);
    assert!(
        failed["defect"]
            .as_str()
            .unwrap()
            .starts_with("row has 6 cells, not 5")
    );

    // A script that fails shows its own stderr, so the owner sees why.
    fs::remove_file(server.dir.join("home/.swarm/kit")).unwrap();
    let (status, _, body) = server.request("GET", "/api/kit", "", "", true);
    assert_eq!(status, 500, "{body}");
    assert!(body.contains("preset.py"), "{body}");
}
