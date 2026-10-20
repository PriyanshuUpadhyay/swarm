//! CLI context.

use super::*;

pub(crate) fn init() -> Result<(), Box<dyn std::error::Error>> {
    let runs_dir = swarm::paths::runs_dir()?;

    std::fs::create_dir_all(runs_dir)?;
    let adapters = swarm::paths::root_dir()?.join("adapters");
    std::fs::create_dir_all(&adapters)?;
    // Nothing shipped is written here any more. The binary carries the adapters
    // (`swarm::adapter::SHIPPED`) and a file on disk states an edit, so a copy that says what the
    // binary already says is a copy that can only go stale. See `swarm::adapter::load`.
    for (name, text) in swarm::adapter::SHIPPED {
        let file = adapters.join(format!("{name}.conf"));
        if std::fs::read_to_string(&file).is_ok_and(|deployed| deployed == text) {
            std::fs::remove_file(&file)?;
            println!("swarm: removed adapters/{name}.conf, which said what this binary says");
        }
    }
    swarm::store::open(&swarm::paths::sqlite_db()?)?;
    // Setting up the hooks changes the owner's Codex and AGY config, so it waits for consent.
    println!(
        "swarm: to see Codex and AGY agents' chats and questions, run `swarm setup hooks`; it trusts swarm's own Codex hooks and adds swarm's AGY hooks"
    );

    Ok(())
}

pub(crate) const USAGE: &str = "usage: swarm --version | init | doctor [--json] | setup [--json] | setup status --json | setup <all|hooks,trust,plugin,hud> [--plan [--json] | --digest <digest> | --yes] [--cwd <dir>] [--consent <standing|ask>] [--resume] | hooks status --json | managed list [--json] | managed revert (<id>... | --all) [--plan [--json] | --digest <digest>] | adapter check <name> | session new <talk_mode> [--chair <claude|codex|pi>:<id>] (cwd: pwd -P) | session chair <claude|codex|pi>:<id> | session continue <new_id> <old_id> | session archive <id>... | session unarchive <id>... | sessions --json [--archived] | agent add <agent_id> <role> | serve [--port N] [--assets DIR] | ui [stop | --foreground] [--port N] [--assets DIR] | herdr-split | notify <title> [--body <text>] | hud <uninstall|status> [--json] | notices --json | notices --due | host-context --provider <claude|codex|agy> | hook <claude|codex|agy> [event] | status-feed | guard <claude|codex|agy> PreToolUse | roles --json | roles get <role> [--provider <claude|codex|agy|pi>] | roles check --json | roles save --revision <revision> <profile-json> | roles new <name> --revision <revision> | roles rename <old> <new> --revision <revision> | roles copy <from> <to> --revision <revision> | roles delete <name> --revision <revision> | roles reset --revision <revision> | roles set-min-usage <pct> --revision <revision> | providers --json | models --provider <claude|codex|agy|pi> --json | accounts --provider <claude|codex|agy|pi> --json | accounts login --provider <claude|codex> --name <name> --revision <revision> --json [--cwd <dir>] | accounts reset --revision <revision> --json | accounts pick --provider <claude|codex> [--account <auto|menu|NAME> | --session <id> | --last [--all]] [--model <model>] --json | usage --json | usage --format openusage | working --json | usage --refresh --provider <claude|codex> --json | agents --json [--all] | messages --json [--after <seq>] | run <claude|codex> [--account [auto|menu|NAME]] [-- <args>...] | launch <agent_id> <role> [--provider <claude|codex|agy|pi>] [--model <model> for chat] [--account <auto|name>] [--cwd <dir>] [-- <provider args>...] | spawn <agent_id> <role> [--provider <p>] [--account <auto|name>] [-- <cmd>...] | type <agent_id> | answer <agent_id> <prompt_id> <choice> | interrupt <agent_id> | key <agent_id> <Up|C-u> | attach <agent_id> | focus <session_id> <agent_id> | close <agent_id> | send <recipient> <kind> | commands <agent_id> --json | finish | exited | sweep [--every <secs>] | drain | inbox | ack <seq>";

