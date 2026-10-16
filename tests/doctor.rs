use std::process::Command;

#[test]
fn doctor_json_has_one_row_per_item_and_an_empty_home_needs_setup() {
    let root = std::env::temp_dir().join(format!("swarm-doctor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("owner");
    std::fs::create_dir_all(home.join(".swarm")).unwrap();
    std::fs::write(home.join(".swarm/swarm-home"), "swarm\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(&home)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(out.status.success(), "{out:?}");
    let doctor: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doctor["schema"], "swarm.doctor.v1");
    let items = doctor["items"].as_array().unwrap();
    let ids: Vec<_> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    let mut expected = vec![
        "hooks",
        "guard",
        "trust",
        "plugin",
        "hud",
        "transcript",
        "notices",
    ];
    if cfg!(feature = "serve") {
        expected.push("ui");
    }
    assert_eq!(ids, expected);
    for item in items {
        assert!(
            ["ok", "setup", "broken"].contains(&item["state"].as_str().unwrap()),
            "{item}"
        );
        assert!(
            item["title"].as_str().is_some_and(|t| !t.is_empty()),
            "{item}"
        );
        assert!(item["reason"].is_string(), "{item}");
        assert!(
            item["group"].is_null() || item["group"].is_string(),
            "{item}"
        );
    }
    let item = |id: &str| items.iter().find(|i| i["id"] == id).unwrap();
    // A home with no hooks or trust answer needs setup, and the row names the setup group.
    assert_eq!(item("hooks")["state"], "setup");
    assert_eq!(item("hooks")["group"], "hooks");
    assert_eq!(item("trust")["state"], "setup");
    assert_eq!(item("trust")["group"], "trust");
}

#[test]
fn doctor_reports_a_consent_file_it_cannot_read_as_broken() {
    let root = std::env::temp_dir().join(format!("swarm-doctor-consent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("owner");
    std::fs::create_dir_all(home.join(".swarm")).unwrap();
    std::fs::write(home.join(".swarm/swarm-home"), "swarm\n").unwrap();
    std::fs::write(home.join(".swarm/consent.json"), "{").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(&home)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(out.status.success(), "{out:?}");
    let doctor: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let trust = doctor["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "trust")
        .unwrap();
    assert_eq!(trust["state"], "broken", "{trust}");
}

/// Every file and folder under `root`, so a run that writes anything shows as a change.
fn tree(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() && !path.is_symlink() {
                todo.push(path.clone());
            }
            paths.push(path);
        }
    }
    paths.sort();
    paths
}

#[test]
fn bare_setup_lists_the_doctor_rows_and_writes_nothing() {
    let root = std::env::temp_dir().join(format!("swarm-setup-list-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("owner");
    std::fs::create_dir_all(home.join(".swarm")).unwrap();
    std::fs::write(home.join(".swarm/swarm-home"), "swarm\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &home)
            .env("SWARM_HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .current_dir(&home)
            .args(args)
            .output()
            .unwrap()
    };
    let before = tree(&home);
    let listed = run(&["setup"]);
    let json = run(&["setup", "--json"]);
    let doctor = run(&["doctor", "--json"]);
    let after = tree(&home);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(listed.status.success(), "{listed:?}");
    assert_eq!(before, after);
    let text = String::from_utf8(listed.stdout).unwrap();
    assert!(text.contains("setup  Agent hooks"), "{text}");
    assert!(
        text.contains("Set up:    swarm setup hooks,trust"),
        "{text}"
    );
    let ids = |out: &std::process::Output| {
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&json), ids(&doctor));
}
