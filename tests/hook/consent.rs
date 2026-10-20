// Tests for consent.
use super::*;

/// The launch consent is a managed edit: Managed Changes lists it, and its undo of a first answer
/// records `ask`, which counts as set up, so the app does not ask again (ADR 0043, "asks once").
/// `--consent ask` records the owner's ask answer (owner answer 2026-10-06).
#[test]
fn launch_consent_is_a_managed_edit_that_reverts_to_ask() {
    let home = scratch("setup-consent");
    let consent = home.join(".swarm/consent.json");
    let status = || -> serde_json::Value {
        serde_json::from_slice(&setup(&home, &["status", "--json"]).stdout).unwrap()
    };
    let apply = |extra: &[&str]| {
        let mut args = vec!["trust", "--plan", "--json"];
        args.extend(extra);
        let plan: serde_json::Value = serde_json::from_slice(&setup(&home, &args).stdout).unwrap();
        let mut args = vec!["trust", "--digest", plan["digest"].as_str().unwrap()];
        args.extend(extra);
        let applied = setup(&home, &args);
        assert!(applied.status.success(), "{applied:?}");
        plan
    };
    let consent_entries = || -> Vec<serde_json::Value> {
        let list = swarm(&home, &[], &["managed", "list", "--json"], "");
        let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
        list["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["path"] == serde_json::json!(["trust"]))
            .cloned()
            .collect()
    };

    apply(&[]);
    assert_eq!(status()["trust"], true);
    let entries = consent_entries();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0]["writer"], "launch.trust");
    assert_eq!(entries[0]["wrote"], "standing");
    assert_eq!(entries[0]["state"], "present");

    let id = entries[0]["id"].as_str().unwrap();
    let plan = swarm(
        &home,
        &[],
        &["managed", "revert", id, "--plan", "--json"],
        "",
    );
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    let reverted = swarm(
        &home,
        &[],
        &["managed", "revert", id, "--digest", digest],
        "",
    );
    assert!(reverted.status.success(), "{reverted:?}");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&consent).unwrap()).unwrap();
    assert_eq!(value["trust"], "ask", "{value}");
    assert_eq!(status()["trust"], true);
    assert_eq!(consent_entries()[0]["state"], "off");

    std::fs::remove_file(&consent).unwrap();
    let plan = apply(&["--consent", "ask"]);
    assert!(
        plan["files"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("+  \"trust\": \"ask\""),
        "{plan}"
    );
    assert_eq!(status()["trust"], true);
    let plan: serde_json::Value =
        serde_json::from_slice(&setup(&home, &["trust", "--plan", "--json"]).stdout).unwrap();
    assert_eq!(plan["consent"], "ask");
    let bad = setup(&home, &["--plan", "--consent", "always"]);
    assert!(!bad.status.success());
    std::fs::remove_dir_all(&home).unwrap();
}

/// A child pane may read the setup plan, but not apply it: an apply writes the owner's consent
/// and config, so a seat left on `ask` could turn on standing consent itself (SRV-2).
#[test]
fn a_child_agent_may_plan_setup_but_not_apply_it() {
    let home = scratch("setup-child");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let child = [("SWARM_AGENT_ID", "coder"), ("SWARM_SESSION_ID", "s1")];
    let status = swarm(&home, &child, &["setup", "status", "--json"], "");
    assert!(status.status.success(), "{status:?}");
    let plan = swarm(
        &home,
        &child,
        &["setup", "hooks,trust", "--plan", "--json", "--cwd", &cwd],
        "",
    );
    assert!(plan.status.success(), "{plan:?}");
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let digest = plan["digest"].as_str().unwrap();
    for args in [
        vec![
            "setup",
            "hooks,trust",
            "--yes",
            "--consent",
            "standing",
            "--cwd",
            &cwd,
        ],
        vec!["setup", "hooks,trust", "--digest", digest, "--cwd", &cwd],
    ] {
        let applied = swarm(&home, &child, &args, "");
        assert!(!applied.status.success(), "{applied:?}");
        assert!(
            String::from_utf8_lossy(&applied.stderr).contains("a child agent cannot"),
            "{applied:?}"
        );
    }
    assert!(!home.join(".swarm/consent.json").exists());
    assert!(!home.join(".claude.json").exists());
    std::fs::remove_dir_all(&home).unwrap();
}

/// The approve command a seat launch prints under `ask` approves the folder and keeps the ask
/// answer: with no `--consent`, setup plans the owner's recorded answer, and the plan reports the
/// answer it sets, which the app's radio starts at.
#[test]
fn setup_with_no_consent_flag_keeps_the_recorded_answer() {
    let home = scratch("setup-keeps-ask");
    let repo = git_repo(&home, "app");
    let cwd = repo.to_string_lossy().into_owned();
    let consent = home.join(".swarm/consent.json");
    std::fs::write(&consent, "{\"trust\": \"ask\"}\n").unwrap();
    let plan = setup(&home, &["trust", "--plan", "--json", "--cwd", &cwd]);
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["consent"], "ask");
    let paths: Vec<&str> = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    assert!(
        !paths.iter().any(|path| path.ends_with("consent.json")),
        "{paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with(".claude.json")),
        "{paths:?}"
    );
    let digest = plan["digest"].as_str().unwrap();
    let applied = setup(&home, &["trust", "--digest", digest, "--cwd", &cwd]);
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(&consent).unwrap(),
        "{\"trust\": \"ask\"}\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// A bare `swarm setup` that waits on the trust lock reads the owner's answer once it holds the
/// lock, so an `ask` that the app records while it waits is kept, not written over with standing
/// (L-4).
#[test]
fn a_setup_waiting_on_the_trust_lock_keeps_an_answer_recorded_meanwhile() {
    let home = scratch("setup-lock-answer");
    let repo = git_repo(&home, "app");
    let consent = home.join(".swarm/consent.json");
    let holder = std::fs::File::create(home.join(".swarm/trust.lock")).unwrap();
    holder.lock().unwrap();
    let waiting = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), &home)
        .current_dir(&home)
        .args(["setup", "trust", "--yes", "--cwd"])
        .arg(&repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The waiting setup has read the files it reads before the lock by now.
    std::thread::sleep(std::time::Duration::from_millis(500));
    std::fs::write(&consent, "{\"trust\": \"ask\"}\n").unwrap();
    drop(holder);
    let applied = waiting.wait_with_output().unwrap();
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(&consent).unwrap(),
        "{\"trust\": \"ask\"}\n"
    );
    std::fs::remove_dir_all(&home).unwrap();
}
