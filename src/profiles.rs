use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct RoutingState {
    config: RoutingConfig,
}

#[derive(Deserialize)]
struct RoutingConfig {
    routes: BTreeMap<String, Vec<String>>,
    runners: BTreeMap<String, RoutingRunner>,
}

#[derive(Deserialize)]
struct RoutingRunner {
    provider: String,
    model: String,
    effort: Option<String>,
    sandbox: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct RoleList {
    pub roles: Vec<Role>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Role {
    pub role: String,
    pub runner: String,
    pub provider: String,
    pub model: String,
    pub effort: Option<String>,
    pub sandbox: Option<String>,
    pub fallbacks: Vec<String>,
}

pub fn translate_roles(json: &str) -> Result<RoleList, String> {
    let input: RoutingState =
        serde_json::from_str(json).map_err(|error| format!("routing JSON: {error}"))?;
    let mut roles = Vec::with_capacity(input.config.routes.len());
    for (role, runner_ids) in input.config.routes {
        let (runner, fallbacks) = runner_ids
            .split_first()
            .ok_or_else(|| format!("route {role} has no runners"))?;
        let details = input
            .config
            .runners
            .get(runner)
            .ok_or_else(|| format!("route {role} names unknown runner {runner}"))?;
        roles.push(Role {
            role,
            runner: runner.clone(),
            provider: details.provider.clone(),
            model: details.model.clone(),
            effort: details.effort.clone(),
            sandbox: details.sandbox.clone(),
            fallbacks: fallbacks.to_vec(),
        });
    }
    Ok(RoleList { roles })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountList {
    pub provider: String,
    pub source: Option<String>,
    pub accounts: Vec<Account>,
    pub auto: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Account {
    pub name: String,
    pub email: Option<String>,
    pub home: String,
    pub env: BTreeMap<String, String>,
    pub signed_in: bool,
    pub remaining_pct: Option<i64>,
    pub summary: Option<String>,
}

#[derive(Deserialize)]
struct YeloAccount {
    name: Option<String>,
    dir: String,
    email: Option<String>,
    signed_in: bool,
    remaining: Option<i64>,
    usage: Option<String>,
}

#[derive(Deserialize)]
struct YeloPick {
    name: Option<String>,
}

pub fn empty_accounts(provider: &str) -> AccountList {
    AccountList {
        provider: provider.to_string(),
        source: None,
        accounts: Vec::new(),
        auto: None,
    }
}

pub fn translate_accounts(
    provider: &str,
    list_json: &str,
    pick_json: Option<&str>,
) -> Result<AccountList, String> {
    let input: Vec<YeloAccount> =
        serde_json::from_str(list_json).map_err(|error| format!("yelo account JSON: {error}"))?;
    let env_name = match provider {
        "claude" => "CLAUDE_CONFIG_DIR",
        "codex" => "CODEX_HOME",
        _ => return Err(format!("unknown provider {provider}")),
    };
    let accounts: Vec<Account> = input
        .into_iter()
        .filter_map(|row| {
            let name = row.name?;
            Some(Account {
                name,
                email: row.email,
                env: BTreeMap::from([(env_name.to_string(), row.dir.clone())]),
                home: row.dir,
                signed_in: row.signed_in,
                remaining_pct: row.remaining,
                summary: row.usage,
            })
        })
        .collect();
    let auto = pick_json
        .map(|json| serde_json::from_str::<YeloPick>(json).map(|row| row.name))
        .transpose()
        .map_err(|error| format!("yelo pick JSON: {error}"))?
        .flatten();
    if let Some(name) = &auto
        && !accounts
            .iter()
            .any(|account| account.name == *name && account.signed_in)
    {
        return Err(format!("yelo picked unknown or signed-out account {name}"));
    }
    Ok(AccountList {
        provider: provider.to_string(),
        source: Some("yelo".to_string()),
        accounts,
        auto,
    })
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Usage {
    pub meters: Vec<UsageMeter>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct UsageMeter {
    pub provider: String,
    pub account: Option<String>,
    pub label: String,
    pub window: Option<String>,
    pub used_pct: Option<i64>,
    pub resets_in: Option<String>,
    pub state: String,
    pub reason: Option<String>,
    pub as_of: Option<i64>,
}

#[derive(Deserialize)]
struct YeloUsageMeter {
    provider: String,
    label: String,
    window: Option<String>,
    pct: Option<i64>,
    reset: Option<String>,
    state: String,
    reason: Option<String>,
    #[serde(rename = "asOf")]
    as_of: Option<i64>,
}

pub fn translate_usage(
    json: &str,
    account_lists: &[AccountList],
) -> Result<(Usage, Vec<String>), String> {
    let input: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|error| format!("yelo usage JSON: {error}"))?;
    let mut skipped = Vec::new();
    let rows: Vec<YeloUsageMeter> = input
        .into_iter()
        .filter_map(|row| match serde_json::from_value(row) {
            Ok(row) => Some(row),
            Err(error) => {
                skipped.push(error.to_string().replace(['\r', '\n'], " "));
                None
            }
        })
        .collect();
    let meters = rows
        .into_iter()
        .map(|row| {
            let account = row.label.split_once('·').and_then(|(_, tail)| {
                account_lists
                    .iter()
                    .find(|list| list.provider == row.provider)
                    .and_then(|list| {
                        list.accounts
                            .iter()
                            .find(|account| account.email.as_deref() == Some(tail))
                            .or_else(|| list.accounts.iter().find(|account| account.name == tail))
                    })
                    .map(|account| account.name.clone())
            });
            UsageMeter {
                provider: row.provider,
                account,
                label: row.label,
                window: row.window,
                used_pct: row.pct,
                resets_in: row.reset,
                state: row.state,
                reason: row.reason,
                as_of: row.as_of,
            }
        })
        .collect();
    Ok((Usage { meters }, skipped))
}

pub fn resolve_account<'a>(
    accounts: &'a AccountList,
    requested: &str,
) -> Result<&'a Account, String> {
    let name = if requested == "auto" {
        accounts
            .auto
            .as_deref()
            .ok_or_else(|| format!("no automatic account for {}", accounts.provider))?
    } else {
        requested
    };
    accounts
        .accounts
        .iter()
        .find(|account| account.name == name)
        .ok_or_else(|| format!("unknown {} account {name}", accounts.provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT_LIST: &str = r#"[
        {"name":"work","dir":"/profiles/work","email":"work@example.com","signed_in":true,"remaining":52,"usage":"5h 98% left · 7d 52% left"},
        {"name":"away","dir":"/profiles/away","email":null,"signed_in":false,"remaining":null,"usage":null},
        {"name":null,"dir":"/profiles/nameless","email":null,"signed_in":true,"remaining":90,"usage":"7d 90% left"}
    ]"#;

    #[test]
    fn translates_routes_and_keeps_fallback_order() {
        let json = r#"{"path":"/roles.json","config":{"routes":{"ORCHESTRATOR":["claudeLead","codexBackup"],"CODER":["codexWork"]},"runners":{"claudeLead":{"provider":"claude","model":"opus","effort":null},"codexBackup":{"provider":"codex","model":"gpt-backup","sandbox":"workspace-write"},"codexWork":{"provider":"codex","model":"gpt-work","effort":"high","sandbox":"workspace-write"}}}}"#;

        let result = translate_roles(json).unwrap();

        assert_eq!(result.roles[1].runner, "claudeLead");
        assert_eq!(result.roles[1].fallbacks, ["codexBackup"]);
        assert_eq!(result.roles[1].effort, None);
        assert_eq!(result.roles[1].sandbox, None);
    }

    #[test]
    fn translates_accounts_and_resolves_auto() {
        let result = translate_accounts(
            "claude",
            ACCOUNT_LIST,
            Some(r#"{"name":"work","dir":"/profiles/work"}"#),
        )
        .unwrap();

        assert_eq!(result.source.as_deref(), Some("yelo"));
        assert_eq!(
            result.accounts[0].env["CLAUDE_CONFIG_DIR"],
            "/profiles/work"
        );
        assert_eq!(result.accounts[0].remaining_pct, Some(52));
        assert_eq!(result.accounts.len(), 2);
        assert_eq!(resolve_account(&result, "auto").unwrap().name, "work");
        assert_eq!(
            resolve_account(&result, "missing").unwrap_err(),
            "unknown claude account missing"
        );
    }

    #[test]
    fn a_nameless_pick_has_no_auto_account() {
        let result = translate_accounts(
            "codex",
            ACCOUNT_LIST,
            Some(
                r#"{"name":null,"dir":"/profiles/nameless","email":null,"signed_in":true,"remaining":90,"usage":"7d 90% left"}"#,
            ),
        )
        .unwrap();

        assert_eq!(result.auto, None);
        assert_eq!(
            resolve_account(&result, "auto").unwrap_err(),
            "no automatic account for codex"
        );
    }

    #[test]
    fn translates_usage_status_rows_and_matches_email_then_name() {
        let accounts =
            translate_accounts("codex", ACCOUNT_LIST, Some(r#"{"name":"work"}"#)).unwrap();
        let json = r#"[
            {"label":"cx·work@example.com","provider":"codex","window":"7d","pct":10,"reset":"4d22h","state":"ok","asOf":1789576942},
            {"label":"cx·unknown@example.com","provider":"codex","window":"5h","pct":20,"reset":null,"state":"stale","asOf":null},
            {"label":"cx","provider":"codex","state":"logged_out","reason":"logged out"},
            {"label":"cx·away","provider":"codex","state":"missing","reason":"no data"},
            {"provider":"codex"}
        ]"#;

        let (result, skipped) = translate_usage(json, &[accounts]).unwrap();

        assert_eq!(result.meters[0].account.as_deref(), Some("work"));
        assert_eq!(result.meters[1].account, None);
        assert_eq!(result.meters[1].resets_in, None);
        assert_eq!(result.meters[2].account, None);
        assert_eq!(result.meters[2].window, None);
        assert_eq!(result.meters[2].used_pct, None);
        assert_eq!(result.meters[2].reason.as_deref(), Some("logged out"));
        assert_eq!(result.meters[3].account.as_deref(), Some("away"));
        assert_eq!(result.meters.len(), 4);
        assert_eq!(skipped, ["missing field `label`"]);
    }

    #[test]
    fn agy_has_no_account_source() {
        assert_eq!(
            empty_accounts("agy"),
            AccountList {
                provider: "agy".into(),
                source: None,
                accounts: vec![],
                auto: None
            }
        );
    }
}
