use std::{
    fs,
    os::unix::fs::symlink,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    build_home: PathBuf,
}
impl Fixture {
    fn new(role: &str, copy: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("swarm-setup-skills-{role}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("owner");
        let build_home = root.join("build");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&build_home).unwrap();
        if copy {
            let installed = build_home.join(".swarm/skills");
            fs::create_dir_all(&installed).unwrap();
            fs::write(build_home.join(".swarm/swarm-home"), "swarm\n").unwrap();
            assert!(
                Command::new("cp")
                    .arg("-Rp")
                    .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skills/."))
                    .arg(&installed)
                    .status()
                    .unwrap()
                    .success()
            );
            swarm::skills::write_manifest(&installed, &installed.join("manifest.json")).unwrap();
        }
        Self {
            root,
            home,
            build_home,
        }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &self.home)
            .env("SWARM_HOME", &self.build_home)
            .env("PATH", "/usr/bin:/bin")
            .current_dir(&self.home)
            .args(args)
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.run(args);
        assert!(out.status.success(), "{out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn plan(&self) -> serde_json::Value {
        self.json(&["setup", "--plan", "--json", "--only", "skills"])
    }
    fn destination(&self) -> PathBuf {
        self.home.join(".agents/skills/flow")
    }
    fn status(&self) -> serde_json::Value {
        self.json(&["setup", "status", "--json"])
    }
    fn apply(&self, digest: &str) -> Output {
        self.run(&["setup", "--digest", digest, "--only", "skills"])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn setup_usage_lists_the_skills_group() {
    let f = Fixture::new("usage-skills", false);
    let output = f.run(&["setup", "--invalid"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--only <hooks|trust|herdr|skills>"));
}

#[test]
fn shared_cli_roots_plan_and_apply_each_canonical_leaf_once() {
    let f = Fixture::new("shared-cli-root", true);
    fs::create_dir_all(f.home.join(".agents/skills")).unwrap();
    fs::create_dir_all(f.home.join(".gemini/config")).unwrap();
    symlink(
        f.home.join(".agents/skills"),
        f.home.join(".gemini/config/skills"),
    )
    .unwrap();
    let plan = f.plan();
    assert_eq!(plan["files"].as_array().unwrap().len(), 46);
    let output = f.apply(plan["digest"].as_str().unwrap());
    assert!(output.status.success(), "{output:?}");
    assert_eq!(f.status()["skills"], true);
    assert_eq!(f.plan()["files"], serde_json::json!([]));
}

#[test]
fn unreadable_store_is_a_skills_conflict_and_status_names_the_error() {
    for role in ["store-directory", "store-corrupt", "store-bad-row"] {
        let f = Fixture::new(role, true);
        let database = f.build_home.join(".swarm/swarm.db");
        if role == "store-directory" {
            fs::create_dir(&database).unwrap();
        } else if role == "store-corrupt" {
            fs::write(&database, "invalid database").unwrap();
        } else {
            let plan = f.plan();
            assert!(f.apply(plan["digest"].as_str().unwrap()).status.success());
            let store = rusqlite::Connection::open(&database).unwrap();
            store
                .execute(
                    "UPDATE managed_edit SET wrote = 'invalid target' WHERE writer = 'skills'",
                    [],
                )
                .unwrap();
        }
        let plan = f.json(&["setup", "--plan", "--json"]);
        let conflict = plan["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["group"] == "skills")
            .unwrap();
        assert_eq!(conflict["kind"], "unreadable");
        assert_eq!(conflict["file"], database.to_string_lossy().as_ref());
        assert_eq!(conflict["entry"], "the managed skills records");
        assert_eq!(
            conflict["wanted"],
            "a readable swarm store with valid managed skills records"
        );
        assert_eq!(
            conflict["fix"],
            format!(
                "repair the swarm store {}, then check the Skills plan again",
                database.display()
            )
        );
        assert!(
            conflict["found"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
        );
        assert!(
            plan["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file["group"] == "hooks")
        );
        let status = f.status();
        assert_eq!(status["skills"], false);
        assert!(
            status["skills_error"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
        );
        assert!(status["hooks"].is_boolean() && status["trust"].is_boolean());
    }
}

#[test]
fn relative_skills_home_is_a_conflict_and_status_names_the_error() {
    let f = Fixture::new("relative-build-home", false);
    let installed = f.home.join("relative/.swarm/skills");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    assert!(
        Command::new("cp")
            .arg("-Rp")
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skills"))
            .arg(&installed)
            .status()
            .unwrap()
            .success()
    );
    swarm::skills::write_manifest(&installed, &installed.join("manifest.json")).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &f.home)
            .env("SWARM_HOME", "relative")
            .env("PATH", "/usr/bin:/bin")
            .current_dir(&f.home)
            .args(args)
            .output()
            .unwrap()
    };
    let output = run(&["setup", "--plan", "--json"]);
    assert!(output.status.success(), "{output:?}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let conflict = plan["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["group"] == "skills")
        .unwrap();
    assert!(conflict["found"].as_str().unwrap().contains("absolute"));
    assert_eq!(conflict["file"], "relative");
    assert_eq!(conflict["entry"], "the swarm home");
    assert_eq!(conflict["wanted"], "an absolute swarm home path");
    assert_eq!(
        conflict["fix"],
        "set SWARM_HOME to an absolute path, then check the Skills plan again"
    );
    assert!(
        plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["group"] == "hooks")
    );
    let output = run(&["setup", "status", "--json"]);
    assert!(output.status.success(), "{output:?}");
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["skills"], false);
    assert!(
        status["skills_error"]
            .as_str()
            .unwrap()
            .contains("absolute")
    );
}

#[test]
fn runtime_python_cache_keeps_the_installed_copy_available() {
    let f = Fixture::new("runtime-python-cache", true);
    let plan = f.plan();
    assert!(f.apply(plan["digest"].as_str().unwrap()).status.success());
    let cache = f
        .build_home
        .join(".swarm/skills/kit/references/__pycache__");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("x.pyc"), "runtime cache").unwrap();
    fs::write(cache.parent().unwrap().join("loose.pyc"), "runtime cache").unwrap();
    assert_eq!(f.status()["skills"], true);
    let plan = f.json(&["setup", "--plan", "--json"]);
    assert!(
        plan["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["group"] != "skills")
    );
}

#[test]
fn clean_home_plans_and_records_all_69_links_in_the_selected_build_home() {
    let f = Fixture::new("clean-owner", true);
    assert_eq!(f.status()["skills"], false);
    assert!(f.status().get("skills_error").is_none());
    let plan = f.plan();
    assert_eq!(plan["files"].as_array().unwrap().len(), 69);
    assert_eq!(plan["conflicts"], serde_json::json!([]));
    assert!(!f.home.join(".agents").exists());
    assert!(!f.build_home.join(".swarm/swarm.db").exists());
    for file in plan["files"].as_array().unwrap() {
        assert_eq!(file["group"], "skills");
        assert!(file["diff"].as_str().unwrap().contains("link -> "));
    }
    let apply = f.apply(plan["digest"].as_str().unwrap());
    assert!(apply.status.success(), "{apply:?}");
    for root in [".claude/skills", ".agents/skills", ".gemini/config/skills"] {
        for link in swarm::skills::link_catalog() {
            assert_eq!(
                fs::read_link(f.home.join(root).join(link.name)).unwrap(),
                f.build_home.join(".swarm/skills").join(link.path)
            );
        }
    }
    assert_eq!(f.status()["skills"], true);
    assert_eq!(f.plan()["files"], serde_json::json!([]));
    let entries = f.json(&["managed", "list", "--json"]);
    let entries: Vec<_> = entries["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["writer"] == "skills")
        .collect();
    assert_eq!(entries.len(), 69);
    assert!(
        entries
            .iter()
            .all(|e| e["kind"] == "symlink" && e["recorded"] == true && e["state"] == "present")
    );
    fs::remove_dir_all(f.build_home.join(".swarm/skills")).unwrap();
    assert_eq!(f.status()["skills"], false);
}

#[test]
fn foreign_link_is_taken_and_no_plan_or_status_claims_it() {
    let f = Fixture::new("foreign-owner", true);
    fs::create_dir_all(f.destination().parent().unwrap()).unwrap();
    symlink("/foreign/flow", f.destination()).unwrap();
    let plan = f.plan();
    assert_eq!(plan["conflicts"][0]["kind"], "taken");
    assert_eq!(plan["conflicts"][0]["group"], "skills");
    assert_eq!(f.status()["skills"], false);
    assert!(!f.apply(plan["digest"].as_str().unwrap()).status.success());
    assert_eq!(
        fs::read_link(f.destination()).unwrap(),
        PathBuf::from("/foreign/flow")
    );
    assert!(!f.home.join(".claude/skills").exists());
}

#[test]
fn missing_default_copy_has_a_refresh_fix_and_other_groups_work() {
    let f = Fixture::new("missing-default", false);
    let plan = f.plan();
    assert_eq!(plan["files"], serde_json::json!([]));
    assert_eq!(plan["conflicts"][0]["group"], "skills");
    assert!(
        plan["conflicts"][0]["fix"]
            .as_str()
            .unwrap()
            .contains("swarm skills refresh")
    );
    assert_eq!(f.status()["skills"], false);
    assert!(f.status().get("skills_error").is_none());
    assert!(!f.build_home.join(".swarm/skills").exists());
    let other = f.run(&["setup", "--only", "hooks,herdr"]);
    assert!(other.status.success(), "{other:?}");
    assert!(!f.home.join(".agents/skills").exists());
}

#[test]
fn trust_only_excludes_skills_and_selected_groups_change_the_digest() {
    let f = Fixture::new("trust-only", true);
    let trust = f.json(&["setup", "--plan", "--json", "--only", "trust"]);
    assert!(
        trust["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["group"] == "trust")
    );
    assert!(trust["conflicts"].as_array().unwrap().is_empty());
    let skills = f.plan();
    let with_herdr = f.json(&["setup", "--plan", "--json", "--only", "skills,herdr"]);
    assert_ne!(skills["digest"], with_herdr["digest"]);
}

#[test]
fn stale_content_leaf_or_resolved_parent_refuses_the_setup_digest() {
    for role in ["content", "leaf", "parent"] {
        let f = Fixture::new(role, true);
        let plan = f.plan();
        match role {
            "content" => {
                let copy = f.build_home.join(".swarm/skills");
                fs::write(copy.join("kit/references/role.md"), "changed bundle\n").unwrap();
                swarm::skills::write_manifest(&copy, &copy.join("manifest.json")).unwrap();
            }
            "leaf" => {
                fs::create_dir_all(f.destination().parent().unwrap()).unwrap();
                symlink("/foreign/changed", f.destination()).unwrap();
            }
            "parent" => {
                let elsewhere = f.root.join("elsewhere");
                fs::create_dir_all(&elsewhere).unwrap();
                symlink(elsewhere, f.home.join(".agents")).unwrap();
            }
            _ => unreachable!(),
        }
        assert_ne!(plan["digest"], f.plan()["digest"]);
        let out = f.apply(plan["digest"].as_str().unwrap());
        assert!(!out.status.success(), "{role}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("changed after the plan"),
            "{role}: {out:?}"
        );
        assert!(!f.home.join(".claude/skills").exists());
    }
}

#[test]
fn recorded_present_link_is_kept_and_a_missing_recorded_link_is_planned_again() {
    let f = Fixture::new("recorded-present", true);
    let plan = f.plan();
    assert!(f.apply(plan["digest"].as_str().unwrap()).status.success());
    let kept = f.destination();
    let before = fs::read_link(&kept).unwrap();
    let missing = f.home.join(".claude/skills/flow");
    fs::remove_file(&missing).unwrap();
    assert_eq!(f.status()["skills"], false);
    let plan = f.plan();
    assert_eq!(plan["files"].as_array().unwrap().len(), 1);
    assert_eq!(plan["files"][0]["path"], missing.to_string_lossy().as_ref());
    assert_eq!(plan["conflicts"], serde_json::json!([]));
    assert_eq!(fs::read_link(&kept).unwrap(), before);
    assert!(f.apply(plan["digest"].as_str().unwrap()).status.success());
    assert_eq!(f.status()["skills"], true);
}

#[test]
fn every_host_skill_path_names_a_catalog_skill_and_a_planned_cli_destination() {
    let f = Fixture::new("host-agreement", true);
    let plan = f.plan();
    let catalog = swarm::skills::link_catalog();
    for provider in ["claude", "codex", "agy"] {
        for (host, worker) in [
            ("herdr", false),
            ("herdr", true),
            ("app", false),
            ("app", true),
        ] {
            let context = swarm::host::context(provider, |name| match (host, name) {
                ("herdr", "HERDR_ENV") => Some("1".into()),
                ("herdr", "HERDR_PANE_ID") => Some("fixture-pane".into()),
                ("app", "SWARM_ADAPTER") => Some("tmux-solo".into()),
                ("app", "SWARM_SESSION_ID") => Some("fixture-session".into()),
                ("app", "TMUX_PANE") => Some("fixture-pane".into()),
                (_, "SWARM_AGENT_ID") if worker => Some("fixture-worker".into()),
                _ => None,
            })
            .unwrap();
            let paths: Vec<_> = context
                .split('`')
                .filter_map(|text| {
                    text.strip_prefix("~/")
                        .and_then(|path| path.strip_suffix("/SKILL.md"))
                })
                .collect();
            assert!(!paths.is_empty(), "{provider} {host} worker={worker}");
            for path in paths {
                let (root, name) = path.rsplit_once('/').unwrap();
                assert!(
                    [".claude/skills", ".agents/skills", ".gemini/config/skills"].contains(&root),
                    "{path}"
                );
                let link = catalog
                    .iter()
                    .find(|link| link.name == name)
                    .unwrap_or_else(|| panic!("host names absent catalog skill {path}"));
                let destination = f.home.join(path);
                let file = plan["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|file| file["path"] == destination.to_string_lossy().as_ref())
                    .unwrap_or_else(|| {
                        panic!("host destination absent from real Skills plan: {path}")
                    });
                assert_eq!(file["group"], "skills");
                assert!(
                    file["diff"].as_str().unwrap().contains(
                        f.build_home
                            .join(".swarm/skills")
                            .join(&link.path)
                            .to_str()
                            .unwrap()
                    )
                );
            }
        }
    }
}

#[test]
fn owner_recreated_link_agrees_in_list_and_setup_and_does_not_block_undo_all() {
    let f = Fixture::new("owner-recreated-undo-all", true);
    let plan = f.plan();
    assert!(f.apply(plan["digest"].as_str().unwrap()).status.success());
    let listed = f.json(&["managed", "list", "--json"]);
    let owner = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["file"]
                .as_str()
                .unwrap()
                .ends_with("/.agents/skills/flow")
        })
        .unwrap();
    let undo = f.run(&["managed", "revert", owner["id"].as_str().unwrap()]);
    assert!(undo.status.success(), "{undo:?}");
    let target = f.build_home.join(".swarm/skills/kit/skills/flow");
    symlink(&target, f.destination()).unwrap();
    let listed = f.json(&["managed", "list", "--json"]);
    let owner = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["file"]
                .as_str()
                .unwrap()
                .ends_with("/.agents/skills/flow")
        })
        .unwrap();
    assert_eq!(owner["state"], "changed");
    let plan = f.plan();
    let conflict = plan["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|conflict| conflict["file"] == f.destination().to_string_lossy().as_ref())
        .unwrap();
    assert_eq!(conflict["kind"], "taken");
    let undo = f.json(&["managed", "revert", "--all", "--plan", "--json"]);
    assert_eq!(undo["files"].as_array().unwrap().len(), 68);
    assert_eq!(undo["conflicts"], serde_json::json!([]));
    let output = f.run(&[
        "managed",
        "revert",
        "--all",
        "--digest",
        undo["digest"].as_str().unwrap(),
    ]);
    assert!(output.status.success(), "{output:?}");
    for root in [".claude/skills", ".agents/skills", ".gemini/config/skills"] {
        for skill in swarm::skills::link_catalog() {
            let path = f.home.join(root).join(skill.name);
            if path == f.destination() {
                assert_eq!(fs::read_link(path).unwrap(), target);
            } else {
                assert!(fs::symlink_metadata(path).is_err());
            }
        }
    }
}

#[test]
fn finder_metadata_keeps_the_applied_skills_copy_available() {
    let f = Fixture::new("finder-metadata-after-apply", true);
    let plan = f.plan();
    assert!(f.apply(plan["digest"].as_str().unwrap()).status.success());
    fs::write(
        f.build_home.join(".swarm/skills/.DS_Store"),
        "Finder metadata",
    )
    .unwrap();
    assert_eq!(f.status()["skills"], true);
    let plan = f.plan();
    assert_eq!(plan["files"], serde_json::json!([]));
    assert_eq!(plan["conflicts"], serde_json::json!([]));
}
