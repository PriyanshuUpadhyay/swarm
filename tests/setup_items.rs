//! `swarm setup plugin` and `swarm setup hud` as plan items with a digest (ADR 0092).

use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Fixture {
    home: PathBuf,
    exe: PathBuf,
}

impl Fixture {
    /// A home with `swarm` copied into `prefix/bin`, a kit in `prefix/libexec/skills/kit`, and
    /// fake `claude` and `pi` on PATH that log each call to `calls`.
    fn new(name: &str) -> Self {
        let home = std::env::temp_dir().join(format!("swarm-items-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".swarm")).unwrap();
        let home = std::fs::canonicalize(home).unwrap();
        let kit = home.join("prefix/libexec/skills/kit");
        std::fs::create_dir_all(kit.join(".claude-plugin")).unwrap();
        std::fs::write(kit.join(".claude-plugin/marketplace.json"), "{}").unwrap();
        for skill in ["flow", "swarm-voice"] {
            std::fs::create_dir_all(kit.join("skills").join(skill)).unwrap();
            std::fs::write(kit.join("skills").join(skill).join("SKILL.md"), "---\n").unwrap();
        }
        std::fs::create_dir_all(home.join("prefix/bin")).unwrap();
        let exe = home.join("prefix/bin/swarm");
        std::fs::copy(env!("CARGO_BIN_EXE_swarm"), &exe).unwrap();
        std::fs::create_dir_all(home.join("bin")).unwrap();
        for cli in ["claude", "pi"] {
            let path = home.join("bin").join(cli);
            std::fs::write(
                &path,
                format!("#!/bin/sh\necho \"{cli} $*\" >> \"$HOME/calls\"\n"),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { home, exe }
    }

    fn run(&self, env: &[(&str, &str)], args: &[&str]) -> Output {
        let mut child = Command::new(&self.exe)
            .env_clear()
            .env("HOME", &self.home)
            .env("SWARM_HOME", &self.home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.home.join("bin").display()),
            )
            .envs(env.iter().copied())
            .current_dir(&self.home)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdin.take());
        child.wait_with_output().unwrap()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.home.join("calls")).unwrap_or_default()
    }

    /// A skill link that an older swarm's `skills` setup made and recorded.
    fn old_link(&self, name: &str) -> PathBuf {
        let target = self.home.join(".swarm/skills/kit/skills").join(name);
        std::fs::create_dir_all(&target).unwrap();
        let link = self.home.join(".claude/skills").join(name);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        let store = swarm::store::open(&self.home.join(".swarm/swarm.db")).unwrap();
        let plan = swarm::managed::LinkPlan::create(&store, link.clone(), target).unwrap();
        swarm::managed::apply(&store, &[plan.into()]).unwrap();
        assert!(link.is_symlink());
        link
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn text(output: &[u8]) -> String {
    String::from_utf8_lossy(output).into_owned()
}

#[test]
fn the_plugin_plan_names_each_command_and_its_digest_applies_only_that_plan() {
    let f = Fixture::new("plugin");
    let old = f.old_link("flow");
    let owners = f.home.join(".claude/skills/mine");
    std::os::unix::fs::symlink(&f.home, &owners).unwrap();

    let plan = f.run(&[], &["setup", "plugin", "--plan"]);
    assert!(plan.status.success(), "{plan:?}");
    let shown = text(&plan.stdout);
    let link = f.home.join(".swarm/kit");
    for line in [
        format!("link {} -> ", link.display()),
        format!(
            "run  'claude' 'plugin' 'marketplace' 'add' '{}'",
            link.display()
        ),
        "run  'claude' 'plugin' 'install' 'swarm@swarm'".into(),
        "run  'claude' 'plugin' 'update' 'swarm@swarm' '--scope' 'user'".into(),
        format!("run  'pi' 'install' '{}'", link.display()),
        "skipped (plugin): codex is not on PATH".into(),
        format!("--- {}", old.display()),
        "Run `swarm setup plugin --digest ".into(),
    ] {
        assert!(shown.contains(&line), "{line}\n{shown}");
    }
    // The old link goes, so only the owner's own link is named as theirs.
    assert!(!shown.contains("flow is your own skill"), "{shown}");
    assert!(f.calls().is_empty(), "a plan runs no install");
    assert!(old.is_symlink());

    let json = f.run(&[], &["setup", "plugin", "--plan", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let digest = json["digest"].as_str().unwrap().to_string();
    assert_eq!(json["items"], serde_json::json!(["plugin"]));
    assert_eq!(json["commands"][0]["group"], "plugin");
    assert_eq!(json["files"][0]["path"], old.to_string_lossy().as_ref());

    // A pane is no owner, and a run with no terminal needs --yes or the digest.
    let child = f.run(
        &[("SWARM_AGENT_ID", "coder")],
        &["setup", "plugin", "--digest", &digest],
    );
    assert!(
        text(&child.stderr).contains("a child agent cannot"),
        "{child:?}"
    );
    let no_tty = f.run(&[], &["setup", "plugin"]);
    assert!(!no_tty.status.success());
    assert!(
        text(&no_tty.stderr).contains("run `swarm setup plugin --yes`"),
        "{no_tty:?}"
    );
    assert!(f.calls().is_empty() && old.is_symlink() && !link.exists());

    // An old link the owner points elsewhere after the plan changes the digest, so nothing runs.
    let target = std::fs::read_link(&old).unwrap();
    std::fs::remove_file(&old).unwrap();
    std::os::unix::fs::symlink(&f.home, &old).unwrap();
    let stale = f.run(&[], &["setup", "plugin", "--digest", &digest]);
    assert!(
        text(&stale.stderr).contains("changed after the plan"),
        "{stale:?}"
    );
    assert!(f.calls().is_empty() && !link.exists());
    std::fs::remove_file(&old).unwrap();
    std::os::unix::fs::symlink(&target, &old).unwrap();

    let applied = f.run(&[], &["setup", "plugin", "--digest", &digest]);
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        f.calls(),
        format!(
            "claude plugin marketplace add {0}\nclaude plugin install swarm@swarm\nclaude plugin update swarm@swarm --scope user\npi install {0}\n",
            link.display()
        )
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        f.home.join("prefix/libexec/skills/kit")
    );
    assert!(!old.exists() && owners.is_symlink());
    let doctor = f.run(&[], &["doctor", "--json"]);
    let doctor: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let row = doctor["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "plugin")
        .unwrap();
    assert_eq!(row["state"], "ok", "{row}");
}

#[test]
fn doctor_and_launch_report_stale_copies_without_running_a_cli() {
    let f = Fixture::new("stale-plugin");
    std::fs::write(f.home.join(".swarm/swarm-home"), "swarm\n").unwrap();
    let kit = f.home.join("prefix/libexec/skills/kit");
    std::fs::write(kit.join("plugin.json"), r#"{"version":"0.1.0+new"}"#).unwrap();
    let claude = f.home.join(".claude/plugins");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("installed_plugins.json"),
        r#"{"version":2,"plugins":{"swarm@swarm":[{"scope":"user","version":"0.1.0+old"}]}}"#,
    )
    .unwrap();
    for folder in [
        ".codex/plugins/cache/swarm/swarm/0.1.0+old",
        ".codex-work/plugins/cache/swarm/swarm/0.1.0+old",
        ".swarm/accounts/codex/work/plugins/cache/swarm/swarm/0.1.0+old",
        ".gemini/config/plugins/swarm",
    ] {
        let folder = f.home.join(folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("plugin.json"), r#"{"version":"0.1.0+old"}"#).unwrap();
    }
    let out = f.run(&[], &["doctor", "--json"]);
    assert!(out.status.success(), "{out:?}");
    let doctor: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let stale: Vec<_> = doctor["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"].as_str().unwrap().starts_with("plugin-"))
        .collect();
    assert_eq!(stale.len(), 5, "{doctor}");
    for row in stale {
        assert_eq!(row["state"], "setup");
        assert_eq!(row["group"], "plugin");
        assert!(
            row["reason"]
                .as_str()
                .unwrap()
                .contains("swarm setup plugin")
        );
    }
    let launch = f.run(&[], &["launch", "bad/id", "chat"]);
    let stderr = text(&launch.stderr);
    let notices: Vec<_> = stderr
        .lines()
        .filter(|line| line.contains("stale plugin"))
        .collect();
    assert_eq!(notices.len(), 1, "{stderr}");
    for name in ["Claude", "Codex", "AGY", "swarm setup plugin"] {
        assert!(notices[0].contains(name), "{stderr}");
    }
    assert!(f.calls().is_empty(), "a stale check must not run a CLI");
}

#[test]
fn the_hud_plan_shows_its_launch_agent_and_old_commands_name_the_new_one() {
    let f = Fixture::new("hud");
    let app = f.home.join("SwarmHUD.app");
    std::fs::create_dir_all(&app).unwrap();
    let env = [("SWARM_HUD_APP", app.to_str().unwrap())];
    let plan = f.run(&env, &["setup", "hud", "--plan"]);
    assert!(plan.status.success(), "{plan:?}");
    let shown = text(&plan.stdout);
    let agent = f
        .home
        .join("Library/LaunchAgents/io.github.priyanshuupadhyay.swarm.hud.plist");
    for line in [
        format!("+++ {}", agent.display()),
        format!("<string>{}", app.display()),
        "run  launchctl bootstrap".into(),
        "Run `swarm setup hud --digest ".into(),
    ] {
        assert!(shown.contains(&line), "{line}\n{shown}");
    }
    assert!(!agent.exists());
    let json = f.run(&env, &["setup", "hud", "--plan", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["files"][0]["group"], "hud");

    for (args, new) in [
        (&["hud", "install"][..], "swarm setup hud"),
        (&["hooks", "setup"], "swarm setup hooks"),
        (&["setup", "--only", "hooks"], "swarm setup hooks,trust"),
    ] {
        let old = f.run(&env, args);
        assert!(!old.status.success());
        assert!(text(&old.stderr).contains(new), "{args:?}: {old:?}");
    }
    let unknown = f.run(&env, &["setup", "skills"]);
    assert!(
        text(&unknown.stderr).contains("unknown setup item skills"),
        "{unknown:?}"
    );
}

// BSD `script` takes the command after the log file; the Linux one needs `-c`.
#[cfg(target_os = "macos")]
#[test]
fn a_terminal_run_asks_and_only_a_yes_applies() {
    use std::{
        io::{Read, Write},
        path::Path,
    };
    let f = Fixture::new("tty");
    // `script` gives the run a terminal. The answer is typed once the question shows, because an
    // end of input that reaches the terminal first reads as no answer.
    let ask = |answer: &str| {
        let mut child = Command::new("/usr/bin/script")
            .args(["-q", "/dev/null"])
            .arg(&f.exe)
            .args(["setup", "plugin"])
            .env_clear()
            .env("HOME", &f.home)
            .env("SWARM_HOME", &f.home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", f.home.join("bin").display()),
            )
            .current_dir(&f.home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = child.stdout.take().unwrap();
        let mut shown = Vec::new();
        let mut chunk = [0; 4096];
        while !text(&shown).contains("Apply this plan? [y/N]") {
            let read = out.read(&mut chunk).unwrap();
            assert!(read > 0, "{}", text(&shown));
            shown.extend_from_slice(&chunk[..read]);
        }
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(answer.as_bytes()).unwrap();
        // Wait for the reply before closing input, so the close cannot become the answer.
        let reply = if answer == "y\n" {
            "is installed"
        } else {
            "nothing changed"
        };
        while !text(&shown).contains(reply) {
            let read = out.read(&mut chunk).unwrap();
            assert!(read > 0, "{}", text(&shown));
            shown.extend_from_slice(&chunk[..read]);
        }
        drop(stdin);
        out.read_to_end(&mut shown).unwrap();
        assert!(child.wait().unwrap().success());
        text(&shown)
    };
    ask("n\n");
    assert!(f.calls().is_empty() && !f.home.join(".swarm/kit").exists());
    ask("y\n");
    assert!(f.calls().contains("pi install"), "{}", f.calls());
    assert!(Path::new(&f.home.join(".swarm/kit")).is_symlink());
}
