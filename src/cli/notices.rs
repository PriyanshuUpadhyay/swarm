//! CLI notice health: whether macOS lets swarm's notices through, and the owner's way to fix it
//! (ADR 0102).

use super::*;

/// The bundle id in ui/Resources/SwarmNotify-Info.plist.
const BUNDLE: &str = "io.github.priyanshuupadhyay.swarm.notify";

/// The System Settings panes that `swarm notices settings` opens. Fixed here, so a caller names a
/// pane and never a URL.
const NOTIFICATIONS_URL: &str = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=io.github.priyanshuupadhyay.swarm.notify";
const FOCUS_URL: &str = "x-apple.systempreferences:com.apple.Focus-Settings.extension";

/// The title of `swarm notices test`'s notice.
const TEST_NOTICE_TITLE: &str = "Swarm test";

/// `swarm notices health|allow|test --json`, `swarm notices settings <notifications|focus>`, and
/// the HUD's `swarm notices --inbox --json` and `swarm notices seen` (ADR 0147).
pub(crate) fn notices_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let json = |rest: &[String]| -> Result<(), Box<dyn std::error::Error>> {
        match rest {
            [flag] if flag == "--json" => Ok(()),
            _ => Err(USAGE.into()),
        }
    };

    match args {
        [flag, rest @ ..] if flag == "--inbox" => {
            json(rest)?;
            let root = swarm::paths::root_dir()?;
            let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
            print_json(&inbox(&root, &connection)?)
        }

        [verb, rest @ ..] if verb == "seen" => {
            let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
            // Clear names the chats it shows and its newest row, so a row that came after the HUD
            // drew stays unseen.
            let (through, rest) = match rest {
                [flag, id, rest @ ..] if flag == "--through" => (Some(id.parse::<i64>()?), rest),
                _ => (None, rest),
            };

            let sessions: Vec<&str> = rest.iter().map(String::as_str).collect();
            if sessions.is_empty() || sessions.iter().any(|session| session.starts_with('-')) {
                return Err(USAGE.into());
            }
            swarm::store::mark_notices_seen(&connection, &sessions, through)?;
            Ok(())
        }

        [verb, rest @ ..] if verb == "health" => {
            json(rest)?;
            print_json(&health())
        }

        [verb, rest @ ..] if verb == "allow" => {
            json(rest)?;
            // The macOS prompt waits for the owner, so this gives them two minutes.
            if let Some(helper) = helper_executable() {
                let mut command = std::process::Command::new(helper);
                command.arg("--authorize");
                if swarm::process::bounded_output(command, std::time::Duration::from_secs(120))
                    .is_none()
                {
                    return print_json(&row(
                        "unknown",
                        "The notice helper did not answer the permission request within 2 minutes",
                        &[],
                        None,
                    ));
                }
            }
            print_json(&health())
        }

        [verb, rest @ ..] if verb == "test" => {
            json(rest)?;
            print_json(&send_test()?)
        }

        [verb, pane] if verb == "settings" => {
            let url = match pane.as_str() {
                "notifications" => NOTIFICATIONS_URL,
                "focus" => FOCUS_URL,
                _ => return Err(USAGE.into()),
            };
            let status = std::process::Command::new("open").arg(url).status()?;
            if !status.success() {
                return Err(format!("swarm: could not open System Settings ({status})").into());
            }
            Ok(())
        }

        _ => Err(USAGE.into()),
    }
}

/// At most this many waiting agents get a screen read for the HUD's cards.
const INBOX_NEEDS: usize = 3;
/// At most this many unseen chats show in the HUD's "Since you looked" list.
const INBOX_ROWS: usize = 20;

