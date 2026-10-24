//! CLI roles.

use super::*;

/// Whether each runner can start now (ADR 0032). Each provider's accounts are read at most once.
pub(crate) struct Probe {
    pub(crate) deadline: std::time::Instant,
    pub(crate) min_usage_left_pct: u8,
    /// `--account <name>`: only that account counts. None or `auto` counts every account.
    pub(crate) account: Option<String>,
    pub(crate) accounts: std::cell::RefCell<
        std::collections::HashMap<Provider, Result<swarm::profiles::AccountList, String>>,
    >,
}

impl Probe {
    pub(crate) fn new(config: &swarm::config::Config, account: Option<&str>) -> Probe {
        // Account selection starts its budget after launch has checked the host and opened the store.
        Probe {
            deadline: swarm::profiles::native::deadline(ACCOUNT_PICK_TIMEOUT_SECONDS),
            min_usage_left_pct: config.min_usage_left_pct,
            account: account.filter(|name| *name != "auto").map(str::to_string),
            accounts: Default::default(),
        }
    }

    pub(crate) fn check(
        &self,
        runner: &swarm::config::Runner,
    ) -> Option<(swarm::config::SkipCode, String)> {
        let provider = runner.provider;
        if !installed(provider.id()) {
            return Some((
                swarm::config::SkipCode::CliMissing,
                format!("{} CLI not found on PATH", provider.id()),
            ));
        }
        let provider = provider.account_provider(&runner.model)?;
        let mut cache = self.accounts.borrow_mut();
        let accounts = cache
            .entry(provider)
            .or_insert_with(|| pick_accounts(provider, self.deadline, self.account.is_none()));
        // An explicit account bypasses the Fable guard; authentication and general limits still apply.
        let model = self.account.is_none().then_some(runner.model.as_str());
        let scoped = accounts.as_ref().ok().map(|list| list.for_model(model));
        if let Some(list) = &scoped
            && let Err(reason) = swarm::profiles::pick_auto(
                &list.accounts,
                list.current_home.as_deref(),
                model,
                self.min_usage_left_pct,
            )
        {
            return Some((swarm::config::SkipCode::FableSpent, reason));
        }
        let accounts = scoped
            .as_ref()
            .and_then(|list| account_states(list, self.account.as_deref(), model));
        // A named account belongs to one provider; another provider's runner cannot use it.
        if let (Some(name), Some([])) = (&self.account, accounts.as_deref()) {
            return Some((
                swarm::config::SkipCode::SignedOut,
                format!("no {} account named {name}", provider.id()),
            ));
        }
        swarm::config::account_skip(provider, accounts.as_deref(), self.min_usage_left_pct)
    }
}

/// Native authentication uncertainty remains a read failure for ADR 0032.
pub(crate) fn account_states(
    list: &swarm::profiles::AccountList,
    only: Option<&str>,
    model: Option<&str>,
) -> Option<Vec<swarm::config::AccountState>> {
    let rows: Vec<_> = list
        .accounts
        .iter()
        .filter(|account| !account.invalid_home())
        .filter(|account| only.is_none_or(|name| account.name == name))
        .filter(|account| !swarm::profiles::is_fable_model(model) || !account.fable_spent)
        .collect();
    if rows
        .iter()
        .any(|account| account.auth_state == swarm::profiles::AuthState::Unavailable)
    {
        return None;
    }
    Some(
        rows.into_iter()
            .map(|account| swarm::config::AccountState {
                signed_in: account.auth_state == swarm::profiles::AuthState::SignedIn,
                remaining_pct: (account.usage_state == "fresh")
                    .then_some(account.remaining_pct)
                    .flatten(),
            })
            .collect(),
    )
}

/// The runner `role` starts with, as `roles get` prints it. Each skipped runner is one stderr
/// line, and `running` adds the line that names the runner that starts.
pub(crate) fn resolve_role(
    role: &str,
    provider: Option<&str>,
    account: Option<&str>,
    running: bool,
) -> Result<(serde_json::Value, Probe), String> {
    let (config, _) = swarm::config::load().map_err(|error| format!("swarm: {error}"))?;
    let profile = config
        .profile(role)
        .map_err(|error| format!("swarm: {error}"))?;
    let only = provider
        .map(|name| Provider::parse(name).ok_or_else(|| format!("swarm: unknown provider {name}")))
        .transpose()?;
    if let Some(only) = only
        && !profile.runners.iter().any(|runner| runner.provider == only)
    {
        return Err(format!(
            "swarm: {role}: profile has no {} runner",
            only.id()
        ));
    }
    let probe = Probe::new(&config, account);
    let selection = swarm::config::select(profile, only, |runner| probe.check(runner));
    let id = |index: usize| format!("{role}#{}", index + 1);
    let Some(index) = selection.pick else {
        let lines: Vec<String> = selection
            .skipped
            .iter()
            .map(|skip| {
                format!(
                    "  {} {}: {}",
                    skip.index + 1,
                    profile.runners[skip.index].label(),
                    skip.text
                )
            })
            .collect();
        return Err(format!(
            "swarm: {role}: no runner can run\n{}",
            lines.join("\n")
        ));
    };
    // The error above lists each skip once; these lines only go with a runner that starts.
    for skip in &selection.skipped {
        eprintln!(
            "swarm: {role}: skipped {}: {}",
            profile.runners[skip.index].label(),
            skip.text
        );
    }
    let runner = &profile.runners[index];
    if running {
        eprintln!("swarm: {role}: running {}", runner.label());
    }
    let mut value = serde_json::to_value(runner).map_err(|error| error.to_string())?;
    let fields = value
        .as_object_mut()
        .expect("a runner serializes to an object");
    fields.insert("role".into(), role.into());
    fields.insert("runnerId".into(), id(index).into());
    let fallbacks: Vec<String> = (0..profile.runners.len())
        .filter(|other| *other != index)
        .map(id)
        .collect();
    fields.insert("fallbackRunnerIds".into(), fallbacks.into());
    if let Some(first) = selection.skipped.first() {
        fields.insert("substitutedFor".into(), id(first.index).into());
    }
    fields.insert(
        "skipped".into(),
        serde_json::to_value(&selection.skipped).map_err(|error| error.to_string())?,
    );
    Ok((value, probe))
}

/// A provider counts as installed when an executable file of its name is on PATH, because that
/// binary is what a pane runs.
pub(crate) fn installed(provider: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|dir| {
            std::fs::metadata(dir.join(provider))
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
    })
}

pub(crate) fn load_accounts(
    provider: &str,
    deadline: std::time::Instant,
) -> Result<swarm::profiles::AccountList, Box<dyn std::error::Error>> {
    let provider =
        Provider::parse(provider).ok_or_else(|| format!("swarm: unknown provider {provider}"))?;
    swarm::profiles::native::load(provider, deadline)
        .map_err(|error| format!("swarm: {error}").into())
}

/// The accounts for a pick. An automatic pick ranks accounts by usage left, so it first refreshes
/// a usage cache that is missing or older than the freshness limit; a named pick reads no usage.
pub(crate) fn pick_accounts(
    provider: Provider,
    deadline: std::time::Instant,
    auto: bool,
) -> Result<swarm::profiles::AccountList, String> {
    let mut accounts = swarm::profiles::native::load(provider, deadline)?;
    if auto
        && let Some(meters) = swarm::usage::refresh_if_old(
            provider,
            &accounts,
            swarm::profiles::native::deadline(USAGE_PICK_REFRESH_TIMEOUT_SECONDS),
        )
    {
        swarm::profiles::apply_usage(&mut accounts, &meters);
    }
    Ok(accounts)
}
