use super::{Account, AccountList, AuthState};
use crate::providers::Provider;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum NativeReadError {
    CliUnavailable,
    Failed(String),
}

impl std::fmt::Display for NativeReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CliUnavailable => formatter.write_str("provider CLI is unavailable"),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl From<&str> for NativeReadError {
    fn from(message: &str) -> Self {
        Self::Failed(message.into())
    }
}

impl From<String> for NativeReadError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<NativeReadError> for String {
    fn from(error: NativeReadError) -> Self {
        error.to_string()
    }
}

fn resolved(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// A Codex session that a pick follows: one id, or the newest session started in a folder (None:
/// in any folder).
pub enum CodexSession {
    Id(String),
    Last(Option<PathBuf>),
}

/// The account whose home holds the session. Codex keeps each session inside one home, so
/// `resume`, `fork` and the other session commands work only with that account. A rollout is
/// `rollout-<YYYY-MM-DDThh-mm-ss>-<uuid>.jsonl` under `sessions/<y>/<m>/<d>/`; an id also matches
/// one that Codex moved to `archived_sessions/`.
pub fn codex_session_owner<'a>(
    accounts: &'a [Account],
    session: &CodexSession,
) -> Option<&'a Account> {
    let entries = |dir: PathBuf| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect()
    };
    let mut rollouts = Vec::new();
    for account in accounts {
        let home = Path::new(&account.home);
        let mut files: Vec<_> = entries(home.join("sessions"))
            .into_iter()
            .flat_map(entries)
            .flat_map(entries)
            .flat_map(entries)
            .collect();
        if matches!(session, CodexSession::Id(_)) {
            files.extend(entries(home.join("archived_sessions")));
        }
        rollouts.extend(files.into_iter().filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let rest = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
            let (started, id) = rest.split_at_checked(19)?;
            let id = id.strip_prefix('-').filter(|id| id.len() == 36)?;
            uuid::Uuid::parse_str(id).ok()?;
            Some((started.to_string(), id.to_ascii_lowercase(), path, account))
        }));
    }
    match session {
        CodexSession::Id(wanted) => rollouts
            .into_iter()
            .find(|(_, id, _, _)| *id == wanted.to_ascii_lowercase())
            .map(|(.., account)| account),
        CodexSession::Last(cwd) => {
            rollouts.sort_by(|left, right| right.0.cmp(&left.0));
            let cwd = cwd.clone().map(resolved);
            rollouts
                .into_iter()
                .find(|(_, _, path, _)| {
                    cwd.as_ref().is_none_or(|cwd| {
                        rollout_cwd(path).is_some_and(|dir| resolved(dir) == *cwd)
                    })
                })
                .map(|(.., account)| account)
        }
    }
}

/// The folder a rollout's session started in, from its first line, the `session_meta` record.
fn rollout_cwd(path: &Path) -> Option<PathBuf> {
    let mut line = String::new();
    BufReader::new(std::fs::File::open(path).ok()?)
        .read_line(&mut line)
        .ok()?;
    let meta: Value = serde_json::from_str(&line).ok()?;
    (meta["type"] == "session_meta")
        .then(|| meta["payload"]["cwd"].as_str().map(PathBuf::from))
        .flatten()
}

pub struct Discovery {
    pub homes: Vec<(String, PathBuf)>,
    pub current: PathBuf,
    pub skipped: Vec<(String, PathBuf)>,
}