pub(crate) fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("swarm: {name} not set"))
}

pub(crate) fn session_id() -> Result<String, String> {
    match env::var("SWARM_SESSION_ID") {
        Ok(session) => valid_session_id(&session),
        Err(_) => Ok(identity()?.0),
    }
}

pub(crate) fn valid_session_id(id: &str) -> Result<String, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "swarm: bad SWARM_SESSION_ID".to_string())?;
    if id.get_version_num() != 7 {
        return Err("swarm: bad SWARM_SESSION_ID".to_string());
    }
    Ok(id.to_string())
}

/// Every agent pane has one of these set. It is a guard against a model's mistake, not a trust
/// boundary.
pub(crate) fn in_agent_pane() -> bool {
    [
        "TMUX_PANE",
        "HERDR_PANE_ID",
        "SUPERSET_TERMINAL_ID",
        "REX_BLOCK",
    ]
    .iter()
    .any(|name| env::var_os(name).is_some())
}

pub(crate) fn adapter_name() -> String {
    swarm::host::adapter(|name| env::var(name).ok())
}

/// The host contract that `swarm host-context` would print in a pane that `launch` is about to
/// open, for a CLI with no SessionStart hook. The pane gets these ids, and a tmux-swarm pane
/// always has `TMUX_PANE`.
pub(crate) fn pane_context(
    provider: &str,
    session: &str,
    agent_id: &str,
    role: &str,
) -> Option<String> {
    let adapter = adapter_name();
    swarm::host::context(provider, |name| match name {
        "SWARM_SESSION_ID" => Some(session.into()),
        "SWARM_AGENT_ID" => Some(agent_id.into()),
        "SWARM_ROLE" => Some(role.into()),
        "SWARM_ADAPTER" => Some(adapter.clone()),
        "TMUX_PANE" if adapter == "tmux-swarm" => Some("pane".into()),
        _ => env::var(name).ok(),
    })
}

/// The stored role of a seat, for its start context. Best effort: the context still prints
/// without it.
pub(crate) fn seat_role(session: &str, agent_id: &str) -> Option<String> {
    // Read only, with a column every schema has: a start hook from another build must not create,
    // migrate, or refuse the database that the launching build owns.
    let db = swarm::paths::sqlite_db_to_read()
        .ok()
        .filter(|db| db.is_file())?;
    let connection =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    connection
        .query_row(
            "SELECT role FROM agent WHERE session_id = ?1 AND id = ?2",
            (session, agent_id),
            |row| row.get(0),
        )
        .ok()
}

/// A pane that `swarm launch` made carries its ids in its env. A chair keeps none: `swarm session
/// new` records the chair's pane, and the chair is found from that pane.
pub(crate) fn identity() -> Result<(String, String), String> {
    if let Ok(session) = env::var("SWARM_SESSION_ID") {
        return Ok((valid_session_id(&session)?, env_var("SWARM_AGENT_ID")?));
    }
    let found = own_pane().and_then(|pane| {
        let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
        swarm::store::orchestrator_at_pane(&connection, &pane)
    });
    match found {
        Ok(Some(identity)) => Ok(identity),
        _ => Err("swarm: this pane has no swarm session; run `swarm session new lane`".into()),
    }
}

/// The pane this process runs in, as the adapter's `self` verb reports it.
pub(crate) fn own_pane() -> Result<String, Box<dyn std::error::Error>> {
    let pane =
        swarm::adapter::load(&swarm::paths::root_dir()?, &adapter_name())?.run("self", &[])?;
    match pane.trim().is_empty() {
        true => Err("swarm: the adapter gave no pane".into()),
        false => Ok(pane),
    }
}

pub(crate) fn run_tool(
    executable: &str,
    args: &[&str],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    std::process::Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| format!("swarm: cannot run {executable}: {error}").into())
}