/// ADR 0147: the HUD's inbox. `needs` holds each waiting agent with the prompt on its screen, whose
/// id `swarm answer` accepts; `rows` holds the newest unseen result of each chat. Each list is cut,
/// so `needs_total` and `rows_total` give the full counts.
pub(crate) fn inbox(
    root: &std::path::Path,
    connection: &rusqlite::Connection,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut waiting = Vec::new();
    for agent in swarm::store::running_agents(connection)? {
        if agent.state == "blocked" && pane_open(root, connection, &agent.session, &agent.id)? {
            waiting.push(agent);
        }
    }
    let needs_total = waiting.len();
    let mut needs = Vec::new();
    for waiting in waiting.into_iter().take(INBOX_NEEDS) {
        let (session, agent) = (waiting.session.as_str(), waiting.id.as_str());
        let row = swarm::store::agents(connection, session)?
            .into_iter()
            .find(|row| row.id == agent);
        let log = row.as_ref().and_then(|row| row.log.clone());
        let adapter = swarm::store::adapter_of(connection, session)?;

        // A failed screen read keeps the card with Open only.
        let prompt = match (&adapter, row.as_ref().and_then(|row| row.pane.as_deref())) {
            (Some(adapter), Some(pane)) => swarm::adapter::load(root, adapter)
                .ok()
                .and_then(|loaded| {
                    read_screen(&loaded, pane, Some(&waiting.provider), log.as_deref())
                })
                .and_then(|(_, _, prompt, _)| prompt),
            _ => None,
        };

        let prompt = match prompt {
            Some(mut prompt) => {
                if waiting.provider == "claude" {
                    tag_permission_prompt(root, session, agent, &mut prompt)?;
                }
                let mut card = serde_json::json!({
                    "id": prompt.id,
                    "question": prompt.question,
                    "choices": prompt.choices,
                });
                if waiting.provider == "pi" && prompt.is_pi_plan() {
                    card["kind"] = "plan".into();
                }
                Some(card)
            }
            None => None,
        };

        needs.push(serde_json::json!({
            "session": session,
            "agent": agent,
            "adapter": adapter,
            "title": chat_title(connection, session, agent, log.as_deref())?,
            "kind": row.and_then(|row| row.state_kind),
            "prompt": prompt,
        }));
    }

    let (rows, rows_total) = swarm::store::notice_inbox(connection, INBOX_ROWS)?;
    Ok(serde_json::json!({
        "schema": "swarm.inbox.v1",
        "needs": needs,
        "needs_total": needs_total,
        "rows": rows,
        "rows_total": rows_total,
    }))
}

/// A wait lasts while its pane is open (ADR 0147). No sweep covers a chair, so a chair whose pane
/// closed during a wait stays `blocked`; this forgets its pane. A pane that cannot be checked
/// counts as open, so a failed listing never drops a real wait.
fn pane_open(
    root: &std::path::Path,
    connection: &rusqlite::Connection,
    session: &str,
    agent: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (Some(adapter), Some(pane)) = (
        swarm::store::adapter_of(connection, session)?,
        swarm::store::pane_of(connection, session, agent)?,
    ) else {
        return Ok(true);
    };
    let gone = swarm::adapter::load(root, &adapter)
        .and_then(|adapter| adapter.has_pane(&pane))
        .is_ok_and(|open| !open);
    if gone {
        swarm::store::clear_pane(connection, session, agent)?;
    }
    Ok(!gone)
}

/// The notice health: `state` is ok, allow, blocked, held, missing, or unknown, and more may come.
/// `action` names what the owner's button does, or is null.
pub(crate) fn health() -> serde_json::Value {
    let Some(helper) = helper_executable() else {
        return row("missing", "This build ships no notice helper", &[], None);
    };
    let mut command = std::process::Command::new(helper);
    command.arg("--status");
    let status = swarm::process::bounded_output(command, std::time::Duration::from_secs(5))
        .and_then(|out| serde_json::from_str::<serde_json::Value>(&out).ok())
        .ok_or_else(|| "Could not read the notice permission from the notice helper".to_string());
    judge(status, || last_notice_held("1h"))
}

/// The rule, apart from macOS: the helper's `--status` answer (or why it is missing), then whether
/// a Focus held the newest notice. A failed read is `unknown`, never `ok`.
/// `held` reads the log only when the permission is fine, so the read stays within 20 s.
pub(crate) fn judge(
    status: Result<serde_json::Value, String>,
    held: impl FnOnce() -> Result<Option<bool>, String>,
) -> serde_json::Value {
    let status = match status {
        Ok(status) => status,
        Err(why) => return row("unknown", &why, &[], None),
    };
    let settings_steps = [
        "Click Open Notification settings.",
        "Turn on Allow notifications for Swarm Notices.",
        "Pick Banners or Alerts.",
        "Come back here. Swarm checks again.",
    ];
    match (status["authorization"].as_str(), status["style"].as_str()) {
        (Some("notDetermined"), _) => {
            return row(
                "allow",
                "macOS has not asked you yet to allow swarm's notices.",
                &["Click Allow notices.", "In the macOS prompt, click Allow."],
                Some("allow"),
            );
        }
        (Some("denied"), _) => {
            return row(
                "blocked",
                "Notices from Swarm Notices are off in System Settings.",
                &settings_steps,
                Some("notifications"),
            );
        }
        (_, Some("none")) => {
            return row(
                "blocked",
                "Swarm Notices may post, but its alert style is None, so no banner shows.",
                &settings_steps,
                Some("notifications"),
            );
        }
        (Some("authorized" | "provisional" | "ephemeral"), Some("banner" | "alert")) => {}
        // A value this code does not know is a failed read, never `ok`.
        (authorization, style) => {
            return row(
                "unknown",
                &format!(
                    "The notice helper gave a permission that swarm does not know ({}, {})",
                    authorization.unwrap_or("none"),
                    style.unwrap_or("none")
                ),
                &[],
                None,
            );
        }
    }
    match held() {
        Ok(Some(true)) => row(
            "held",
            "A Focus, such as Do Not Disturb, held swarm's last notice.",
            &[
                "Click Open Focus settings.",
                "Pick the Focus that is on, such as Do Not Disturb.",
                "Under Allowed Notifications, add Swarm Notices to the allowed apps.",
                "Or turn the Focus off in Control Center.",
                "Come back here and click Send a test notice.",
            ],
            Some("focus"),
        ),
        Ok(_) => row("ok", "Swarm's notices reach you", &[], None),
        Err(why) => row(
            "unknown",
            &format!("Could not read the notice log, so a Focus hold is unknown: {why}"),
            &[],
            None,
        ),
    }
}