pub fn discover(provider: Provider, home: &Path, active: Option<PathBuf>) -> Discovery {
    let default = home.join(match provider {
        Provider::Codex => ".codex",
        _ => ".claude",
    });
    let current = resolved(active.clone().unwrap_or_else(|| default.clone()));
    let mut candidates = vec![("default".to_string(), default)];
    let mut skipped = Vec::new();
    let directory = match provider {
        Provider::Codex => home.to_path_buf(),
        _ => home.join(".claude/.profiles"),
    };
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let name = match provider {
                Provider::Codex => name.strip_prefix(".codex-").map(str::to_string),
                _ => (!name.starts_with('.')).then_some(name),
            };
            if let Some(name) = name {
                if crate::config::valid_account_name(&name) {
                    candidates.push((name, entry.path()));
                } else {
                    skipped.push((name, entry.path()));
                }
            }
        }
    }
    candidates.sort_by_key(|(name, path)| {
        (
            name != "default",
            resolved(path.clone()) != *path,
            name.clone(),
        )
    });
    // A named native home keeps its name when it is also the active home.
    if let Some(active) = active
        && !candidates
            .iter()
            .chain(&skipped)
            .any(|(_, path)| resolved(path.clone()) == current)
    {
        candidates.push(("current".into(), active));
    }
    let mut seen = BTreeSet::new();
    let homes = candidates
        .into_iter()
        .filter_map(|(name, path)| {
            let path = resolved(path);
            seen.insert(path.clone()).then_some((name, path))
        })
        .collect();
    Discovery {
        homes,
        current,
        skipped,
    }
}

pub fn load(provider: Provider, deadline: Instant) -> Result<AccountList, String> {
    load_inner(provider, deadline, true)
}

pub(crate) fn identities(provider: Provider, deadline: Instant) -> Result<AccountList, String> {
    load_inner(provider, deadline, false)
}

fn load_inner(
    provider: Provider,
    deadline: Instant,
    include_usage: bool,
) -> Result<AccountList, String> {
    let metadata = crate::accounts::load()?;
    let mut list = super::empty_accounts(provider.id());
    list.revision = metadata.revision;
    list.modified = !metadata.accounts.is_empty();
    if !provider.has_accounts() {
        return Ok(list);
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    let active = match provider {
        Provider::Codex => std::env::var_os("CODEX_HOME"),
        _ => std::env::var_os("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR")),
    }
    .filter(|value| !value.is_empty())
    .map(PathBuf::from);
    // Secure-storage paths can differ from legacy config homes; retain the known account name.
    let active = if provider == Provider::Claude && active.is_some() {
        let known = crate::accounts::merge(
            provider,
            discover(provider, &home, None).homes,
            &metadata.accounts,
        );
        active.map(|active| {
            known
                .into_iter()
                .find_map(|(name, path)| {
                    crate::providers::secure_storage_dir(&path.to_string_lossy(), &name)
                        .filter(|storage| {
                            resolved(PathBuf::from(storage)) == resolved(active.clone())
                        })
                        .map(|_| path)
                })
                .unwrap_or(active)
        })
    } else {
        active
    };
    let discovery = discover(provider, &home, active);
    let homes = crate::accounts::merge(provider, discovery.homes, &metadata.accounts);
    list.source = Some("swarm".into());
    list.state = "ready".into();
    // Each read starts the CLI, which takes about a second, so the homes are read at once: in a row,
    // every home after the first few would pass the shared deadline.
    let homes: Vec<_> = homes
        .into_iter()
        .map(|(name, path)| (name, path.to_string_lossy().into_owned()))
        .collect();
    let identities: Vec<_> = std::thread::scope(|scope| {
        let reads: Vec<_> = homes
            .iter()
            .map(|(name, path)| {
                let identity_env = provider.identity_env(name, path);
                scope.spawn(move || match provider {
                    Provider::Codex => AppServer::start(&identity_env, deadline)
                        .and_then(|mut server| {
                            server.request(2, "account/read", json!({"refreshToken":false}))
                        })
                        .and_then(|result| {
                            let api_key = result["account"]["type"] == "apiKey";
                            codex_identity(result)
                                .map(|(state, email)| (state, email, api_key))
                                .map_err(NativeReadError::from)
                        }),
                    _ => claude_identity(&identity_env, deadline)
                        .map(|(state, email)| (state, email, false)),
                })
            })
            .collect();
        reads
            .into_iter()
            .map(|read| read.join().expect("an account read panicked"))
            .collect()
    });
    for ((name, path), identity) in homes.into_iter().zip(identities) {
        let env = provider.launch_env(&name, &path);
        if matches!(identity, Err(NativeReadError::CliUnavailable)) {
            list.state = "unavailable".into();
        }
        let (auth_state, email, api_key) =
            identity.unwrap_or((AuthState::Unavailable, None, false));
        list.accounts.push(Account {
            name,
            email,
            home: path,
            env,
            auth_state,
            remaining_pct: None,
            usage_state: if api_key { "no_source" } else { "missing" }.into(),
            usage_source: (!api_key).then_some(
                match provider {
                    Provider::Codex => "codex_app_server",
                    _ => "claude_api",
                }
                .into(),
            ),
            summary: None,
            invalid_home: false,
            fable_spent: false,
            renewable: false,
        });
    }
    if include_usage {
        let meters = crate::usage::for_accounts(&list, crate::usage::now_seconds())?;
        super::apply_usage(&mut list, &meters);
    }
    for (name, path) in discovery.skipped {
        let canonical = resolved(path.clone());
        if list
            .accounts
            .iter()
            .any(|account| Path::new(&account.home) == canonical)
        {
            continue;
        }
        let path = path.to_string_lossy().into_owned();
        let display_path = path.escape_debug();
        let reason = format!(
            "Skipped native home {display_path}: invalid account name; use 1-{} lowercase ASCII letters, digits, '.', '_' or '-'; no '..' or reserved auto, default, current, '.', '..' names",
            crate::config::MAX_ACCOUNT_NAME_LENGTH
        );
        eprintln!("swarm: {reason}");
        // Empty and reserved labels use the path to avoid missing or duplicate row names.
        let name = if name.is_empty() || matches!(name.as_str(), "auto" | "default" | "current") {
            path.clone()
        } else {
            name
        };
        list.accounts.push(Account {
            name,
            home: path,
            env: BTreeMap::new(),
            email: None,
            auth_state: AuthState::Unavailable,
            remaining_pct: None,
            usage_state: "no_source".into(),
            usage_source: None,
            summary: Some(reason),
            invalid_home: true,
            fable_spent: false,
            renewable: false,
        });
    }
    list.accounts
        .sort_by(|left, right| left.name.cmp(&right.name));
    list.current_home = discovery.current.to_str().map(str::to_string);
    // Only a caller of `load` uses `auto`. A usage read must not read the roles config, which can
    // write its one-time import and warn on each read.
    if include_usage {
        list.auto = super::pick_auto(
            &list.accounts,
            list.current_home.as_deref(),
            None,
            crate::config::min_usage_left_pct(),
        )?;
    }
    Ok(list)
}

