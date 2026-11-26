use super::*;

#[test]
fn audit_regression_usage_does_not_follow_a_home_to_a_new_login() {
    let account = Account {
        name: "work".into(),
        email: Some("new@example.invalid".into()),
        home: "/same-home".into(),
        env: Default::default(),
        auth_state: AuthState::SignedIn,
        usage_state: "missing".into(),
        usage_source: Some("claude_api".into()),
        remaining_pct: None,
        summary: None,

        invalid_home: false,
        fable_spent: false,
        renewable: false,
        login: None,
    };

    let mut meter = status_meter(
        "claude",
        Some(&account),
        "fresh",
        ReasonCode::NoCachedUsage,
        None,
    );
    meter.used_pct = Some(15);
    meter.as_of_seconds = Some(1000);
    meter.reason = None;
    meter.reason_code = None;

    let cache = serde_json::from_value(serde_json::json!({
        "accounts": [{
            "home": account.home,
            "identity": "previous-login-fingerprint",
            "meters": [meter]
        }]
    }))
    .unwrap();
    let unknown = Account {
        name: "unknown".into(),
        email: None,
        ..account.clone()
    };
    let unavailable = Account {
        name: "unavailable".into(),
        auth_state: AuthState::Unavailable,
        ..account.clone()
    };
    let meters = meters_from_cache(
        "claude",
        &[account, unknown, unavailable],
        1000,
        Ok(Some(cache)),
    );
    assert!(
        meters
            .iter()
            .all(|meter| meter.state == "missing" && meter.used_pct.is_none())
    );
    assert_eq!(
        meters[0].reason.as_deref(),
        Some(ReasonCode::NoCachedUsage.text())
    );
    assert_ne!(meters[1].reason, meters[0].reason);
    assert_eq!(meters[1].reason, meters[2].reason);
}