/// One health answer.
fn row(state: &str, reason: &str, steps: &[&str], action: Option<&str>) -> serde_json::Value {
    serde_json::json!({"state": state, "reason": reason, "steps": steps, "action": action})
}

/// Whether a Focus held the newest swarm notice in the last `window` of the unified log, or None
/// when no notice was posted in it.
fn last_notice_held(window: &str) -> Result<Option<bool>, String> {
    let predicate = format!(
        r#"(process == "usernoted" AND eventMessage BEGINSWITH "Presenting <NotificationRecord app:\"{BUNDLE}\"") OR (process == "NotificationCenter" AND eventMessage CONTAINS "({BUNDLE}) muted by DND")"#
    );
    let mut command = std::process::Command::new("/usr/bin/log");
    command.args([
        "show",
        "--last",
        window,
        "--style",
        "compact",
        "--predicate",
        &predicate,
    ]);
    let out = swarm::process::bounded_output(command, std::time::Duration::from_secs(15))
        .ok_or_else(|| "log show failed or took over 15 s".to_string())?;
    held_in(&out)
}

/// The outcome of the newest "Presenting" record of the helper: held when NotificationCenter
/// logged that ident as muted by DND after it. NotificationCenter logs old idents as muted again,
/// so only the newest presented ident counts. None when no notice was presented; an error when the
/// record has no ident that this reads, so a changed log text gives `unknown`, not `ok`.
pub(crate) fn held_in(log: &str) -> Result<Option<bool>, String> {
    let Some((at, line)) = log
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("Presenting <NotificationRecord"))
        .last()
    else {
        return Ok(None);
    };
    let ident = line
        .split("ident:\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or("the notice log has a record with no ident")?;
    let muted = format!("{ident} ({BUNDLE}) muted by DND");
    Ok(Some(log.lines().skip(at).any(|line| line.contains(&muted))))
}

/// Adds a test notice, starts the helper, and waits up to 10 s for macOS to present it; then the
/// health of that notice.
fn send_test() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    swarm::store::add_notice(
        &connection,
        None,
        None,
        Some(swarm::store::notice_kind::TEST),
        TEST_NOTICE_TITLE,
        "Test notice: swarm's notices reach you.",
    )?;
    start_notify_helper();
    let started = std::time::Instant::now();
    let mut failed = None;
    while started.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_secs(1));
        // One second more than has passed, so the window starts just before the post.
        let window = format!("{}s", started.elapsed().as_secs() + 1);
        match last_notice_held(&window) {
            Ok(Some(_)) => {
                // NotificationCenter logs a Focus hold just after the post, so wait for it.
                std::thread::sleep(std::time::Duration::from_secs(2));
                return Ok(health());
            }
            Ok(None) => {}
            Err(why) => failed = Some(why),
        }
    }
    let reason = match failed {
        Some(why) => format!("Could not read the notice log, so the test result is unknown: {why}"),
        None => {
            "No test notice reached macOS within 10 s. Check that Settings › Notices posts notices."
                .into()
        }
    };
    Ok(row("unknown", &reason, &[], None))
}

/// The helper's executable, which `swarm notices` runs directly for `--status` and `--authorize`.
fn helper_executable() -> Option<std::path::PathBuf> {
    notify_helper()
        .map(|app| app.join("Contents/MacOS/SwarmNotify"))
        .filter(|path| path.exists())
}

#[cfg(test)]
#[path = "notices/tests.rs"]
mod tests;
