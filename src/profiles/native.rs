use super::{Account, AccountList, AuthState};
use crate::providers::Provider;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
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
    let variable = match provider {
        Provider::Codex => "CODEX_HOME",
        _ => "CLAUDE_CONFIG_DIR",
    };
    let active = std::env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let discovery = discover(provider, &home, active);
    let homes = crate::accounts::merge(provider, discovery.homes, &metadata.accounts);
    list.source = Some("swarm".into());
    list.state = "ready".into();
    for (name, path) in homes {
        let path = path.to_string_lossy().into_owned();
        let env = provider.account_env(&name, &path, |variable| std::env::var_os(variable));
        let identity = match provider {
            Provider::Codex => AppServer::start(&env, deadline)
                .and_then(|mut server| {
                    server.request(2, "account/read", json!({"refreshToken":false}))
                })
                .and_then(|result| {
                    let api_key = result["account"]["type"] == "apiKey";
                    codex_identity(result)
                        .map(|(state, email)| (state, email, api_key))
                        .map_err(NativeReadError::from)
                }),
            _ => claude_identity(&env, deadline).map(|(state, email)| (state, email, false)),
        };
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
                    _ => "yelo",
                }
                .into(),
            ),
            summary: None,
        });
    }
    if include_usage {
        let meters = crate::usage::for_accounts(&list, deadline, crate::usage::now_seconds())?;
        super::apply_usage(&mut list, &meters);
    }
    for (name, path) in discovery.skipped {
        let path = path.to_string_lossy().into_owned();
        let display_path = path.escape_debug();
        let reason = format!(
            "Skipped native home {display_path}: invalid account name; use 1-{} lowercase ASCII letters, digits, '.', '_' or '-'; no '..' or reserved auto, default, current, '.', '..' names",
            crate::config::MAX_ACCOUNT_NAME_LENGTH
        );
        eprintln!("swarm: {reason}");
        // Reserved labels cannot share the default/current row's stable name.
        let name = if matches!(name.as_str(), "auto" | "default" | "current") {
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
        });
    }
    list.accounts
        .sort_by(|left, right| left.name.cmp(&right.name));
    list.auto = super::pick_auto(&list.accounts, discovery.current.to_str());
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
    let mut child = command
        .args(args)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| NativeReadError::CliUnavailable)?;
    let stdout = child
        .stdout
        .take()
        .ok_or("provider stdout is unavailable")?;
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result =
            std::io::Read::read_to_end(&mut BufReader::new(stdout), &mut bytes).map(|_| bytes);
        let _ = sender.send(result);
    });
    let response = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let mut status = child.try_wait().ok().flatten();
    while response.is_ok() && status.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
        status = child.try_wait().ok().flatten();
    }
    let _ = child.kill();
    let _ = child.wait();
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
