//! The identity of one native Claude home.

use super::*;

/// The state, email, and login of one Claude home. The login is the keychain item's account and
/// organization ids. `claude auth status` takes its email from the config file, and every home
/// without its own `CLAUDE_CONFIG_DIR` shares `~/.claude.json`, where the account that ran last
/// writes its email. So that email is kept only when the file names the keychain's account.
pub(super) fn claude_identity(
    name: &str,
    home: &str,
    env: &BTreeMap<String, String>,
    deadline: Instant,
) -> Result<(AuthState, Option<String>, Option<String>), NativeReadError> {
    let executable = std::env::var("SWARM_CLAUDE_CMD").unwrap_or_else(|_| "claude".into());
    let value = read_json(&executable, &["auth", "status"], env, deadline)?;
    let state = match value["loggedIn"].as_bool() {
        Some(true) => AuthState::SignedIn,
        Some(false) => AuthState::SignedOut,
        None => return Err("Claude identity is unavailable".into()),
    };
    let service = crate::usage::claude_keychain_service(
        crate::providers::secure_storage_dir(home, name).as_deref(),
    );
    let security = std::env::var("SWARM_SECURITY_CMD").unwrap_or_else(|_| "security".into());
    let user = std::env::var("USER").unwrap_or_default();
    let credentials = read_json(
        &security,
        &["find-generic-password", "-s", &service, "-a", &user, "-w"],
        &BTreeMap::new(),
        deadline,
    )
    .ok();
    let id = |key: &str| {
        credentials
            .as_ref()
            .and_then(|value| value["claudeAiOauth"][key].as_str())
            .map(str::to_string)
    };
    let account = id("accountUuid");
    let login = account
        .as_ref()
        .zip(id("organizationUuid"))
        .map(|(account, org)| format!("{account}/{org}"));
    let config = match env.get("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir).join(".claude.json"),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".claude.json"),
    };
    let config_account = std::fs::read(config)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|config| {
            config["oauthAccount"]["accountUuid"]
                .as_str()
                .map(str::to_string)
        });
    let email = value["email"]
        .as_str()
        .filter(|_| match (&account, &config_account) {
            (Some(account), Some(config)) => account == config,
            _ => true,
        })
        .map(str::to_string);
    Ok((state, email, login))
}