pub fn codex_identity(result: Value) -> Result<(AuthState, Option<String>), String> {
    match &result["account"] {
        Value::Null if result.get("account").is_some() => Ok((AuthState::SignedOut, None)),
        Value::Object(account)
            if matches!(
                account.get("type").and_then(Value::as_str),
                Some("chatgpt" | "apiKey")
            ) =>
        {
            Ok((
                AuthState::SignedIn,
                account
                    .get("email")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ))
        }
        _ => Err("Codex identity is unavailable".into()),
    }
}

fn claude_identity(
    env: &BTreeMap<String, String>,
    deadline: Instant,
) -> Result<(AuthState, Option<String>), NativeReadError> {
    let executable = std::env::var("SWARM_CLAUDE_CMD").unwrap_or_else(|_| "claude".into());
    let value = read_json(&executable, &["auth", "status"], env, deadline)?;
    match value["loggedIn"].as_bool() {
        Some(logged_in) => Ok((
            if logged_in {
                AuthState::SignedIn
            } else {
                AuthState::SignedOut
            },
            value["email"].as_str().map(str::to_string),
        )),
        None => Err("Claude identity is unavailable".into()),
    }
}

pub fn read_json(
    executable: &str,
    args: &[&str],
    env: &BTreeMap<String, String>,
    deadline: Instant,
) -> Result<Value, NativeReadError> {
    if Instant::now() >= deadline {
        return Err("provider read timed out".into());
    }
    let mut command = Command::new(executable);
    let claude_auth = args == ["auth", "status"];
    if claude_auth {
        // Native status must not inherit a different profile from the caller shell.
        for key in Provider::Claude.account_env_keys() {
            command.env_remove(key);
        }
    }
    let child = command
        .args(args)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| NativeReadError::CliUnavailable)?;
    let (response, status) = read_stdout(child, u64::MAX, deadline);
    if response.is_err() || status.is_none() {
        return Err("provider read timed out".into());
    }
    if !status.is_some_and(|status| status.success() || (claude_auth && status.code() == Some(1))) {
        return Err("provider read is unavailable".into());
    }
    let bytes = response
        .map_err(|_| "provider read timed out")?
        .map_err(|_| "provider read failed")?;
    serde_json::from_slice(&bytes).map_err(|_| "provider response JSON is invalid".into())
}