pub(crate) fn tool_stdout(
    executable: &str,
    args: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    checked_stdout(executable, run_tool(executable, args)?)
}

pub(crate) fn checked_stdout(
    executable: &str,
    output: std::process::Output,
) -> Result<String, Box<dyn std::error::Error>> {
    if !output.status.success() {
        return Err(format!(
            "swarm: {executable} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("swarm: {executable} printed non-UTF-8 output: {error}").into())
}

pub(crate) fn print_json(value: &impl serde::Serialize) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

/// A printed format with a version (`docs/usage-format.md`): `schema` comes first, then the body.
#[derive(serde::Serialize)]
pub(crate) struct Versioned<'a, T> {
    pub(crate) schema: &'static str,
    #[serde(flatten)]
    pub(crate) body: &'a T,
}

pub(crate) fn valid_chair_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub(crate) fn parse_chair(value: &str) -> Result<Option<(&str, &str)>, String> {
    let (provider, id) = value.split_once(':').ok_or_else(|| USAGE.to_string())?;
    if !matches!(provider, "claude" | "codex" | "pi") {
        return Err(USAGE.to_string());
    }
    Ok(valid_chair_id(id).then_some((provider, id)))
}

pub(crate) fn env_chair() -> Option<(String, String)> {
    for (provider, variable) in [
        ("claude", "CLAUDE_CODE_SESSION_ID"),
        ("codex", "CODEX_THREAD_ID"),
        ("pi", "PI_SESSION_ID"),
    ] {
        if let Ok(id) = env::var(variable) {
            return valid_chair_id(&id).then(|| (provider.to_string(), id));
        }
    }
    None
}

pub(crate) fn default_codex_home_with_env(
    mut env_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> Result<std::path::PathBuf, String> {
    if let Some(home) = env_var("CODEX_HOME") {
        return Ok(home.into());
    }
    env_var("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".codex"))
        .ok_or_else(|| "swarm: HOME not set".to_string())
}

pub(crate) fn default_codex_home() -> Result<std::path::PathBuf, String> {
    default_codex_home_with_env(|variable| std::env::var_os(variable))
}

/// The native Codex home, each discovered `~/.codex-<name>` home, and each home that
/// `swarm accounts login` made under `accounts/codex/` in swarm's folder.
pub(crate) fn codex_homes(
    user_home: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let mut homes = vec![default_codex_home()?];
    let swarm_homes = swarm::paths::codex_accounts_dir()?;
    for (folder, prefix) in [(user_home, ".codex-"), (swarm_homes.as_path(), "")] {
        if let Ok(entries) = std::fs::read_dir(folder) {
            homes.extend(
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.file_name().to_string_lossy().starts_with(prefix)
                            && entry.path().is_dir()
                    })
                    .map(|entry| entry.path()),
            );
        }
    }
    homes.sort();
    homes.dedup();
    Ok(homes)
}

pub(crate) fn attach(
    agent_id: &str,
) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let pane = swarm::store::require_pane(&connection, &session_id()?, agent_id)?;
    swarm::adapter::load(&root, &adapter_name())?.attach(&[("pane", &pane)])
}

pub(crate) fn focus(session_id: &str, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = swarm::paths::root_dir()?;
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let adapter = swarm::store::adapter_of(&connection, session_id)?
        .ok_or("swarm: session has no adapter")?;
    let adapter = swarm::adapter::load(&root, &adapter)?;
    // Unsupported hosts report their capability, rather than pretending the pane is closed.
    if adapter.focus.is_none() {
        return Err(format!("swarm: {} has no focus", adapter.name).into());
    }
    let try_pane = |agent| -> Result<(), Box<dyn std::error::Error>> {
        let pane =
            swarm::store::pane_of(&connection, session_id, agent)?.ok_or("swarm: pane closed")?;
        adapter.run("focus", &[("pane", &pane)])?;
        Ok(())
    };
    if try_pane(agent_id).is_err() {
        let chair = swarm::store::orchestrator_of(&connection, session_id)
            .map_err(|_| "swarm: pane closed")?;
        try_pane(&chair).map_err(|_| "swarm: pane closed")?;
    }
    activate_client(&adapter);
    Ok(())
}

pub(crate) fn activate_client(adapter: &swarm::adapter::Adapter) {
    if adapter.client.is_none() {
        return;
    }
    let Ok(client) = adapter.run("client", &[]) else {
        return;
    };
    let Ok(pid) = client.trim().parse::<u32>() else {
        return;
    };
    if let Some(app) = client_app(pid) {
        // ponytail: with several Ghostty windows the last-focused one rises; a per-window pick
        // needs AX. Add it if this bites.
        let _ = std::process::Command::new("open").arg(app).status();
    }
}

pub(crate) fn client_app(mut pid: u32) -> Option<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    while pid > 1 && seen.insert(pid) {
        let command = std::process::Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !command.status.success() {
            return None;
        }
        let command = String::from_utf8(command.stdout).ok()?;
        let path = command.trim();
        if let Some(end) = path.find(".app/Contents/MacOS/") {
            return Some(std::path::PathBuf::from(&path[..end + 4]));
        }
        let parent = std::process::Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !parent.status.success() {
            return None;
        }
        pid = String::from_utf8(parent.stdout).ok()?.trim().parse().ok()?;
    }
    None
}

