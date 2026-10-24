//! CLI accounts.

use super::*;

/// `--account menu`: the accounts as a numbered list on `out`, then the choice read from `input`.
/// Enter alone is `auto`; a number or a name picks that account.
pub(crate) fn menu_choice(
    accounts: &swarm::profiles::AccountList,
    input: &mut impl std::io::BufRead,
    out: &mut impl std::io::Write,
) -> Result<String, String> {
    let names: Vec<&str> = accounts.accounts.iter().map(|a| a.name.as_str()).collect();
    if names.is_empty() {
        return Err(format!(
            "swarm: no {} accounts; add one with swarm accounts login --provider {} --name NAME",
            accounts.provider, accounts.provider
        ));
    }
    let width = names.iter().map(|n| n.len()).max().unwrap_or(0);
    let write = |out: &mut dyn std::io::Write, text: String| {
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())
    };
    for (index, account) in accounts.accounts.iter().enumerate() {
        let left = match (&account.auth_state, account.remaining_pct) {
            (swarm::profiles::AuthState::SignedIn, Some(pct)) => format!("{pct}% left"),
            (swarm::profiles::AuthState::SignedIn, None) => "usage unknown".into(),
            _ => "signed out".into(),
        };
        write(
            out,
            format!(
                "{:>2}) {:width$}  {:28}  {left}\n",
                index + 1,
                account.name,
                account.email.as_deref().unwrap_or("")
            ),
        )?;
    }
    write(out, format!("Account [1-{}, Enter = auto]: ", names.len()))?;
    out.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|e| e.to_string())?;
    let line = line.trim();
    if line.is_empty() {
        return Ok("auto".into());
    }
    match line.parse::<usize>() {
        Ok(n) if (1..=names.len()).contains(&n) => Ok(names[n - 1].into()),
        _ if names.contains(&line) => Ok(line.into()),
        _ => Err(format!("swarm: no account {line:?} in the list")),
    }
}

/// Shows the menu on the terminal, not on stdout, so `accounts pick --json` output stays JSON.
pub(crate) fn ask_account(
    accounts: Result<&swarm::profiles::AccountList, String>,
    model: Option<&str>,
) -> Result<String, String> {
    let accounts = accounts
        .map_err(|error| format!("swarm: {error}"))?
        .for_model(model);
    let tty = std::fs::File::open("/dev/tty")
        .map_err(|_| "swarm: --account menu needs a terminal; name the account instead")?;
    menu_choice(
        &accounts,
        &mut std::io::BufReader::new(tty),
        &mut std::io::stderr(),
    )
}

pub(crate) fn select_account(
    provider: Provider,
    requested: &str,
    model: Option<&str>,
    min_usage_left_pct: u8,
    accounts: Result<&swarm::profiles::AccountList, String>,
) -> Result<Option<swarm::profiles::Account>, String> {
    let mut accounts = accounts.map(|accounts| accounts.for_model(model));
    if requested == "auto"
        && let Ok(accounts) = &mut accounts
    {
        accounts.auto = swarm::profiles::pick_auto(
            &accounts.accounts,
            accounts.current_home.as_deref(),
            model,
            min_usage_left_pct,
        )
        .map_err(|error| format!("swarm: {error}"))?;
    }
    // A signed-out account the owner names still opens the CLI, so its own login can sign it in.
    if requested != "auto"
        && let Ok(list) = &accounts
        && let Some(account) = list.accounts.iter().find(|account| {
            account.name == requested && account.auth_state == swarm::profiles::AuthState::SignedOut
        })
    {
        eprintln!(
            "swarm: {} account {requested} is signed out; sign in from the CLI",
            provider.id()
        );
        return Ok(Some(account.clone()));
    }
    match accounts
        .and_then(|accounts| swarm::profiles::resolve_account(&accounts, requested).cloned())
    {
        Ok(account) => Ok(Some(account)),
        Err(error) if requested == "auto" => {
            eprintln!("swarm: {error}; {} uses its own login", provider.id());
            Ok(None)
        }
        Err(error) => Err(format!("swarm: {error}")),
    }
}

