use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skills")
}

#[test]
fn manifest_command_writes_schema_pins_and_complete_catalog() {
    let home = std::env::temp_dir().join(format!("swarm-manifest-home-{}", std::process::id()));
    fs::create_dir_all(&home).unwrap();
    let out = home.join("manifest.json");
    let result = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", home.join("branch"))
        .args(["skills", "manifest"])
        .arg(fixture())
        .arg(&out)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["content_id"].as_str().unwrap().len(), 64);
    assert_eq!(
        json["kit_commit"],
        "ec8b85f0953292c72853a3428a2ee395077ff8c3"
    );
    assert_eq!(
        json["vendor_commits"]["taste-skill"],
        "ce26fc25c0e5e8cab638f883de62d9a86ee5e45b"
    );
    assert_eq!(
        json["vendor_commits"]["emil-skills"],
        "e8a175de22ae1e49370fc144c1f3bb9aeedf988d"
    );
    let catalog = json["catalog"].as_array().unwrap();
    assert_eq!(catalog.len(), 23);
    for link in catalog {
        assert!(
            fixture()
                .join(link["path"].as_str().unwrap())
                .join("SKILL.md")
                .is_file()
        );
    }
    for path in [
        "kit/references/role.md",
        "kit/contracts/role.json",
        "kit/scripts/role.sh",
        "kit/LICENSE",
        "kit/README.md",
        "kit/vendor/taste-skill/LICENSE",
        "kit/vendor/emil-skills/LICENSE",
    ] {
        assert!(
            json["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file["path"] == path),
            "{path}"
        );
    }
    assert!(!home.join("branch").exists());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn manifest_validates_paths_duplicates_bytes_modes_and_catalog() {
    let original = swarm::skills::Manifest::from_tree(&fixture()).unwrap();
    original.validate(&fixture()).unwrap();
    for mutation in [
        "traversal",
        "absolute",
        "duplicate",
        "duplicate-name",
        "catalog",
        "bytes",
        "mode",
        "directory-mode",
        "schema",
    ] {
        let mut manifest = original.clone();
        match mutation {
            "traversal" => manifest.files[0].path = "../owner".into(),
            "absolute" => manifest.files[0].path = "/owner".into(),
            "duplicate" => manifest.files.push(manifest.files[0].clone()),
            "duplicate-name" => manifest.catalog.push(manifest.catalog[0].clone()),
            "catalog" => manifest.catalog[0].path = "kit/missing".into(),
            "bytes" => manifest.files[0].sha256 = "0".repeat(64),
            "mode" => manifest.files[0].mode ^= 0o111,
            "directory-mode" => manifest.directories[0].mode ^= 0o111,
            "schema" => manifest.schema = 2,
            _ => unreachable!(),
        }
        assert!(manifest.validate(&fixture()).is_err(), "{mutation}");
    }
    assert_eq!(
        fs::metadata(fixture().join("kit/scripts/role.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[test]
fn manifest_is_stable_excludes_git_metadata_and_refuses_source_links() {
    let tree = std::env::temp_dir().join(format!("swarm-manifest-source-{}", std::process::id()));
    fs::create_dir_all(&tree).unwrap();
    assert!(
        Command::new("cp")
            .args(["-Rp"])
            .arg(fixture().join("."))
            .arg(&tree)
            .status()
            .unwrap()
            .success()
    );
    let original = swarm::skills::Manifest::from_tree(&tree).unwrap();
    fs::create_dir_all(tree.join(".git")).unwrap();
    fs::write(tree.join(".git/ignored"), "owner data").unwrap();
    swarm::skills::write_manifest(&tree, &tree.join("manifest.json")).unwrap();
    let repeated = swarm::skills::Manifest::read(&tree).unwrap();
    assert_eq!(original.content_id, repeated.content_id);
    assert!(!repeated.files.iter().any(|file| file.path.contains(".git")));
    let script = tree.join("kit/scripts/role.sh");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).unwrap();
    assert_ne!(
        original.content_id,
        swarm::skills::Manifest::from_tree(&tree)
            .unwrap()
            .content_id
    );
    fs::write(&script, "changed bytes").unwrap();
    assert!(original.validate(&tree).is_err());
    std::os::unix::fs::symlink("/outside/owner", tree.join("escape")).unwrap();
    assert!(swarm::skills::Manifest::from_tree(&tree).is_err());
    fs::remove_dir_all(tree).unwrap();
}

#[test]
fn manifest_and_host_use_the_same_skill_names() {
    let manifest = swarm::skills::Manifest::from_tree(&fixture()).unwrap();
    for provider in ["claude", "codex", "agy"] {
        let context = swarm::host::context(provider, |name| match name {
            "HERDR_ENV" => Some("1".into()),
            "HERDR_PANE_ID" => Some("fixture-pane".into()),
            _ => None,
        })
        .unwrap();
        let mut found = 0;
        for path in context.split("/skills/").skip(1) {
            let name = path.split('/').next().unwrap();
            assert!(
                manifest.catalog.iter().any(|link| link.name == name),
                "{provider}: {name}"
            );
            found += 1;
        }
        assert!(found >= 2, "{provider}: {context}");
    }
}

#[test]
fn manifest_checks_listed_content_and_ignores_foreign_files() {
    let tree = std::env::temp_dir().join(format!(
        "swarm-manifest-foreign-files-{}",
        std::process::id()
    ));
    fs::create_dir_all(&tree).unwrap();
    assert!(
        Command::new("cp")
            .arg("-Rp")
            .arg(fixture().join("."))
            .arg(&tree)
            .status()
            .unwrap()
            .success()
    );
    swarm::skills::write_manifest(&tree, &tree.join("manifest.json")).unwrap();
    let original = swarm::skills::Manifest::read(&tree).unwrap();
    for path in [
        ".DS_Store",
        "kit/references/.role.md.swp",
        "kit/references/__pycache__/role.pyc",
        "kit/references/editor-backup/role.md",
    ] {
        let path = tree.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "foreign data").unwrap();
    }
    original.validate(&tree).unwrap();
    assert_eq!(
        swarm::skills::Manifest::read(&tree).unwrap().content_id,
        original.content_id
    );
    let script = tree.join("kit/scripts/role.sh");
    let bytes = fs::read(&script).unwrap();
    let permissions = fs::metadata(&script).unwrap().permissions();
    for role in [
        "missing-file",
        "changed-bytes",
        "changed-mode",
        "linked-file",
    ] {
        match role {
            "missing-file" => fs::remove_file(&script).unwrap(),
            "changed-bytes" => fs::write(&script, "changed source").unwrap(),
            "changed-mode" => {
                fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).unwrap()
            }
            "linked-file" => {
                fs::remove_file(&script).unwrap();
                std::os::unix::fs::symlink(fixture().join("kit/scripts/role.sh"), &script).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(original.validate(&tree).is_err(), "{role}");
        assert!(swarm::skills::Manifest::read(&tree).is_err(), "{role}");
        if fs::symlink_metadata(&script).is_ok() {
            fs::remove_file(&script).unwrap();
        }
        fs::write(&script, &bytes).unwrap();
        fs::set_permissions(&script, permissions.clone()).unwrap();
    }
    let directory = tree.join("kit/scripts");
    let permissions = fs::metadata(&directory).unwrap().permissions();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(original.validate(&tree).is_err());
    assert!(swarm::skills::Manifest::read(&tree).is_err());
    fs::set_permissions(&directory, permissions).unwrap();
    let backup = tree.join("unlisted-script-backup");
    for role in ["missing-directory", "linked-directory"] {
        fs::rename(&directory, &backup).unwrap();
        if role == "linked-directory" {
            std::os::unix::fs::symlink(&backup, &directory).unwrap();
        }
        assert!(original.validate(&tree).is_err(), "{role}");
        assert!(swarm::skills::Manifest::read(&tree).is_err(), "{role}");
        if role == "linked-directory" {
            fs::remove_file(&directory).unwrap();
        }
        fs::rename(&backup, &directory).unwrap();
    }
    let mut incomplete = original.clone();
    incomplete
        .directories
        .retain(|directory| directory.path != "kit/scripts");
    assert!(incomplete.validate(&tree).is_err());
    original.validate(&tree).unwrap();
    fs::remove_dir_all(tree).unwrap();
}
