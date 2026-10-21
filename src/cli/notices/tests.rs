use super::*;

fn status(authorization: &str, style: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"authorization": authorization, "style": style}))
}

#[test]
fn judge_names_the_fix_for_each_state() {
    let cases = [
        (
            status("notDetermined", "banner"),
            Ok(None),
            "allow",
            Some("allow"),
        ),
        (
            status("denied", "banner"),
            Ok(Some(true)),
            "blocked",
            Some("notifications"),
        ),
        (
            status("authorized", "none"),
            Ok(None),
            "blocked",
            Some("notifications"),
        ),
        (
            status("authorized", "banner"),
            Ok(Some(true)),
            "held",
            Some("focus"),
        ),
        (status("authorized", "alert"), Ok(Some(false)), "ok", None),
        (status("provisional", "banner"), Ok(None), "ok", None),
        // A failed read, or a value this code does not know, never reads as ok.
        (Err("no answer".into()), Ok(Some(false)), "unknown", None),
        (status("unknown", "banner"), Ok(None), "unknown", None),
        (Ok(serde_json::json!({})), Ok(None), "unknown", None),
        (
            status("authorized", "banner"),
            Err("log failed".into()),
            "unknown",
            None,
        ),
    ];
    for (status, held, state, action) in cases {
        let health = judge(status, held);
        assert_eq!(health["state"], state, "{health}");
        assert_eq!(health["action"].as_str(), action, "{health}");
    }
}

#[test]
fn held_in_reads_only_the_newest_presented_notice() {
    let presented = |ident: &str| {
        format!(
            "usernoted Presenting <NotificationRecord app:\"{BUNDLE}\" ident:\"{ident}\" req:\"X\">"
        )
    };
    let muted = |ident: &str| {
        format!("NotificationCenter {ident} ({BUNDLE}) muted by DND suppression: delay")
    };
    assert_eq!(held_in(""), Ok(None));
    assert_eq!(
        held_in(&[presented("old"), muted("old")].join("\n")),
        Ok(Some(true))
    );
    // NotificationCenter mutes the old notice again; the newest notice was shown.
    let log = [
        presented("old"),
        muted("old"),
        presented("newest"),
        muted("old"),
    ]
    .join("\n");
    assert_eq!(held_in(&log), Ok(Some(false)));
    // A record whose ident this cannot read is an error, so the health is unknown, not ok.
    assert!(held_in("usernoted Presenting <NotificationRecord app:\"x\">").is_err());
}

#[test]
fn the_settings_link_names_the_helper() {
    assert!(NOTIFICATIONS_URL.ends_with(BUNDLE));
}