pub(crate) const ACCOUNT_PICK_TIMEOUT_SECONDS: u64 = 2;
/// A usage refresh before an automatic pick: one CLI start and one quota read per account.
pub(crate) const USAGE_PICK_REFRESH_TIMEOUT_SECONDS: u64 = 8;
// Leave time for CLI output before SwarmCLIProfileSource kills its process at 20 seconds.
pub(crate) const NATIVE_READ_TIMEOUT_SECONDS: u64 = 18;
pub(crate) const LOGIN_ADAPTER_TIMEOUT_SECONDS: u64 = 18;
pub(crate) const LOGIN_PANE_CLOSE_TIMEOUT_SECONDS: u64 = 1;

/// `swarm guard <provider> <event>`: the rule list's verdict on one tool call, in the CLI's own
/// reply format (ADR 0040). It never opens the store, so a busy bus cannot slow a tool call.
pub(crate) fn guard(args: &[String]) -> (String, String, i32) {
    let [provider, event] = args else {
        return (String::new(), format!("{USAGE}\n"), 2);
    };
    let Some(provider) = Provider::parse(provider) else {
        return (String::new(), format!("{USAGE}\n"), 2);
    };
    let deadline = std::time::Instant::now() + swarm::guard::DEADLINE;
    let verdict = match (swarm::paths::guards_file(), env::var("HOME")) {
        (Ok(path), Ok(home)) => {
            let path = path.to_string_lossy();
            let payload = read_within(std::io::stdin(), std::time::Duration::from_secs(2));
            match (std::fs::read_to_string(path.as_ref()), payload) {
                (_, None) => swarm::guard::Verdict::Deny(
                    "swarm guard: cannot read the hook payload on stdin within 2 s, so the call is blocked".into(),
                ),
                (Err(error), _) if error.kind() != std::io::ErrorKind::NotFound => {
                    swarm::guard::Verdict::Deny(format!(
                        "swarm guard: cannot read {path}: {error}, so the call is blocked"
                    ))
                }
                (list, Some(payload)) => swarm::guard::decide(
                    list.ok().as_deref(),
                    &path,
                    provider,
                    event,
                    &payload,
                    &home,
                    deadline,
                ),
            }
        }
        _ => swarm::guard::Verdict::Deny(
            "swarm guard: HOME is not set, so the call is blocked".into(),
        ),
    };
    swarm::guard::render(provider, &verdict)
}

#[cfg(test)]
#[path = "context/tests.rs"]
mod tests;
