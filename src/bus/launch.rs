// Provider launch arguments and child-agent limits.
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

pub fn argv(
    agent_id: &str,
    role: &str,
    resolved: &ResolvedRole,
    swarm_home: &str,
    cwd: &std::path::Path,
) -> Result<Vec<String>, String> {
    let provider = required(role, "provider", resolved.provider.as_deref())?;
    let effort = || required(role, "effort", resolved.effort.as_deref());
    let model = || required(role, "model", resolved.model.as_deref());
    let Some(provider) = Provider::parse(provider) else {
        return Err(format!(
            "swarm: role {role} uses unsupported provider {provider}"
        ));
    };
    match provider {
        Provider::Claude => {
            let mut args = vec![
                provider.id().into(),
                "--model".into(),
                model()?.into(),
                "--effort".into(),
                effort()?.into(),
            ];
            if let Some(permission) = &resolved.permission {
                args.extend(["--permission-mode".into(), permission.clone()]);
            }
            let handler = |command: String| serde_json::json!({"hooks": [{"type": "command", "command": command, "timeout": STATE_HOOK_TIMEOUT}]});
            let state = state_hook_command(provider.id())?;
            let mut hooks = serde_json::Map::new();
            for event in CLAUDE_STATE_EVENTS {
                let mut group = handler(state.clone());
                if event == "PermissionRequest" {
                    group["hooks"][0]["timeout"] = crate::permission::HOOK_TIMEOUT_SECONDS.into();
                }
                hooks.insert(event.into(), serde_json::json!([group]));
            }
            // `/clear` opens a new log, so every Claude agent reports SessionStart state; the chair
            // also records its new session id.
            let mut start = vec![handler(state.clone())];
            if agent_id == "orchestrator" {
                start.insert(0, handler(chair_hook_command(provider.id())?));
            }
            hooks.insert("SessionStart".into(), start.into());
            args.extend([
                "--settings".into(),
                serde_json::json!({ "hooks": hooks }).to_string(),
            ]);
            Ok(args)
        }
        Provider::Codex => {
            let mut args = vec![
                provider.id().into(),
                "--model".into(),
                model()?.into(),
                "-c".into(),
                format!("model_reasoning_effort=\"{}\"", effort()?),
                // Codex asks "Update available! 1. Update now 2. Skip" before it reads anything,
                // and an agent pane has nobody at it to answer.
                "-c".into(),
                "check_for_update_on_startup=false".into(),
                // A login shell runs the owner's profile, and `brew shellenv` there puts
                // /opt/homebrew/bin back in front of the pane's swarm link, so the seat ran another
                // swarm build. The pane already holds the login environment.
                "-c".into(),
                "allow_login_shell=false".into(),
            ];
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
                    let mut filesystem = format!(
                        r#"":workspace_roots"={{".git"="write",".git/hooks"="read",".git/config"="read",".git/modules"="read",".codex"="read",".agents"="read"}},{}="write""#,
                        quote(&root)
                    );
                    // A linked worktree keeps its git dir in the common dir, outside the workspace.
                    if let Some((git_dir, common)) = outside_git_dirs(cwd) {
                        filesystem += &format!(
                            r#",{}="write",{}="read",{}="read",{}="read""#,
                            quote(&common),
                            quote(&format!("{common}/hooks")),
                            quote(&format!("{common}/config")),
                            quote(&format!("{common}/modules"))
                        );
                        if git_dir != common {
                            filesystem += &format!(
                                r#",{}="write",{}="read""#,
                                quote(&git_dir),
                                quote(&format!("{git_dir}/config.worktree"))
                            );
                        }
                    }
                    // A seat's reply rings its chair with `tmux -L swarm` on a tmux host, and the
                    // sandbox refuses every AF_UNIX socket that it does not name.
                    // SAFETY: getuid has no preconditions and cannot fail.
                    let uid = unsafe { crate::hud::getuid() };
                    let tmux = tmux_socket_dir(std::env::var("TMUX_TMPDIR").ok().as_deref(), uid);
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
                            r#"permissions.swarm-code={{extends=":workspace",filesystem={{{filesystem}}},network={{enabled=true,unix_sockets={{{}="allow"}}{domains}}}}}"#,
                            quote(&tmux)
                        ),
                    ]);
                }
                Some(sandbox) => args.extend(["--sandbox".into(), sandbox.into()]),
                None => {}
            }
            if let Some(approval) = &resolved.approval {
                args.extend(["--ask-for-approval".into(), approval.clone()]);
            }
            // `swarm setup hooks` trusts these for the owner (`codex_hook_trust`); the trust holds
            // only while each value stays byte-identical, so nothing per launch goes in it. The
            // pane env names the agent. The empty first group puts swarm's hook in group 1, so
            // its trust key differs from another tool's `-c` hook in group 0.
            let command = serde_json::to_string(&shared_hook_command(provider.id()))
                .expect("string serialization cannot fail");
            for event in CODEX_STATE_EVENTS {
                args.extend([
                    "-c".into(),
                    format!(
                        "hooks.{event}=[{{hooks=[]}},{{hooks=[{{type=\"command\",command={command},timeout={STATE_HOOK_TIMEOUT}}}]}}]"
                    ),
                ]);
            }
            Ok(args)
        }
        Provider::Agy => {
            let mut args = vec![provider.id().into()];
            if let Some(model) = resolved
                .model
                .as_deref()
                .filter(|model| *model != "default")
            {
                args.extend(["--model".into(), model.into()]);
            }
            let effort = effort()?;
            if agy_takes_effort(resolved.model.as_deref()) {
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
            // pi sends OSC 7501 only to a terminal that answers its probe. `swarm pty` answers it
            // in a tmux-swarm pane (ADR 0123), and the variable makes pi send on any other host.
            let mut args = vec![
                "env".into(),
                "PI_PROGRAM_STATUS=1".into(),
                provider.id().into(),
            ];
            if let Some(model) = resolved
                .model
                .as_deref()
                .filter(|model| *model != "default")
            {
                args.extend(["--model".into(), model.into()]);
            }
            args.extend(["--thinking".into(), effort()?.into()]);
            Ok(args)
        }
    }
}

