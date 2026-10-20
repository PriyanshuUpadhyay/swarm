use crate::profiles::native::{AppServer, read_json, read_stdout};
use crate::profiles::{
    Account, AccountList, AuthState, ReasonCode, ResetCredits, Usage, UsageMeter,
};
use crate::providers::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const MAX_AGE_SECONDS: i64 = 300;
/// The version of the printed usage format (`docs/usage-format.md`).
pub const USAGE_SCHEMA: &str = "swarm.usage.v1";
/// The last second of year 9999. A reset time in text has the same limit.
const MAX_RESET_SECONDS: i64 = 253_402_300_799;

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    accounts: Vec<CachedAccount>,
}

#[derive(Serialize, Deserialize)]
struct CachedAccount {
    home: String,
    meters: Vec<UsageMeter>,
}

mod cache;
mod claude;
mod codex;
mod openusage;

pub use cache::*;
use claude::fresh_claude;
use codex::fresh_codex;
pub use openusage::openusage;

#[cfg(test)]
mod tests {
    use super::cache::fold_twins;
    use super::claude::{claude_keychain_service, claude_reset_seconds, claude_windows};
    use super::codex::normalize_limits;
    use super::openusage::iso_time;
    use super::*;

    #[test]
    fn keychain_service_matches_the_adopted_profile_vector() {
        assert_eq!(claude_keychain_service(None), "Claude Code-credentials");
        assert_eq!(
            claude_keychain_service(Some("/Users/example/.claude-work")),
            "Claude Code-credentials-dd1118a7"
        );
    }

    #[test]
    fn claude_windows_keep_failed_values_and_accept_the_api_aliases() {
        let mut account = work();
        account.name = "personal".into();
        let meters = claude_windows(
            &account,
            &json!({
                "fiveHour":{"used_percent":"12.6","reset_at":"2026-10-12T01:02:03.123Z"},
                "sevenDay":{"percent":101},
                "seven_day_fable":{"utilization":90}
            }),
            1000,
        )
        .unwrap();
        assert_eq!(meters[0].window.as_deref(), Some("5h"));
        assert_eq!(meters[0].used_pct, Some(13));
        assert_eq!(meters[0].reset_time_seconds, Some(1791766923));
        assert_eq!(meters[0].as_of_seconds, Some(1000));
        assert_eq!(meters[1].state, "failed");
        assert_eq!(meters[1].used_pct, None);
        assert_eq!(meters[2].window.as_deref(), Some("fb"));
        assert_eq!(meters[2].window_minutes, Some(10080));
        assert!(claude_windows(&account, &json!({}), 1000).is_err());
        for payload in [
            json!({"seven_day":{"utilization":40,"resets_at":"2026-10-12T01:02:03Z"}}),
            json!({"five_hour":null,"seven_day":{"utilization":40,"resets_at":"2026-10-12T01:02:03Z"}}),
        ] {
            let meters = claude_windows(&account, &payload, 1000).unwrap();
            assert_eq!(meters.len(), 1, "{payload}");
            assert_eq!(meters[0].window.as_deref(), Some("7d"));
            assert_eq!(meters[0].state, "fresh");
            assert_eq!(meters[0].used_pct, Some(40));
        }
        // A time in milliseconds is not a time in seconds. The limit is the end of year 9999,
        // as for a time in text.
        assert_eq!(
            claude_reset_seconds(&json!(253402300799_i64)),
            Some(253402300799)
        );
        for value in [
            json!(1791766923000_i64),
            json!("1791766923000"),
            json!(-1),
            json!("2026-02-29T01:02:03Z"),
            json!("2026-10-12T24:02:03Z"),
            json!("2026-10-12T01:02:03.éZ"),
            json!("２０２６-10-12T01:02:03Z"),
        ] {
            assert_eq!(claude_reset_seconds(&value), None);
        }
        assert_eq!(
            claude_reset_seconds(&json!("2024-02-29T00:00:00Z")),
            Some(1709164800)
        );
    }

    fn work() -> Account {
        Account {
            name: "work".into(),
            email: None,
            home: "/tmp/demo/.codex-work".into(),
            env: BTreeMap::new(),
            auth_state: AuthState::SignedIn,
            remaining_pct: None,
            usage_state: "missing".into(),
            usage_source: Some("codex_app_server".into()),
            summary: None,
            invalid_home: false,
            fable_spent: false,
            renewable: false,
        }
    }

