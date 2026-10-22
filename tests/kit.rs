//! `swarm kit`: the installed kit's preset script gives the Skills page its facts (ADR 0111).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A swarm home whose `kit` links to this checkout's kit, as `swarm setup plugin` links a release.
fn home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-kit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let dir = std::fs::canonicalize(dir).unwrap();
    assert!(swarm(&dir, &["init"], "", None).status.success());
    std::os::unix::fs::symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/kit"),
        dir.join(".swarm/kit"),
    )
    .unwrap();
    dir
}

/// The built binary with only HOME, SWARM_HOME and PATH; `path` replaces this test's own PATH.
fn swarm(home: &Path, args: &[&str], stdin: &str, path: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env(
            "PATH",
            path.map(str::to_string)
                .unwrap_or_else(|| std::env::var("PATH").unwrap()),
        )
        .current_dir(home)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

const DRAFT: &str = "## Preset\n\nStop: chat\n\n| File | Needs | Holds | Block | Why |\n|---|---|---|---|---|\n\
| 01-read | none | the sources | local-research, requirements | the answer rests on them |\n\
| 02-reply | read | the answer | voice-reply | the owner reads it |\n";

#[test]
fn kit_json_lists_the_presets_with_python_stages() {
    let home = home("json");
    let kit = json(&swarm(&home, &["kit", "--json"], "", None));
    assert_eq!(kit["schema"], "swarm.kit/1");
    let research = kit["presets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["preset"] == "research")
        .unwrap();
    let stages: Vec<_> = research["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["file"].as_str().unwrap(), s["stage"].as_u64().unwrap()))
        .collect();
    assert_eq!(stages[0], ("01-question", 0));
    assert_eq!(stages[1], ("02-local", 1));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn kit_check_reads_the_draft_on_stdin_and_names_the_first_defect() {
    let home = home("check");
    let passed = json(&swarm(&home, &["kit", "check"], DRAFT, None));
    assert_eq!(passed["ok"], true);
    assert_eq!(passed["preset"]["steps"][1]["stage"], 1);
    let failed = json(&swarm(
        &home,
        &["kit", "check"],
        &DRAFT.replace("| read |", "| nope |"),
        None,
    ));
    assert_eq!(
        failed,
        serde_json::json!({"ok": false, "defect": "step 02-reply needs 'nope', which no step gives"})
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn kit_names_a_missing_python_and_passes_a_script_error_through() {
    let home = home("python");
    let missing = swarm(
        &home,
        &["kit", "--json"],
        "",
        Some(&home.join("bin").display().to_string()),
    );
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr)
            .contains("swarm kit: python3 not found; the Skills page needs Python 3.11 or newer"),
        "{missing:?}"
    );
    let old = home.join("bin/python3");
    std::fs::write(
        &old,
        "#!/bin/sh\necho 'swarm kit: needs Python 3.11 or newer' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&old, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let failed = swarm(
        &home,
        &["kit", "--json"],
        "",
        Some(&home.join("bin").display().to_string()),
    );
    assert_eq!(failed.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&failed.stderr),
        "swarm kit: needs Python 3.11 or newer\n"
    );
    std::fs::remove_dir_all(home).unwrap();
}