pub(crate) fn run_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::process::CommandExt;
    let (options, extra) = match args.iter().position(|arg| arg == "--") {
        Some(index) => (&args[..index], &args[index + 1..]),
        None => (args, &[][..]),
    };
    let (provider, requested) = match options {
        [provider] => (provider, "auto"),
        // `--account` with no name opens the menu.
        [provider, flag] if flag == "--account" => (provider, "menu"),
        [provider, flag, account] if flag == "--account" => (provider, account.as_str()),
        _ => return Err(USAGE.into()),
    };
    let provider = Provider::parse(provider)
        .filter(|provider| provider.has_accounts())
        .ok_or("swarm: run requires claude or codex")?;
    let accounts = pick_accounts(
        provider,
        account_pick_deadline(),
        matches!(requested, "auto" | "menu"),
    );
    let chosen;
    let requested = if requested == "menu" {
        chosen = ask_account(
            accounts.as_ref().map_err(Clone::clone),
            swarm::bus::command_model(extra),
        )?;
        chosen.as_str()
    } else {
        requested
    };
    let account = select_account(
        provider,
        requested,
        swarm::bus::command_model(extra),
        swarm::config::min_usage_left_pct(),
        accounts.as_ref().map_err(Clone::clone),
    )?;
    let mut command = std::process::Command::new(provider.id());
    command.args(extra);
    for key in provider.account_env_keys() {
        command.env_remove(key);
    }
    if let Some(account) = account {
        eprintln!("account {}", account.name);
        command.envs(account.env);
    }
    Err(format!("swarm: cannot run {}: {}", provider.id(), command.exec()).into())
}

/// The account `swarm run` would use, for a shell wrapper such as `pi` that starts the CLI itself.
/// `clear` names the account variables to unset first; `name` is null when the CLI keeps its own
/// login. `--session ID` or `--last [--all]` picks the Codex account that holds that session.
pub(crate) fn account_pick(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = std::collections::BTreeMap::new();
    let mut flags = std::collections::BTreeSet::new();
    let mut arguments = args.iter();
    while let Some(flag) = arguments.next() {
        if ["--json", "--last", "--all"].contains(&flag.as_str()) {
            if !flags.insert(flag.as_str()) {
                return Err(USAGE.into());
            }
            continue;
        }
        if !["--provider", "--account", "--model", "--session"].contains(&flag.as_str()) {
            return Err(USAGE.into());
        }
        let value = arguments.next().ok_or(USAGE)?;
        if options.insert(flag.as_str(), value.as_str()).is_some() {
            return Err(USAGE.into());
        }
    }
    let provider = options
        .get("--provider")
        .and_then(|name| Provider::parse(name))
        .filter(|provider| provider.has_accounts())
        .ok_or("swarm: accounts pick needs claude or codex")?;
    use swarm::profiles::native::CodexSession;
    let session = match (options.get("--session"), flags.contains("--last")) {
        (Some(id), false) if !flags.contains("--all") => Some(CodexSession::Id(id.to_string())),
        (None, true) => Some(CodexSession::Last(
            (!flags.contains("--all"))
                .then(std::env::current_dir)
                .transpose()?,
        )),
        (None, false) if !flags.contains("--all") => None,
        _ => return Err(USAGE.into()),
    };
    if !flags.contains("--json") || session.is_some() && options.contains_key("--account") {
        return Err(USAGE.into());
    }
    if session.is_some() && provider != Provider::Codex {
        return Err("swarm: --session and --last pick only codex accounts".into());
    }
    let mut requested = options
        .get("--account")
        .copied()
        .unwrap_or("auto")
        .to_string();
    let accounts = pick_accounts(
        provider,
        account_pick_deadline(),
        matches!(requested.as_str(), "auto" | "menu") && session.is_none(),
    );
    if requested == "menu" {
        requested = ask_account(
            accounts.as_ref().map_err(Clone::clone),
            options.get("--model").copied(),
        )?;
    }
    if let Some(session) = &session {
        let list = accounts
            .as_ref()
            .map_err(|error| format!("swarm: {error}"))?;
        requested = swarm::profiles::native::codex_session_owner(&list.accounts, session)
            .ok_or(match session {
                CodexSession::Id(id) => format!("swarm: session {id} is not in any codex account"),
                CodexSession::Last(Some(cwd)) => {
                    format!("swarm: no codex session in {}", cwd.display())
                }
                CodexSession::Last(None) => "swarm: no codex session in any account".into(),
            })?
            .name
            .clone();
    }
    let account = select_account(
        provider,
        &requested,
        options.get("--model").copied(),
        swarm::config::min_usage_left_pct(),
        accounts.as_ref().map_err(Clone::clone),
    )?;
    print_json(&serde_json::json!({
        "schema": "swarm.account-pick.v1",
        "name": account.as_ref().map(|a| &a.name),
        "home": account.as_ref().map(|a| &a.home),
        "env": account.as_ref().map(|a| a.env.clone()).unwrap_or_default(),
        "clear": provider.account_env_keys(),
    }))
}

