//! Which native homes are one login.

use super::*;

/// One login in two homes is one account, and the CLI's own home gives way to the named one.
/// The login is the keychain's account: the default home's `auth status` email comes from the
/// shared `~/.claude.json`, which the account that ran last writes.
#[test]
fn one_login_in_two_homes_is_listed_once_under_its_named_home() {
    use sha2::{Digest, Sha256};
    let home = fixture("one-login");
    for name in ["sid", "abhi"] {
        std::fs::create_dir_all(home.join(".claude/.profiles").join(name)).unwrap();
    }
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude.json"),
        r#"{"oauthAccount":{"accountUuid":"u1"}}"#,
    )
    .unwrap();
    tool(
        &home,
        "claude",
        r#"case "$CLAUDE_SECURESTORAGE_CONFIG_DIR" in
  *abhi*) echo '{"loggedIn":true,"email":"abhi@example.test","orgId":"o2"}' ;;
  *) echo '{"loggedIn":true,"email":"sid@example.test","orgId":"o1"}' ;;
esac"#,
    );
    let sid_service = {
        let hash = Sha256::digest(home.join(".claude-sid").to_string_lossy().as_bytes());
        format!("Claude Code-credentials-{}", hex(&hash[..4]))
    };
    let keychain = |default_account: &str| {
        tool(
            &home,
            "security",
            &format!(
                r#"case "$3" in
  "Claude Code-credentials") a={default_account} ;;
  "{sid_service}") a=u1 ;;
  *) a=u2 ;;
esac
echo '{{"claudeAiOauth":{{"accessToken":"t","accountUuid":"'$a'","organizationUuid":"o"}}}}'"#
            ),
        )
    };
    let names = |list: &serde_json::Value| -> Vec<String> {
        list["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap().to_string())
            .collect()
    };
    keychain("u1");
    assert_eq!(names(&accounts(&home, "claude", &[])), ["abhi", "sid"]);
    // The default home holds another login while sid's email is in the shared file.
    keychain("u9");
    let list = accounts(&home, "claude", &[]);
    assert_eq!(names(&list), ["abhi", "default", "sid"]);
    assert_eq!(list["accounts"][1]["email"], serde_json::Value::Null);
    // Without an email, the keychain login still keys the default home's usage cache.
    tool(
        &home,
        "curl",
        "cat >/dev/null; echo '{\"five_hour\":{\"utilization\":10}}'; printf '\\n200'",
    );
    let refresh = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", &home)
        .env("SWARM_HOME", &home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .args(["usage", "--refresh", "--provider", "claude", "--json"])
        .output()
        .unwrap();
    assert!(
        refresh.status.success(),
        "{}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    assert_eq!(
        accounts(&home, "claude", &[])["accounts"][1]["usage_state"],
        "fresh"
    );
    // One email in two workspaces is two accounts; the CLI's own home is a copy of merlin.
    for (dir, workspace) in [
        (".codex", "w1"),
        (".codex-merlin", "w1"),
        (".codex-team", "w2"),
    ] {
        std::fs::create_dir_all(home.join(dir)).unwrap();
        let auth = serde_json::json!({"tokens": {"account_id": workspace}});
        std::fs::write(home.join(dir).join("auth.json"), auth.to_string()).unwrap();
    }
    tool(
        &home,
        "codex",
        r#"read -r _; echo '{"id":1,"result":{}}'; read -r _; read -r _
echo '{"id":2,"result":{"account":{"type":"chatgpt","email":"me@example.test","planType":"pro"}}}'
while read -r _; do :; done"#,
    );
    assert_eq!(names(&accounts(&home, "codex", &[])), ["merlin", "team"]);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