    #[test]
    fn accounts_with_one_email_show_once_and_the_one_with_a_reading_wins() {
        let meter = |account: &str, used: Option<i64>| UsageMeter {
            account: Some(account.into()),
            used_pct: used,
            ..status_meter("claude", None, "missing", ReasonCode::NoCachedUsage, None)
        };
        let emails = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(n, e)| (n.to_string(), e.to_string()))
                .collect()
        };
        let names = |meters: Vec<UsageMeter>| -> Vec<String> {
            meters
                .into_iter()
                .filter_map(|meter| meter.account)
                .collect()
        };
        let same = emails(&[
            ("default", "login-one"),
            ("me", "login-one"),
            ("team", "login-team"),
        ]);
        let read = vec![
            meter("default", Some(3)),
            meter("me", None),
            meter("team", Some(9)),
        ];
        assert_eq!(names(fold_twins(&same, read)), ["default", "team"]);
        let mut stale = meter("default", Some(40));
        stale.state = "stale".into();
        let mut fresh = meter("me", Some(5));
        fresh.state = "fresh".into();
        assert_eq!(names(fold_twins(&same, vec![stale, fresh])), ["me"]);
        let unread = vec![meter("default", None), meter("me", None)];
        assert_eq!(names(fold_twins(&same, unread)), ["me"]);
        let distinct = emails(&[("default", "login-a"), ("me", "login-b")]);
        let both = vec![meter("default", None), meter("me", None)];
        assert_eq!(names(fold_twins(&distinct, both)), ["default", "me"]);
    }

    #[test]
    fn cache_age_boundary_future_time_and_bad_states_stay_explicit() {
        let account = work();
        let mut meter = normalize_limits(
            &account,
            &json!({"rateLimits":{"primary":{"usedPercent":0}}}),
            1000,
        )
        .unwrap()
        .remove(0);
        assert_eq!(normalize_meter(&meter, 1300).state, "fresh");
        assert_eq!(normalize_meter(&meter, 1301).state, "stale");
        assert_eq!(normalize_meter(&meter, 999).state, "stale");
        meter.as_of_seconds = Some(i64::MIN);
        assert_eq!(normalize_meter(&meter, 1000).state, "stale");
        meter.state = "new-provider-state".into();
        meter.reason = Some("private server diagnostics".into());
        let unknown = normalize_meter(&meter, 1000);
        assert_eq!(unknown.state, "failed");
        assert_eq!(unknown.used_pct, None);
        assert!(!unknown.reason.unwrap().contains("private"));
        assert_eq!(unknown.reason_code, Some(ReasonCode::ReadFailed));
    }

    #[test]
    fn iso_time_gives_utc_with_milliseconds() {
        assert_eq!(iso_time(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_time(951_782_400), "2000-02-29T00:00:00.000Z");
        assert_eq!(iso_time(1_791_763_200), "2026-10-12T00:00:00.000Z");
        assert_eq!(iso_time(1_790_990_000), "2026-10-03T01:13:20.000Z");
    }

    #[test]
    fn openusage_export_keeps_only_known_windows_and_names_each_failed_account() {
        let meter =
            |provider: &str, account: Option<&str>, window: &str, minutes, used, state: &str| {
                let mut meter = UsageMeter {
                    provider: provider.into(),
                    account: account.map(str::to_string),
                    label: account.unwrap_or(provider).into(),
                    window: Some(window.into()),
                    window_minutes: Some(minutes),
                    used_pct: Some(used),
                    reset_time_seconds: Some(1_791_003_600),
                    state: state.into(),
                    source: None,
                    reason: None,
                    reason_code: None,
                    as_of_seconds: Some(1_791_000_000),
                    reset_credits: None,
                };
                if state == "stale" {
                    meter.set_reason(ReasonCode::RefreshFailed, None);
                    meter.as_of_seconds = Some(1_790_990_000);
                }
                meter
            };
        let mut weekly = meter("claude", Some("default"), "7d", 10080, 10, "fresh");
        weekly.reset_time_seconds = Some(1_791_604_800);
        let mut fable = meter("claude", Some("work"), "fb", 10080, 100, "stale");
        fable.reset_time_seconds = None;
        let mut codex = meter("codex", Some("default"), "codex:primary", 300, 30, "fresh");
        codex.as_of_seconds = Some(1_791_000_060);
        let account = |name: &str| Account {
            name: name.into(),
            ..work()
        };
        let usage = Usage {
            meters: vec![
                meter("claude", Some("default"), "5h", 300, 42, "fresh"),
                weekly,
                meter("claude", Some("work"), "5h", 300, 20, "stale"),
                fable,
                status_meter(
                    "claude",
                    Some(&account("broken")),
                    "failed",
                    ReasonCode::ReadFailed,
                    Some("Claude usage read failed"),
                ),
                status_meter(
                    "claude",
                    Some(&account("spare")),
                    "missing",
                    ReasonCode::NoCachedUsage,
                    None,
                ),
                codex,
                meter("codex", Some("default"), "review:primary", 60, 80, "fresh"),
                status_meter("agy", None, "no_source", ReasonCode::NoSource, None),
            ],
        };
        let resource = |used: i64, reset: Option<&str>| {
            let mut value = json!({
                "kind": "consumption", "unit": "percent", "used": used, "limit": 100,
                "remaining": 100 - used, "utilization": used as f64 / 100.0,
            });
            if let Some(reset) = reset {
                value["resetsAt"] = reset.into();
            }
            value
        };
        let with_window = |mut value: Value, seconds: i64| {
            value["windowSeconds"] = seconds.into();
            value
        };
        let expected = json!({
            "schema": "openusage.limits.v1",
            "generatedAt": "2026-10-03T04:01:40.000Z",
            "providers": {
                "claude": {
                    "displayName": "Claude",
                    "fetchedAt": "2026-10-03T04:00:00.000Z",
                    "expiresAt": "2026-10-03T04:05:00.000Z",
                    "stale": false,
                    "resources": {
                        "session": with_window(resource(42, Some("2026-10-03T05:00:00.000Z")), 18000),
                        "weekly": with_window(resource(10, Some("2026-10-10T04:00:00.000Z")), 604800),
                    },
                },
                "claude@work": {
                    "displayName": "Claude (work)",
                    "fetchedAt": "2026-10-03T01:13:20.000Z",
                    "expiresAt": "2026-10-03T01:18:20.000Z",
                    "stale": true,
                    "resources": {
                        "session": with_window(resource(20, Some("2026-10-03T05:00:00.000Z")), 18000),
                        "fable": with_window(resource(100, None), 604800),
                    },
                },
                "codex": {
                    "displayName": "Codex",
                    "fetchedAt": "2026-10-03T04:01:00.000Z",
                    "expiresAt": "2026-10-03T04:06:00.000Z",
                    "stale": false,
                    "resources": {
                        "session": with_window(resource(30, Some("2026-10-03T05:00:00.000Z")), 18000),
                    },
                },
            },
            "errors": [{"providerId": "claude@broken", "message": "Claude usage read failed"}],
        });
        assert_eq!(
            serde_json::to_string(&openusage(&usage, 1_791_000_100)).unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
    }

    #[test]
    fn openusage_leaves_out_a_window_length_that_does_not_fit_in_seconds() {
        let mut meter = status_meter(
            "claude",
            Some(&work()),
            "fresh",
            ReasonCode::ReadFailed,
            None,
        );
        meter.clear_reason();
        meter.window = Some("5h".into());
        meter.window_minutes = Some(i64::MAX);
        meter.used_pct = Some(10);
        meter.as_of_seconds = Some(1000);
        let value = openusage(
            &Usage {
                meters: vec![meter],
            },
            1000,
        );
        let session = &value["providers"]["claude@work"]["resources"]["session"];
        assert_eq!(session["used"], 10);
        assert!(session.get("windowSeconds").is_none(), "{session}");
    }

    #[test]
    fn a_cached_reading_with_half_a_reason_reads_back_as_a_kept_reading() {
        for (reason, code) in [
            (json!(null), json!("relogin_required")),
            (json!("x"), json!(null)),
        ] {
            let stored: UsageMeter = serde_json::from_value(json!({
                "provider": "claude", "account": "work", "label": "work", "window": "5h",
                "window_minutes": 300, "used_pct": 20, "reset_time_seconds": null, "state": "fresh",
                "source": "claude_api", "reason": reason, "reason_code": code, "as_of_seconds": 1000
            }))
            .unwrap();
            let cached = normalize_meter(&stored, 1000);
            assert_eq!(cached.used_pct, Some(20));
            assert_eq!(
                (cached.reason_code, cached.reason.as_deref()),
                (
                    Some(ReasonCode::RefreshFailed),
                    Some(ReasonCode::RefreshFailed.text())
                ),
                "{reason} {code}"
            );
        }
    }

    #[test]
    fn a_reset_time_past_year_9999_is_invalid_from_codex_and_from_the_cache() {
        // The Claude path rejects the same value (see claude_reset_seconds).
        let reply = json!({"rateLimits":{"primary":{"usedPercent":5,"windowDurationMins":300,"resetsAt":1791766923000_i64}}});
        assert_eq!(
            normalize_limits(&work(), &reply, 1000).unwrap_err(),
            "Invalid Codex usage window"
        );
        let bound =
            json!({"rateLimits":{"primary":{"usedPercent":5,"resetsAt":MAX_RESET_SECONDS}}});
        let mut meter = normalize_limits(&work(), &bound, 1000).unwrap().remove(0);
        assert_eq!(meter.reset_time_seconds, Some(MAX_RESET_SECONDS));
        assert_eq!(normalize_meter(&meter, 1000).state, "fresh");
        meter.reset_time_seconds = Some(MAX_RESET_SECONDS + 1);
        let cached = normalize_meter(&meter, 1000);
        assert_eq!(
            (cached.state.as_str(), cached.reset_time_seconds),
            ("failed", None)
        );
    }

    #[test]
    fn a_codex_reply_with_no_window_and_a_cached_read_keep_their_codes() {
        let empty = normalize_limits(&work(), &json!({"rateLimits":{}}), 1000).unwrap();
        assert_eq!(
            (empty[0].reason_code, empty[0].reason.as_deref()),
            (
                Some(ReasonCode::NoQuotaWindows),
                Some("No Codex quota windows")
            )
        );
        for (state, code) in [
            ("missing", ReasonCode::NoCredentials),
            ("missing", ReasonCode::NoQuotaWindows),
            ("failed", ReasonCode::ReloginRequired),
            ("failed", ReasonCode::TokenExpired),
        ] {
            let stored = status_meter("claude", Some(&work()), state, code, None);
            let cached = normalize_meter(&stored, 1000);
            assert_eq!(cached.reason_code, Some(code));
            assert_eq!(cached.reason.as_deref(), Some(code.text()));
        }
    }
    #[test]
    fn codex_reset_credits_ride_on_each_window_and_expired_ones_drop_on_read() {
        let reply = json!({
            "rateLimits": {"primary": {"usedPercent": 5}, "secondary": {"usedPercent": 9}},
            "rateLimitResetCredits": {"availableCount": 3, "credits": [
                {"status": "available", "expiresAt": null},
                {"status": "available", "expiresAt": 2000},
                {"status": "redeemed", "expiresAt": 1500},
                {"status": "available", "expiresAt": 1200},
            ]},
        });
        let meters = normalize_limits(&work(), &reply, 1000).unwrap();
        let credits = ResetCredits {
            available: 3,
            expiry_time_seconds: Some(vec![Some(1200), Some(2000), None]),
        };
        assert!(
            meters
                .iter()
                .all(|meter| meter.reset_credits == Some(credits.clone()))
        );
        // One credit expired after the read: the count drops with it.
        assert_eq!(
            normalize_meter(&meters[0], 1300).reset_credits,
            Some(ResetCredits {
                available: 2,
                expiry_time_seconds: Some(vec![Some(2000), None]),
            })
        );
        // A count with no list keeps the count; no field at all is no credits object.
        let count_only = json!({"rateLimits": {"primary": {"usedPercent": 5}},
                                "rateLimitResetCredits": {"availableCount": 1, "credits": null}});
        assert_eq!(
            normalize_limits(&work(), &count_only, 1000).unwrap()[0].reset_credits,
            Some(ResetCredits {
                available: 1,
                expiry_time_seconds: None
            })
        );
        let none = json!({"rateLimits": {"primary": {"usedPercent": 5}}});
        assert_eq!(
            normalize_limits(&work(), &none, 1000).unwrap()[0].reset_credits,
            None
        );
        // A failed cached meter has no reading, so it carries no credits.
        let mut broken = meters[0].clone();
        broken.used_pct = Some(101);
        assert_eq!(normalize_meter(&broken, 1000).reset_credits, None);
        // An old cache line has no field and still reads.
        let old: UsageMeter = serde_json::from_value(json!({"provider":"codex","account":"w","label":"w",
            "window":"primary","window_minutes":300,"used_pct":5,"reset_time_seconds":null,"state":"fresh",
            "source":null,"reason":null,"reason_code":null,"as_of_seconds":1000})).unwrap();
        assert_eq!(old.reset_credits, None);
    }
}