/// Reads up to `limit` bytes of the child's stdout and waits for its exit, both before `deadline`.
/// The child is always killed and reaped. A status of `None` means it did not exit in time.
pub(crate) fn read_stdout(
    mut child: Child,
    limit: u64,
    deadline: Instant,
) -> (
    Result<std::io::Result<Vec<u8>>, RecvTimeoutError>,
    Option<ExitStatus>,
) {
    let (sender, receiver) = channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take(limit).read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        });
    } else {
        let _ = sender.send(Err(std::io::Error::other("child has no stdout pipe")));
    }
    let response = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let mut status = child.try_wait().ok().flatten();
    while response.is_ok() && status.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
        status = child.try_wait().ok().flatten();
    }
    let _ = child.kill();
    let _ = child.wait();
    (response, status)
}

pub struct AppServer {
    child: Child,
    responses: Receiver<Result<Value, String>>,
    deadline: Instant,
}

impl AppServer {
    pub fn start(
        env: &BTreeMap<String, String>,
        deadline: Instant,
    ) -> Result<Self, NativeReadError> {
        if Instant::now() >= deadline {
            return Err("Codex read timed out".into());
        }
        let executable = std::env::var("SWARM_CODEX_CMD").unwrap_or_else(|_| "codex".into());
        let mut child = Command::new(executable)
            .args(["app-server", "--listen", "stdio://"])
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| NativeReadError::CliUnavailable)?;
        let stdout = child.stdout.take().ok_or("Codex stdout is unavailable")?;
        let (sender, responses) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let value = line
                    .map_err(|_| "Codex response read failed".into())
                    .and_then(|line| {
                        serde_json::from_str(&line)
                            .map_err(|_| "Codex response JSON is invalid".into())
                    });
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            child,
            responses,
            deadline,
        };
        server.request(
            1,
            "initialize",
            json!({"clientInfo":{"name":"swarm","title":"Swarm","version":env!("CARGO_PKG_VERSION")}}),
        )?;
        server.send(json!({"method":"initialized","params":{}}))?;
        Ok(server)
    }

    fn send(&mut self, value: Value) -> Result<(), NativeReadError> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or("Codex stdin is unavailable")?;
        writeln!(stdin, "{value}")
            .and_then(|_| stdin.flush())
            .map_err(|_| "Codex request write failed".into())
    }

    pub fn request(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, NativeReadError> {
        if Instant::now() >= self.deadline {
            return Err("Codex read timed out".into());
        }
        self.send(json!({"id":id,"method":method,"params":params}))?;
        loop {
            if Instant::now() >= self.deadline {
                return Err("Codex read timed out".into());
            }
            let value = self
                .responses
                .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "Codex read timed out")??;
            if value["id"] != id {
                continue;
            }
            if value.get("error").is_some() {
                return Err("Codex request failed".into());
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| "Codex response has no result".into());
        }
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn deadline(seconds: u64) -> Instant {
    Instant::now() + Duration::from_secs(seconds)
}
