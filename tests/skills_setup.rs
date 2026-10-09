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
fn clean_home_plans_and_records_all_69_links_in_the_selected_build_home() {
    let f = Fixture::new("clean-owner", true);
    assert_eq!(f.status()["skills"], false);
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
