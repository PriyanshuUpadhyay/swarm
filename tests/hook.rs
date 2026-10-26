use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swarm-hook-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".swarm")).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("bin/open"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(
        dir.join("bin/open"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// `exe` with only HOME, PATH, and SWARM_HOME set. SWARM_HOME pins the data to HOME, so a branch
/// build (ADR 0027) reads the same place as a `main` one.
fn clean(exe: &Path, home: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        );
    command
}

/// Runs `command` with `stdin` as its input.
fn piped(mut command: Command, stdin: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A hook that is no swarm agent exits before it reads stdin, so the write may hit EPIPE.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    child.wait_with_output().unwrap()
}

/// Runs the built binary in a clean env with the given variables added.
fn swarm(home: &Path, env: &[(&str, &str)], args: &[&str], stdin: &str) -> Output {
    let mut command = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), home);
    command
        .env("SWARM_ADAPTER", "tmux")
        .envs(env.iter().copied())
        .current_dir(home)
        .args(args);
    piped(command, stdin)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn coder_state(home: &Path, session: &str) -> serde_json::Value {
    let listing = swarm(
        home,
        &[("SWARM_SESSION_ID", session)],
        &["agents", "--json"],
        "",
    );
    let listing: serde_json::Value = serde_json::from_str(&stdout(&listing)).unwrap();
    let coder = &listing["agents"][0];
    assert_eq!(coder["id"], "coder");
    coder.clone()
}

/// `name` run as the built binary at its own path, as the brew CLI and the app's helper
/// are two paths on one Mac.
fn build_at(home: &Path, name: &str) -> PathBuf {
    let dir = home.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("swarm");
    // A hard link opens no file for write. A copy does, and on Linux another test's fork can hold
    // that write handle while this test runs the file, so exec fails with ETXTBSY.
    // The home is wiped first, but that wipe ignores its error, so a link from an earlier run
    // can stay. Removing it keeps the copy below from writing through it into the cargo binary.
    let _ = std::fs::remove_file(&exe);
    std::fs::hard_link(env!("CARGO_BIN_EXE_swarm"), &exe)
        .or_else(|_| std::fs::copy(env!("CARGO_BIN_EXE_swarm"), &exe).map(drop))
        .unwrap();
    exe
}

/// `swarm hooks <args>`, where `setup <rest>` runs `swarm setup hooks <rest>`, with `--yes` for an
/// apply with no digest.
fn hooks(exe: &Path, home: &Path, args: &[&str]) -> Output {
    let mut command = clean(exe, home);
    match args {
        ["setup", rest @ ..] => {
            command.args(["setup", "hooks"]).args(rest);
            if rest.is_empty() {
                command.arg("--yes");
            }
        }
        _ => {
            command.arg("hooks").args(args);
        }
    }
    piped(command, "")
}

/// Runs a hook command as Codex and AGY do, through the user's shell, with no `swarm` on PATH.
fn run_hook(shell: &str, command: &str, env: &[(&str, &str)], stdin: &str) -> Output {
    let mut hook = Command::new(shell);
    hook.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .envs(env.iter().copied())
        .args(["-c", command]);
    piped(hook, stdin)
}

/// The user shells on this machine that a hook may run in; fish is not in the macOS base system.
fn shells() -> Vec<&'static str> {
    [
        "/bin/sh",
        "/bin/zsh",
        "/bin/csh",
        "/opt/homebrew/bin/fish",
        "/usr/bin/fish",
    ]
    .into_iter()
    .filter(|shell| Path::new(shell).exists())
    .collect()
}

/// Each hook file's bytes, or None for a missing one, so a test can show that nothing changed.
fn hook_files(home: &Path) -> [Option<Vec<u8>>; 2] {
    [".codex/config.toml", ".gemini/config/hooks.json"]
        .map(|file| std::fs::read(home.join(file)).ok())
}

fn setup(home: &Path, args: &[&str]) -> Output {
    let mut command = clean(Path::new(env!("CARGO_BIN_EXE_swarm")), home);
    command.current_dir(home).arg("setup");
    // These fixtures cover the file items, which need no bundled kit or HUD app.
    if args.first().is_none_or(|arg| arg.starts_with('-')) {
        command.arg("hooks,trust");
    }
    command.args(args);
    if args.first() != Some(&"status")
        && !args.iter().any(|arg| matches!(*arg, "--plan" | "--digest"))
    {
        command.arg("--yes");
    }
    piped(command, "")
}

fn git_repo(home: &Path, name: &str) -> PathBuf {
    let dir = home.join(name);
    assert!(
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    dir
}

/// Waits up to 1 s for the stand-in `open` to record its arguments.
/// The `open` stand-in's log once it holds `lines` whole lines, or what it holds at the deadline.
/// swarm starts `open` and does not wait for it.
fn opened(home: &Path, lines: usize) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let text = std::fs::read_to_string(home.join("opened")).ok();
        let whole = text.as_deref().map_or(0, |text| text.matches('\n').count());
        if whole >= lines || std::time::Instant::now() >= deadline {
            return text;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The `open` arguments that start the notice helper at `helper` for this test's home (ADR 0077).
fn helper_start(home: &Path, helper: &Path) -> String {
    format!(
        "-g {} --env SWARM_HOME={} --env SWARM_BIN={}\n",
        helper.display(),
        home.display(),
        env!("CARGO_BIN_EXE_swarm")
    )
}

#[path = "hook/consent.rs"]
mod consent;
#[path = "hook/focus.rs"]
mod focus;
#[path = "hook/guards.rs"]
mod guards;
#[path = "hook/hook_setup.rs"]
mod hook_setup;
#[path = "hook/notices.rs"]
mod notices;
#[path = "hook/setup_trust.rs"]
mod setup_trust;
#[path = "hook/state_context.rs"]
mod state_context;