pub(crate) fn account_login(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if swarm::host::is_worker(|name| env::var(name).ok()) {
        return Err("swarm: only the owner or orchestrator can open a login pane".into());
    }
    let mut options = std::collections::BTreeMap::new();
    let mut json = false;
    let mut arguments = args.iter();
    while let Some(flag) = arguments.next() {
        if flag == "--json" && !json {
            json = true;
            continue;
        }
        if !["--provider", "--name", "--revision", "--cwd"].contains(&flag.as_str()) {
            return Err("swarm: invalid accounts login option".into());
        }
        let value = arguments
            .next()
            .ok_or("swarm: missing accounts login value")?;
        if options.insert(flag.as_str(), value.as_str()).is_some() {
            return Err("swarm: duplicate accounts login option".into());
        }
    }
    if !json {
        return Err("swarm: accounts login requires --json".into());
    }
    let provider = options
        .get("--provider")
        .and_then(|name| Provider::parse(name))
        .filter(|provider| provider.has_accounts())
        .ok_or("swarm: login needs claude or codex")?;
    let name = options.get("--name").ok_or("swarm: login needs --name")?;
    let expected = options
        .get("--revision")
        .ok_or("swarm: login needs --revision")?;
    let cwd = options
        .get("--cwd")
        .map_or_else(env::current_dir, |value| {
            Ok(std::path::PathBuf::from(value))
        })?;
    if !cwd.is_absolute() || !cwd.is_dir() {
        return Err("swarm: --cwd must be an existing absolute directory".into());
    }
    let cwd = std::fs::canonicalize(cwd).map_err(|_| "swarm: --cwd is unavailable")?;
    let deadline = swarm::profiles::native::deadline(LOGIN_ADAPTER_TIMEOUT_SECONDS);
    let (entry, revision) = swarm::accounts::register(provider, name, expected)?;
    let root = swarm::paths::root_dir()?;
    let adapter = swarm::adapter::Adapter {
        deadline: Some(deadline),
        ..swarm::adapter::load(&root, &adapter_name())
            .map_err(|_| "swarm: account registered; login pane adapter is unavailable")?
    };
    env::set_current_dir(cwd)?;
    // Eight label characters and a UUID suffix keep the unique attempt ID below 40 bytes.
    let label: String = name
        .chars()
        .take(8)
        .map(|ch| if matches!(ch, '.' | '_') { '-' } else { ch })
        .collect();
    let attempt = uuid::Uuid::now_v7().simple().to_string();
    let pane_name = format!("login-{}-{label}-{}", provider.id(), &attempt[16..]);
    let session = env::var("SWARM_SESSION_ID").unwrap_or_default();
    let home = swarm::paths::home()?;
    let pane = adapter
        .run(
            "spawn",
            &[
                ("agent_id", &pane_name),
                ("session_id", &session),
                ("home", &home),
                ("adapter", &adapter.name),
            ],
        )
        .map_err(|_| "swarm: account registered; cannot open login pane")?;
    if pane.is_empty() || pane.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("swarm: account registered; login pane has no valid ID".into());
    }
    let native_env = provider.identity_env(name, &entry.home.to_string_lossy());
    let mut command = vec!["env".to_string()];
    for key in provider.account_env_keys() {
        command.extend(["-u".into(), key.to_string()]);
    }
    command.push("--".into());
    command.extend(
        native_env
            .iter()
            .map(|(key, value)| format!("{key}={value}")),
    );
    command.extend(
        provider
            .login_argv()
            .ok_or("swarm: native login is unavailable")?
            .iter()
            .map(|arg| arg.to_string()),
    );
    // Keep a failed login and its error text visible; a successful login closes the shell.
    let text = format!("{} && exit", swarm::adapter::shell_line(&command));
    if adapter
        .run("ring", &[("pane", &pane), ("text", &text)])
        .is_err()
    {
        // Cleanup also runs after ring exhausts its budget, within the Swift process limit.
        let closer = swarm::adapter::Adapter {
            deadline: Some(swarm::profiles::native::deadline(
                LOGIN_PANE_CLOSE_TIMEOUT_SECONDS,
            )),
            ..adapter
        };
        if closer.run("close", &[("pane", &pane)]).is_err() {
            return Err(format!(
                "swarm: account registered; cannot start native login; pane {pane} stayed open because close failed"
            )
            .into());
        }
        return Err("swarm: account registered; cannot start native login in pane".into());
    }
    print_json(
        &serde_json::json!({"provider":provider.id(), "account":entry.name, "pane":pane, "state":"opened", "revision":revision}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_menu_lists_each_account_and_reads_a_number_a_name_or_enter() {
        let mut list = swarm::profiles::empty_accounts("claude");
        for (name, pct, auth) in [
            ("work", Some(87), swarm::profiles::AuthState::SignedIn),
            ("personal", None, swarm::profiles::AuthState::SignedOut),
        ] {
            list.accounts.push(swarm::profiles::Account {
                name: name.into(),
                email: Some(format!("{name}@example.com")),
                home: String::new(),
                env: Default::default(),
                auth_state: auth,
                usage_state: String::new(),
                usage_source: None,
                remaining_pct: pct,
                summary: None,
                invalid_home: false,
                fable_spent: false,
                renewable: false,
            });
        }
        let pick = |list: &swarm::profiles::AccountList, typed: &str| {
            let mut out = Vec::new();
            let choice = menu_choice(list, &mut typed.as_bytes(), &mut out);
            (choice, String::from_utf8(out).unwrap())
        };
        let (choice, shown) = pick(&list, "2\n");
        assert_eq!(choice.unwrap(), "personal");
        assert!(shown.contains(" 1) work      work@example.com"), "{shown}");
        assert!(
            shown.contains("87% left") && shown.contains("signed out"),
            "{shown}"
        );
        assert_eq!(pick(&list, "work\n").0.unwrap(), "work");
        assert_eq!(pick(&list, "\n").0.unwrap(), "auto");
        assert!(pick(&list, "3\n").0.is_err());
        list.accounts.clear();
        assert!(pick(&list, "1\n").0.unwrap_err().contains("accounts login"));
    }

    #[test]
    fn login_adapter_returns_before_the_swift_process_limit() {
        let deadline = swarm::profiles::native::deadline(LOGIN_ADAPTER_TIMEOUT_SECONDS);
        assert!(
            deadline.duration_since(std::time::Instant::now())
                <= std::time::Duration::from_secs(18)
        );
    }
}
