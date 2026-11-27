// Provider launch arguments.
use super::*;

pub fn valid_agent_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=40).contains(&bytes.len())
        && bytes
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// Hosts a building seat may reach through the owner's Codex network proxy: Python, Rust, npm,
/// and SwiftPM packages, and Hugging Face model files.
const PACKAGE_HOSTS: &[&str] = &[
    "pypi.org",
    "files.pythonhosted.org",
    "crates.io",
    "**.crates.io",
    "registry.npmjs.org",
    "github.com",
    "**.github.com",
    "**.githubusercontent.com",
    "huggingface.co",
    "**.huggingface.co",
    "**.hf.co",
];

/// Internal launch option for a new linked chair with no model to carry forward.
pub const CLI_DEFAULT_MODEL_FLAG: &str = "--cli-default-model";

pub fn argv(
    agent_id: &str,
    role: &str,
    resolved: &crate::config::Runner,
    swarm_home: &str,
    cwd: &std::path::Path,
) -> Result<Vec<String>, String> {
    launch_argv(
        agent_id,
        role,
        resolved,
        swarm_home,
        cwd,
        Some(&resolved.model),
    )
}

pub fn argv_with_cli_default(
    agent_id: &str,
    role: &str,
    resolved: &crate::config::Runner,
    swarm_home: &str,
    cwd: &std::path::Path,
) -> Result<Vec<String>, String> {
    launch_argv(agent_id, role, resolved, swarm_home, cwd, None)
}

