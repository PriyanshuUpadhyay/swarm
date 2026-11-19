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
    args.push("--no-extensions".into());
    for name in ["builtin:mcp", "builtin:codemode"] {
        args.extend(["--extension".into(), name.into()]);
    }
    // Load the installed bridge entry point, not an npm spec that Pi may install again.
    let installed = [
        Some(pi_dir.join("npm/node_modules/pi-claude-bridge/src/index.ts")),
        Some(pi_dir.join("extensions/command-guard.ts")),
        picked_codex_account.then(|| pi_dir.join("extensions/codex-profile.ts")),
        (adapter_name() == "herdr").then(|| pi_dir.join("extensions/herdr-agent-state.ts")),
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
        eprintln!("swarm: Pi packages are not installed; run `swarm setup pi` to enable them");
    }

    // The chat's live row reads the turn's text from this file, not from pi's screen
    // (ADR 0131). A file of an earlier run must not show as this run's text.
    let extension = swarm::bus::write_pi_extension(root)?;
    let live = swarm::bus::pi_live_file(root, session, agent_id);
    if let Some(dir) = live.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(&live);
    args.extend([
        "--extension".into(),
        extension.display().to_string(),
        "--swarm-live".into(),
        live.display().to_string(),
        "--swarm-bin".into(),
        std::env::current_exe()?.display().to_string(),
    ]);
    Ok(args)
}
