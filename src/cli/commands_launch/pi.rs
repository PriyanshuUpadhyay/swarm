//! Pi's launch context and explicitly loaded extensions.

use super::{adapter_name, pane_context, pi_packages, pi_version};
use std::path::Path;

pub(super) fn launch_args(
    root: &Path,
    pi_dir: &Path,
    session: &str,
    agent_id: &str,
    id: &str,
    role: &str,
    picked_codex_account: bool,
) -> std::io::Result<Vec<String>> {
    let (state, reason) = pi_version::state();
    if state != "ok" {
        eprintln!("swarm: warning: {reason}");
    }
    let mut args = vec!["--session-id".to_string(), id.to_owned()];
    if let Some(context) = pane_context("pi", session, agent_id, role) {
        args.extend(["--append-system-prompt".into(), context]);
    }
    args.extend(extension_args(root, pi_dir, picked_codex_account, true)?);

    // A file of an earlier run must not show as this run's text or activity (ADR 0131).
    let live = swarm::bus::pi_live_file(root, session, agent_id);
    if let Some(dir) = live.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(&live);
    let _ = std::fs::remove_file(swarm::bus::pi_children_file(&live));
    args.extend(["--swarm-live".into(), live.display().to_string()]);
    Ok(args)
}

fn extension_args(
    root: &Path,
    pi_dir: &Path,
    picked_codex_account: bool,
    pane: bool,
) -> std::io::Result<Vec<String>> {
    let mut args = vec!["--no-extensions".into()];
    for name in ["builtin:mcp", "builtin:codemode"] {
        args.extend(["--extension".into(), name.into()]);
    }
    // Load the installed bridge entry point, not an npm spec that Pi may install again.
    let installed = [
        Some(pi_dir.join("npm/node_modules/pi-claude-bridge/src/index.ts")),
        Some(pi_dir.join("extensions/command-guard.ts")),
        picked_codex_account.then(|| pi_dir.join("extensions/codex-profile.ts")),
        (pane && adapter_name() == "herdr").then(|| pi_dir.join("extensions/herdr-agent-state.ts")),
    ];
    for path in installed
        .into_iter()
        .flatten()
        .filter(|path| path.is_file())
    {
        args.extend(["--extension".into(), path.display().to_string()]);
    }

    if let Some(packages) = pi_packages::launch_args(&pi_packages::prefix(root)) {
        args.extend(packages);
    } else {
        eprintln!(
            "swarm: the pinned Pi package is not installed; run `swarm setup pi` to enable it"
        );
    }

    let extension = swarm::bus::write_pi_extension(root)?;
    args.extend([
        "--extension".into(),
        extension.display().to_string(),
        "--swarm-bin".into(),
        std::env::current_exe()?.display().to_string(),
    ]);
    Ok(args)
}

/// The native tool sends only a resolved provider/id and a log slot, never arbitrary Pi flags.
pub(crate) fn child_argv(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs};
    let [provider, model, parent, call, slot] = args else {
        return Err("swarm: invalid Pi child launch request".into());
    };
    if let Some(reason) = swarm::guard::native_subagent_model_refusal(model) {
        return Err(reason.into());
    }
    let valid_model_part = |part: &str| {
        !part.is_empty()
            && !part.starts_with('-')
            && part.len() <= 256
            && part.bytes().all(|b| b.is_ascii_graphic())
    };
    if !valid_model_part(provider) || !valid_model_part(model) {
        return Err("swarm: expected an exact Pi provider and model id".into());
    }
    if env::var("SWARM_AGENT_ID").as_deref() != Ok("orchestrator")
        || env::var("HERDR_AGENT_PANE").as_deref() == Ok("1")
    {
        return Err("swarm: only a Pi chair can start a native Pi child".into());
    }
    let root = swarm::paths::root_dir()?;
    let home = std::path::PathBuf::from(env::var_os("HOME").ok_or("swarm: HOME is not set")?);
    let pi_dir = env::current_dir()?.join(swarm::providers::pi_agent_dir(&home));
    let mut argv = extension_args(
        &root,
        &pi_dir,
        env::var_os("PI_CODEX_HOME").is_some(),
        false,
    )?;
    let parent = fs::canonicalize(parent)?;
    if !parent.is_file() || !parent.extension().is_some_and(|ext| ext == "jsonl") {
        return Err("swarm: the Pi parent has no saved session".into());
    }
    let dir =
        swarm::bus::pi_child_dir(&parent, call, slot).ok_or("swarm: invalid Pi child log slot")?;
    let parent_dir = dir.parent().ok_or("swarm: missing Pi child log parent")?;
    // Refuse a pre-existing symlink in any component, or an existing slot (never resume a child).
    for folder in [
        dir.ancestors().nth(3).unwrap(),
        parent_dir.parent().unwrap(),
        parent_dir,
    ] {
        match fs::create_dir(folder) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            other => other?,
        }
        if folder.canonicalize()? != folder || !folder.is_dir() {
            return Err("swarm: Pi child log folder leaves the parent session".into());
        }
    }
    fs::create_dir(&dir)?;
    argv.extend([
        "--mode".into(),
        "rpc".into(),
        "--provider".into(),
        provider.clone(),
        "--model".into(),
        model.clone(),
        "--session-dir".into(),
        dir.to_str().ok_or("swarm: invalid log path")?.into(),
        "--append-system-prompt".into(),
        "[agent-host: swarm — native Pi child]\n[agent-runtime: pi]\nYou are a leaf worker. Do only the delegated task. Do not start agents or panes, use Swarm messaging, or contact the user. Return your result here as your final response.".into(),
    ]);
    println!("{}", serde_json::to_string(&argv)?);
    Ok(())
}