fn launch_argv(
    agent_id: &str,
    role: &str,
    resolved: &crate::config::Runner,
    swarm_home: &str,
    cwd: &std::path::Path,
    model: Option<&str>,
) -> Result<Vec<String>, String> {
    let provider = resolved.provider;
    let effort = resolved.effort.as_str();

    // In tmux-swarm, `swarm pty` answers Pi's program-status probe (ADR 0123).
    // The environment setting also lets Pi send it on other hosts.
    let mut args = if provider == Provider::Pi {
        vec![
            "env".into(),
            "PI_PROGRAM_STATUS=1".into(),
            provider.id().into(),
        ]
    } else {
        vec![provider.id().into()]
    };
    if let Some(model) = model
        .filter(|model| *model != "default" || !matches!(provider, Provider::Agy | Provider::Pi))
    {
        args.extend(["--model".into(), model.into()]);
    }

    match provider {
        Provider::Claude => {
            args.extend(["--effort".into(), effort.into()]);
            if let Some(permission) = &resolved.permission {
                args.extend(["--permission-mode".into(), permission.clone()]);
            }
            args.extend(hook_args(provider, agent_id, cwd)?);
            Ok(args)
        }
        Provider::Codex => {
            args.extend([
                "-c".into(),
                format!("model_reasoning_effort=\"{}\"", effort),
                // Codex asks "Update available! 1. Update now 2. Skip" before it reads anything,
                // and an agent pane has nobody at it to answer.
                "-c".into(),
                "check_for_update_on_startup=false".into(),
                // A login shell runs the owner's profile, and `brew shellenv` there puts
                // /opt/homebrew/bin back in front of the pane's swarm link, so the seat ran another
                // swarm build. The pane already holds the login environment.
                "-c".into(),
                "allow_login_shell=false".into(),
            ]);
            match resolved.sandbox.as_deref() {
                // `--sandbox workspace-write` keeps `.git` read-only, so a seat cannot commit. A
                // permissions profile opens `.git` and keeps its hooks and config, `.codex` and
                // `.agents` read-only. Submodule git dirs under `modules` stay read-only too, because
                // their hooks and config run code when the owner next uses git there. The profile
                // replaces `--sandbox`, which forces the legacy sandbox.
                Some("workspace-write") => {
                    let quote = |path: &str| {
                        serde_json::to_string(path).expect("string serialization cannot fail")
                    };
                    // Codex refuses every command when a writable root has a symlink in its path.
                    let root = format!("{swarm_home}/.swarm");
                    let root = std::fs::canonicalize(&root)
                        .map_or(root, |path| path.to_string_lossy().into_owned());
                    // ADR 0135: a chat in a hub's main worktree reads main and writes only its
                    // `tmp/` and the task worktrees beside it, which Worktrunk makes there. The
                    // workspace root must say `read` itself; a path entry for it does not win.
                    // Main's `tmp/` counts as main here, so its chat does not get main's git dir.
                    let here = crate::task::resolve(cwd, std::path::Path::new("."));
                    let in_hub = crate::task::hub(cwd);
                    let main = in_hub
                        .as_ref()
                        .filter(|hub| here.starts_with(&hub.main))
                        .map(|hub| hub.main.clone());
                    let roots = if main.is_some() {
                        r#""."="read",".codex"="read",".agents"="read""#
                    } else {
                        r#"".git"="write",".git/hooks"="read",".git/config"="read",".git/modules"="read",".codex"="read",".agents"="read""#
                    };
                    let mut filesystem =
                        format!(r#"":workspace_roots"={{{roots}}},{}="write""#, quote(&root));
                    // In a subfolder of main, `.` is only that subfolder, so a write root above
                    // main would open the rest of main; such a chat gets no write roots here.
                    let at_main = main.as_ref().is_some_and(|main| here == *main);
                    if let Some(main) = main.as_ref().filter(|_| at_main) {
                        let path = |path: &std::path::Path| quote(&path.to_string_lossy());
                        filesystem += &format!(
                            r#",{}="write",{}="write""#,
                            path(main.parent().unwrap_or(main)),
                            path(&main.join(crate::task::SCRATCH))
                        );
                    }
                    // A linked worktree keeps its git dir in the common dir, outside the workspace.
                    if let Some((git_dir, common)) = outside_git_dirs(cwd) {
                        filesystem += &format!(
                            r#",{}="write",{}="read",{}="read",{}="read""#,
                            quote(&common),
                            quote(&format!("{common}/hooks")),
                            quote(&format!("{common}/config")),
                            quote(&format!("{common}/modules"))
                        );
                        // In a hub, the files that move or retarget the default branch are read,
                        // on main and in a task worktree. Codex also refuses to create a `read`
                        // file, so the loose default ref is named even when only packed-refs holds
                        // it; packed-refs is named in that case too.
                        if let Some(hub) = &in_hub {
                            let loose = hub.common.join(format!("refs/heads/{}", hub.default));
                            // `default_branch` reads origin/HEAD and `hub` reads main's HEAD, so
                            // a write to either can turn the wall off; both are named even when
                            // missing, so a `fetch` that would create origin/HEAD fails in Codex.
                            let mut files = vec![
                                hub.common.join("HEAD"),
                                hub.common.join("refs/remotes/origin/HEAD"),
                            ];
                            // On main, its git dir is named below; a TOML key may not repeat.
                            if let Some((main_git_dir, _)) =
                                outside_git_dirs(&hub.main).filter(|_| main.is_none())
                            {
                                files.push(main_git_dir.into());
                            }
                            if !loose.exists() {
                                files.push(hub.common.join("packed-refs"));
                            }
                            files.push(loose);
                            for file in files {
                                filesystem +=
                                    &format!(r#",{}="read""#, quote(&file.to_string_lossy()));
                            }
                        }
                        if git_dir != common {
                            // Main's own git dir holds its index and HEAD, so it is read-only too.
                            let access = if main.is_some() { "read" } else { "write" };
                            filesystem += &format!(
                                r#",{}="{access}",{}="read""#,
                                quote(&git_dir),
                                quote(&format!("{git_dir}/config.worktree"))
                            );
                        }
                    }
                    // A seat's reply rings its chair with `tmux -L swarm` on a tmux host, or through
                    // a PTY host's socket (ADR 0131), and the sandbox refuses every AF_UNIX socket
                    // that it does not name.
                    // SAFETY: getuid has no preconditions and cannot fail.
                    let uid = unsafe { crate::hud::getuid() };
                    let tmux = tmux_socket_dir(std::env::var("TMUX_TMPDIR").ok().as_deref(), uid);
                    let mut sockets = format!(r#"{}="allow""#, quote(&tmux));
                    let chair_session = (agent_id == "orchestrator")
                        .then(|| std::env::var("SWARM_SESSION_ID").ok())
                        .flatten();
                    let host = std::env::var("SWARM_PTY_HOST").ok();
                    if let Some(pty) = pty_socket(host.as_deref(), chair_session.as_deref(), uid) {
                        sockets += &format!(r#",{}="allow""#, quote(&pty));
                    }
                    let builds = role.starts_with("code.") || role == "build" || role == "checker";
                    // uv's default cache is outside the writable roots, so uv cannot lock it.
                    if builds {
                        let cache = std::env::temp_dir().join("swarm-uv-cache");
                        args.extend([
                            "-c".into(),
                            format!(
                                "shell_environment_policy.set.UV_CACHE_DIR={}",
                                quote(&cache.to_string_lossy())
                            ),
                        ]);
                    }
                    // The owner's network proxy allows few hosts. A seat that builds or tests
                    // fetches packages, and only these package hosts are added for it.
                    let domains = if builds {
                        let hosts: Vec<String> = PACKAGE_HOSTS
                            .iter()
                            .map(|host| format!(r#"{}="allow""#, quote(host)))
                            .collect();
                        format!(",domains={{{}}}", hosts.join(","))
                    } else {
                        String::new()
                    };
                    args.extend([
                        "-c".into(),
                        r#"default_permissions="swarm-code""#.into(),
                        "-c".into(),
                        format!(
                            r#"permissions.swarm-code={{extends=":workspace",filesystem={{{filesystem}}},network={{enabled=true,unix_sockets={{{sockets}}}{domains}}}}}"#
                        ),
                    ]);
                }
                Some(sandbox) => args.extend(["--sandbox".into(), sandbox.into()]),
                None => {}
            }
            if let Some(approval) = &resolved.approval {
                args.extend(["--ask-for-approval".into(), approval.clone()]);
            }
            args.extend(hook_args(provider, agent_id, cwd)?);

            args.extend(owner_choice::codex_args());
            Ok(args)
        }
        Provider::Agy => {
            if model.is_none_or(agy_takes_effort) {
                args.extend(["--effort".into(), effort.into()]);
            }
            if let Some(permission) = &resolved.permission {
                if permission == "skip" {
                    args.push("--dangerously-skip-permissions".into());
                } else {
                    args.extend(["--mode".into(), permission.clone()]);
                }
            }
            Ok(args)
        }
        Provider::Pi => {
            args.extend(["--thinking".into(), effort.into()]);
            Ok(args)
        }
    }
}

/// Whether AGY takes `--effort` for `model`. An id such as `gemini-3.8-flash-high` fixes its own
/// effort, and AGY refuses `--effort` for it and for a Claude model such as `claude-sonnet-4-6`
/// (probe 2026-10-21).
// ponytail: a name rule from `agy models`; read AGY's own per-model flag if it gets one.
pub fn agy_takes_effort(model: &str) -> bool {
    !(model.starts_with("claude-")
        || ["-low", "-medium", "-high"]
            .iter()
            .any(|level| model.ends_with(level)))
}

/// The model a provider command line names with `--model` or `-m`, without a `[1m]` suffix.
pub fn command_model(command: &[String]) -> Option<&str> {
    let mut args = command.iter().take_while(|arg| *arg != "--");
    while let Some(arg) = args.next() {
        let model = match arg.strip_prefix("--model=") {
            Some(model) => model,
            None if arg == "--model" || arg == "-m" => args.next()?,
            None => continue,
        };
        return model.split('[').next();
    }
    None
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    find_flag(args, flags).is_some()
}

/// The first of `flags` that `args` sets, as `--flag` or `--flag=value`.
fn find_flag<'a>(args: &'a [String], flags: &[&str]) -> Option<&'a str> {
    // Everything past a bare `--` is a positional prompt, not a flag.
    args.iter()
        .take_while(|arg| *arg != "--")
        .map(|arg| arg.split('=').next().unwrap_or(arg))
        .find(|flag| flags.contains(flag))
}

/// The caller's extra args for `provider`, checked and in the provider's shape. `force` lets the
/// role's flags through, but not the locked ones.
pub fn extra_args(
    provider: Provider,
    extra: &[String],
    force: bool,
) -> Result<Vec<String>, String> {
    if let Some(flag) = find_flag(extra, provider.locked_flags()) {
        let fix = match flag {
            "-m" | "--model" => "use launch --model",
            "--effort" | "--thinking" => "use launch --effort",
            "--provider" => "use launch --provider and --model",
            _ => "swarm sets it, so the chat log stays found",
        };
        return Err(format!(
            "swarm: launch owns the {} flag {flag}, also with --force; {fix}",
            provider.id()
        ));
    }
    if !force && let Some(flag) = find_flag(extra, provider.owned_flags()) {
        return Err(format!(
            "swarm: the role owns the {} flag {flag}; remove it from the extra args, or add --force",
            provider.id()
        ));
    }
    // AGY reads a leading positional as a one-shot prompt; `-i` keeps the pane interactive.
    if provider == Provider::Agy && extra.first().is_some_and(|arg| !arg.starts_with('-')) {
        return Ok([vec!["-i".to_string()], extra.to_vec()].concat());
    }
    Ok(extra.to_vec())
}

/// Where a Claude child runs and the args that go with it. Claude files a transcript under its
/// cwd and has no way to move it, so a child runs from a pool dir under the caller's tree to stay
/// out of the caller's /resume picker; `--add-dir` gives it the tree back. Its session id takes a
/// reserved prefix so tools can tell a worker transcript apart. A resuming child keeps its cwd,
/// because its transcript already lives under the original one.
pub fn claude_child(
    agent_id: &str,
    cwd: &std::path::Path,
    extra: &[String],
    session_id: &uuid::Uuid,
) -> (std::path::PathBuf, Vec<String>) {
    const RESUME: &[&str] = &["-r", "--resume", "-c", "--continue", "--fork-session"];
    if has_flag(extra, RESUME) {
        return (cwd.to_path_buf(), extra.to_vec());
    }
    let mut args = Vec::new();
    if !has_flag(extra, &["--session-id"]) {
        let id = session_id.to_string();
        args.extend(["--session-id".to_string(), format!("aaaaaaaa{}", &id[8..])]);
        if !has_flag(extra, &["-n", "--name"]) {
            args.extend(["-n".to_string(), agent_id.to_string()]);
        }
    }
    args.extend_from_slice(extra);
    args.extend(["--add-dir".to_string(), cwd.to_string_lossy().into_owned()]);
    (cwd.join(".herdr").join("workers"), args)
}

/// The git dir and common dir of the repo at canonical `cwd`, when its git dir is outside `cwd`.
fn outside_git_dirs(cwd: &std::path::Path) -> Option<(String, String)> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--absolute-git-dir", "--git-common-dir"])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout);
    // The common dir can print relative to cwd.
    let mut dirs = text
        .lines()
        .map(|line| std::fs::canonicalize(cwd.join(line)).ok());
    let (git_dir, common) = (dirs.next()??, dirs.next()??);
    (!git_dir.starts_with(cwd)).then(|| {
        (
            git_dir.to_string_lossy().into_owned(),
            common.to_string_lossy().into_owned(),
        )
    })
}
