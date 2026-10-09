use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

struct Fixture {
    root: PathBuf,
    helper: PathBuf,
    source: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn new(role: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swarm-skills-{role}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let helper = root.join("Swarm.app/Contents/Helpers/swarm");
        let source = root.join("Swarm.app/Contents/Resources/Skills");
        fs::create_dir_all(helper.parent().unwrap()).unwrap();
        fs::write(&helper, "fixture helper").unwrap();
        fs::create_dir_all(&source).unwrap();
        assert!(
            Command::new("cp")
                .args(["-Rp"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skills/."))
                .arg(&source)
                .status()
                .unwrap()
                .success()
        );
        swarm::skills::write_manifest(&source, &source.join("manifest.json")).unwrap();
        let home = root.join("branch/.swarm");
        fs::create_dir_all(&home).unwrap();
        Self {
            root,
            helper,
            source,
            home,
        }
    }
    fn refresh(&self) -> Result<swarm::skills::RefreshOutcome, Box<dyn std::error::Error>> {
        swarm::skills::refresh_from(&self.helper, &self.home)
    }
    fn change(&self) {
        fs::write(
            self.source.join("kit/references/role.md"),
            "changed version\n",
        )
        .unwrap();
        swarm::skills::write_manifest(&self.source, &self.source.join("manifest.json")).unwrap();
    }
    fn installed(&self) -> swarm::skills::Manifest {
        swarm::skills::Manifest::read(&self.home.join("skills")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn first_copy_noop_and_whole_replacement_use_cask_source() {
    let f = Fixture::new("first-copy");
    let cask = f.root.join("cask/swarm");
    fs::create_dir_all(cask.parent().unwrap()).unwrap();
    symlink(&f.helper, &cask).unwrap();
    assert_eq!(
        swarm::skills::refresh_from(&cask, &f.home).unwrap(),
        swarm::skills::RefreshOutcome::Updated
    );
    let before = f.installed();
    assert_eq!(
        fs::read(f.source.join("manifest.json")).unwrap(),
        fs::read(f.home.join("skills/manifest.json")).unwrap()
    );
    let modified = fs::metadata(f.home.join("skills/manifest.json"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        f.refresh().unwrap(),
        swarm::skills::RefreshOutcome::Unchanged
    );
    assert_eq!(
        fs::metadata(f.home.join("skills/manifest.json"))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    assert_eq!(
        fs::read_to_string(f.home.join("skills-version"))
            .unwrap()
            .trim(),
        before.content_id
    );
    fs::write(
        f.home.join("skills/owner-extra"),
        "discard on version refresh",
    )
    .unwrap();
    f.change();
    assert_eq!(f.refresh().unwrap(), swarm::skills::RefreshOutcome::Updated);
    assert!(!f.home.join("skills/owner-extra").exists());
    assert_ne!(f.installed().content_id, before.content_id);
    assert_eq!(
        fs::metadata(f.home.join("skills/kit/scripts/role.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[test]
fn incomplete_same_id_is_repaired() {
    let f = Fixture::new("repair");
    f.refresh().unwrap();
    fs::remove_file(f.home.join("skills/swarm-voice/SKILL.md")).unwrap();
    assert_eq!(f.refresh().unwrap(), swarm::skills::RefreshOutcome::Updated);
    f.installed();
}

#[test]
fn interrupted_promotion_recovers_old_or_new_before_missing_source_error() {
    for state in ["old", "new", "first-stage"] {
        let f = Fixture::new(&format!("interrupted-{state}"));
        f.refresh().unwrap();
        let old = f.installed().content_id;
        if state == "first-stage" {
            fs::remove_dir_all(f.home.join("skills")).unwrap();
            fs::remove_file(f.home.join("skills-version")).unwrap();
        } else {
            fs::rename(f.home.join("skills"), f.home.join(".skills-previous")).unwrap();
        }
        if state == "new" {
            f.change();
            fs::create_dir_all(f.home.join("skills")).unwrap();
            assert!(
                Command::new("cp")
                    .args(["-Rp"])
                    .arg(f.source.join("."))
                    .arg(f.home.join("skills"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
        if state == "first-stage" {
            f.change();
            fs::create_dir(f.home.join(".skills-stage")).unwrap();
            assert!(
                Command::new("cp")
                    .args(["-Rp"])
                    .arg(f.source.join("."))
                    .arg(f.home.join(".skills-stage"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::remove_dir_all(&f.source).unwrap();
        assert!(f.refresh().is_err());
        let recovered = f.installed().content_id;
        assert_eq!(recovered == old, state == "old");
        assert_eq!(
            fs::read_to_string(f.home.join("skills-version"))
                .unwrap()
                .trim(),
            recovered
        );
        assert!(!f.home.join(".skills-previous").exists());
    }
}

#[test]
fn missing_source_invalid_manifest_copy_and_marker_failure_keep_prior_copy() {
    for failure in ["missing", "manifest", "copy", "unreadable-home", "marker"] {
        let f = Fixture::new(&format!("failure-{failure}"));
        f.refresh().unwrap();
        let old = f.installed().content_id;
        f.change();
        match failure {
            "missing" => fs::remove_dir_all(&f.source).unwrap(),
            "manifest" => fs::write(f.source.join("manifest.json"), "{}").unwrap(),
            "copy" => fs::set_permissions(&f.home, fs::Permissions::from_mode(0o555)).unwrap(),
            "unreadable-home" => {
                fs::set_permissions(&f.home, fs::Permissions::from_mode(0o000)).unwrap()
            }
            "marker" => {
                fs::remove_file(f.home.join("skills-version")).unwrap();
                fs::create_dir(f.home.join("skills-version")).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(f.refresh().is_err(), "{failure}");
        fs::set_permissions(&f.home, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(f.installed().content_id, old, "{failure}");
    }
}

#[test]
fn destination_and_transaction_links_never_change_owner_files() {
    for leaf in [
        "skills",
        ".skills-stage",
        ".skills-previous",
        ".skills-lock",
        "skills-version",
        ".skills-version-next",
    ] {
        let f = Fixture::new(&format!("link-{leaf}"));
        let owner = f.root.join("owner");
        fs::create_dir(&owner).unwrap();
        fs::write(owner.join("keep"), "owner data").unwrap();
        symlink(&owner, f.home.join(leaf)).unwrap();
        let result = f.refresh();
        if leaf == ".skills-lock" {
            assert!(result.is_err());
        } else {
            result.unwrap();
            f.installed();
        }
        assert_eq!(
            fs::read_to_string(owner.join("keep")).unwrap(),
            "owner data"
        );
        assert_eq!(fs::read_dir(&owner).unwrap().count(), 1);
    }
}

#[test]
fn held_home_lock_refuses_a_second_refresh() {
    let f = Fixture::new("lock");
    let held = fs::File::create(f.home.join(".skills-lock")).unwrap();
    held.lock().unwrap();
    let error = f.refresh().unwrap_err().to_string();
    assert!(error.contains("lock"), "{error}");
    assert!(!f.home.join("skills").exists());
}

#[test]
fn branch_and_release_copies_stay_apart() {
    let f = Fixture::new("home-isolation");
    let release =
        PathBuf::from(swarm::paths::branch_home(f.root.to_str().unwrap(), "")).join(".swarm");
    let branch = PathBuf::from(swarm::paths::branch_home(
        f.root.to_str().unwrap(),
        "feature",
    ))
    .join(".swarm");
    swarm::skills::refresh_from(&f.helper, &release).unwrap();
    let old = swarm::skills::Manifest::read(&release.join("skills"))
        .unwrap()
        .content_id;
    f.change();
    swarm::skills::refresh_from(&f.helper, &branch).unwrap();
    assert_eq!(
        swarm::skills::Manifest::read(&release.join("skills"))
            .unwrap()
            .content_id,
        old
    );
    assert_ne!(
        swarm::skills::Manifest::read(&branch.join("skills"))
            .unwrap()
            .content_id,
        old
    );
}

#[test]
fn skills_path_uses_the_claimed_build_home() {
    if let Ok(expected) = std::env::var("SWARM_TEST_SKILLS_EXPECTED") {
        assert_eq!(swarm::paths::skills_dir().unwrap(), PathBuf::from(expected));
        return;
    }
    let f = Fixture::new("path");
    let result = Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env("SWARM_HOME", f.home.parent().unwrap())
        .env("SWARM_TEST_SKILLS_EXPECTED", f.home.join("skills"))
        .args(["--exact", "skills_path_uses_the_claimed_build_home"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
}

#[test]
fn refresh_command_uses_no_flags_and_reports_source_errors() {
    let f = Fixture::new("command");
    fs::copy(env!("CARGO_BIN_EXE_swarm"), &f.helper).unwrap();
    let call = |helper: &Path, args: &[&str]| {
        Command::new(helper)
            .env_clear()
            .env("HOME", f.root.join("owner-home"))
            .env("SWARM_HOME", f.home.parent().unwrap())
            .args(args)
            .output()
            .unwrap()
    };
    for _ in 0..2 {
        let output = call(&f.helper, &["skills", "refresh"]);
        assert!(output.status.success(), "{output:?}");
        f.installed();
    }
    let before = f.installed().content_id;
    let flags = call(&f.helper, &["skills", "refresh", "--force"]);
    assert!(!flags.status.success());
    assert!(String::from_utf8_lossy(&flags.stderr).contains("usage"));
    fs::remove_dir_all(&f.source).unwrap();
    let missing = call(&f.helper, &["skills", "refresh"]);
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("missing bundle source"),
        "{missing:?}"
    );
    assert_eq!(f.installed().content_id, before);
    let bare = call(
        Path::new(env!("CARGO_BIN_EXE_swarm")),
        &["skills", "refresh"],
    );
    assert!(!bare.status.success());
    assert!(
        String::from_utf8_lossy(&bare.stderr).contains("missing bundle source"),
        "{bare:?}"
    );
}
