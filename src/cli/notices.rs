//! CLI notice health: whether macOS lets swarm's notices through, and the owner's way to fix it
//! (ADR 0102).

use super::*;

/// The bundle id in ui/Resources/SwarmNotify-Info.plist.
const BUNDLE: &str = "io.github.priyanshuupadhyay.swarm.notify";

/// The System Settings panes that `swarm notices settings` opens. Fixed here, so a caller names a
/// pane and never a URL.
const NOTIFICATIONS_URL: &str = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=io.github.priyanshuupadhyay.swarm.notify";
const FOCUS_URL: &str = "x-apple.systempreferences:com.apple.Focus-Settings.extension";

/// `swarm notices health|allow|test --json` and `swarm notices settings <notifications|focus>`.
pub(crate) fn notices_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let json = |rest: &[String]| -> Result<(), Box<dyn std::error::Error>> {
        match rest {
            [flag] if flag == "--json" => Ok(()),
            _ => Err(USAGE.into()),
        }
    };
    match args {
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
                bounded_output(command, std::time::Duration::from_secs(120));
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

/// The notice health: `state` is ok, allow, blocked, held, missing, or unknown, and more may come.
/// `action` names what the owner's button does, or is null.
pub(crate) fn health() -> serde_json::Value {
    let Some(helper) = helper_executable() else {
        return row("missing", "This build ships no notice helper", &[], None);
    };
    let mut command = std::process::Command::new(helper);
    command.arg("--status");
    let status = bounded_output(command, std::time::Duration::from_secs(5))
        .and_then(|out| serde_json::from_str::<serde_json::Value>(&out).ok())
        .ok_or_else(|| "Could not read the notice permission from the notice helper".to_string());
    judge(status, last_notice_held("1h"))
}

/// The rule, apart from macOS: the helper's `--status` answer (or why it is missing), then whether
/// a Focus held the newest notice. A failed read is `unknown`, never `ok`.
pub(crate) fn judge(
    status: Result<serde_json::Value, String>,
    held: Result<Option<bool>, String>,
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
        _ => {}
    }
    match held {
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
    let out = bounded_output(command, std::time::Duration::from_secs(15))
        .ok_or_else(|| "log show failed or took over 15 s".to_string())?;
    Ok(held_in(&out))
}

/// The outcome of the newest "Presenting" record of the helper: held when NotificationCenter
/// logged that ident as muted by DND after it. NotificationCenter logs old idents as muted again,
/// so only the newest presented ident counts.
pub(crate) fn held_in(log: &str) -> Option<bool> {
    let (at, line) = log
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("Presenting <NotificationRecord"))
        .last()?;
    let ident = line.split("ident:\"").nth(1)?.split('"').next()?;
    let muted = format!("{ident} ({BUNDLE}) muted by DND");
    Some(log.lines().skip(at).any(|line| line.contains(&muted)))
}

/// Adds a test notice, starts the helper, and waits up to 10 s for macOS to present it; then the
/// health of that notice.
fn send_test() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let connection = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    swarm::store::add_notice(
        &connection,
        None,
        None,
        "Swarm",
        "Test notice: swarm's notices reach you.",
    )?;
    start_notify_helper();
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_secs(1));
        // One second more than has passed, so the window starts just before the post.
        let window = format!("{}s", started.elapsed().as_secs() + 1);
        if let Ok(Some(_)) = last_notice_held(&window) {
            return Ok(health());
        }
    }
    Ok(row(
        "unknown",
        "No test notice reached macOS within 10 s. Check that Settings › Notices posts notices.",
        &[],
        None,
    ))
}

/// The helper's executable, which `swarm notices` runs directly for `--status` and `--authorize`.
fn helper_executable() -> Option<std::path::PathBuf> {
    notify_helper()
        .map(|app| app.join("Contents/MacOS/SwarmNotify"))
        .filter(|path| path.exists())
}

/// The stdout of `command`, or None when it fails, or runs past `timeout` and is killed.
fn bounded_output(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Option<String> {
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // The read runs on its own thread, so a full pipe cannot stall the deadline loop.
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut text).map(|_| text)
    });
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return reader.join().ok()?.ok(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
#[path = "notices/tests.rs"]
mod tests;