/// Whether AGY takes `--effort` for `model`. An id such as `gemini-3.8-flash-high` fixes its own
/// effort, and AGY refuses `--effort` for it and for a Claude model such as `claude-sonnet-4-6`
/// (probe 2026-10-21).
// ponytail: a name rule from `agy models`; read AGY's own per-model flag if it gets one.
pub fn agy_takes_effort(model: Option<&str>) -> bool {
    !model.is_some_and(|model| {
        model.starts_with("claude-")
            || ["-low", "-medium", "-high"]
                .iter()
                .any(|level| model.ends_with(level))
    })
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

/// Why a caller may not launch an agent, or None. A pane `swarm spawn` made carries its own
/// agent id, and only the orchestrator starts children; Herdr marks its own agent panes too.
pub fn launch_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane)
        .then_some("swarm: a child agent cannot launch agents; ask the orchestrator")
}

/// Why a caller may not revert the owner's managed changes, or None: the same child panes that
/// may not launch, because a revert removes the guard registration that blocks their own calls.
pub fn revert_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane).then_some(
        "swarm: a child agent cannot revert the owner's managed changes; ask the orchestrator",
    )
}

/// Why a caller may not apply `swarm setup`, or None: the same child panes, because an apply
/// writes the owner's launch consent, so a seat left on `ask` could give itself standing consent.
pub fn setup_refusal(
    swarm_agent: Option<&str>,
    herdr_agent_pane: Option<&str>,
) -> Option<&'static str> {
    child_agent(swarm_agent, herdr_agent_pane)
        .then_some("swarm: a child agent cannot apply swarm setup; ask the orchestrator")
}

/// Why `swarm sweep --every` may not run on `adapter`, or None. The app's listing already rings an
/// idle chair again, and a chair whose CLI has no background task (pi) waits on the loop, so its
/// CLI queues every ring until someone stops the command.
pub fn sweep_refusal(adapter: &str, every: bool) -> Option<&'static str> {
    (every && adapter == "tmux-swarm").then_some(
        "swarm: the app rings an idle chair again, and a chair that waits on `swarm sweep --every` \
         gets no ring until the loop ends; end the turn instead",
    )
}

fn child_agent(swarm_agent: Option<&str>, herdr_agent_pane: Option<&str>) -> bool {
    swarm_agent.is_some_and(|agent| agent != "orchestrator") || herdr_agent_pane == Some("1")
}

/// Fable runs as a child only for review and council seats. The router refuses it too; this is
/// the second, independent check, so a config edited past the router still cannot reach a pane.
pub fn fable_refusal(agent_id: &str, role: &str, model: Option<&str>) -> Option<String> {
    let fable = model.is_some_and(|model| model.to_ascii_lowercase().contains("fable"));
    (fable
        && agent_id != "orchestrator"
        && !role.starts_with("review.")
        && !role.starts_with("council."))
    .then(|| format!("swarm: Fable is a child only for review.* and council.* (role {role})"))
}

/// A model absent from a readable list, with the account and model choices in list order.
pub fn model_refusal(
    provider: &str,
    model: &str,
    account: Option<&str>,
    other_accounts: &[String],
    models: &[String],
) -> String {
    let login = account.map_or_else(
        || "the CLI's own login".to_string(),
        |name| format!("account {name}"),
    );
    let mut message = format!(
        "swarm: model {model} is not in the {provider} model list for {login}\nfixes that might work:"
    );
    if !other_accounts.is_empty() {
        let accounts = other_accounts
            .iter()
            .map(|name| format!("--account {name}"))
            .collect::<Vec<_>>()
            .join(", ");
        message.push_str(&format!("\n  - another account: {accounts}"));
    }
    message.push_str(&format!("\n  - another model: {}", models.join(", ")));
    message
}

/// Why this session has no spawn path, from the stderr of `herdr status`, or None. A sandbox that
/// denies the Herdr socket denies every later `herdr` call too, and a wider sandbox would spend a
/// user approval on a path this session is not meant to use.
pub fn socket_refusal(herdr_status_stderr: &str) -> Option<&'static str> {
    herdr_status_stderr.contains("PermissionDenied").then_some(
        "swarm: this sandbox denies the Herdr socket, so this session has no spawn path; report that and stop, and do not request a wider sandbox",
    )
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

/// The caller's extra args for `provider`, checked and in the provider's shape.
pub fn extra_args(provider: Provider, extra: &[String]) -> Result<Vec<String>, String> {
    if let Some(flag) = find_flag(extra, provider.owned_flags()) {
        return Err(format!(
            "swarm: the role owns the {} flag {flag}; remove it from the extra args",
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

fn required<'a>(role: &str, field: &str, value: Option<&'a str>) -> Result<&'a str, String> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("swarm: role {role} has no {field}"))
}
