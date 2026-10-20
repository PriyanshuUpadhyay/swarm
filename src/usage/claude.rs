// Claude credential reads and usage window parsing.
use super::*;

pub(super) fn claude_keychain_service(secure_storage: Option<&str>) -> String {
    match secure_storage {
        None => "Claude Code-credentials".into(),
        Some(path) => {
            let hash = Sha256::digest(path.as_bytes());
            format!(
                "Claude Code-credentials-{:02x}{:02x}{:02x}{:02x}",
                hash[0], hash[1], hash[2], hash[3]
            )
        }
    }
}

pub(super) fn fresh_claude(
    account: &Account,
    deadline: Instant,
) -> Result<Vec<UsageMeter>, String> {
    if account.invalid_home() {
        return Ok(vec![status_meter(
            "claude",
            Some(account),
            "no_source",
            ReasonCode::NoSource,
            account.summary.as_deref(),
        )]);
    }
    let storage = crate::providers::secure_storage_dir(&account.home, &account.name);
    let service = claude_keychain_service(storage.as_deref());
    let security = std::env::var("SWARM_SECURITY_CMD").unwrap_or_else(|_| "security".into());
    let user = std::env::var("USER").unwrap_or_default();
    let credentials = read_json(
        &security,
        &["find-generic-password", "-s", &service, "-a", &user, "-w"],
        &BTreeMap::new(),
        deadline,
    )
    .map_err(|_| "Claude credentials are unavailable")?;
    let token = credentials
        .pointer("/claudeAiOauth/accessToken")
        .and_then(Value::as_str)
        .filter(|token| {
            !token.is_empty()
                && token.len() <= 8192
                && token.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .ok_or("Claude credentials are unavailable")?;
    // A missing or unreadable expiry counts as live, so the API answers for it.
    if credentials
        .pointer("/claudeAiOauth/expiresAt")
        .and_then(Value::as_i64)
        .is_some_and(|millis| millis < now_seconds().saturating_mul(1000))
    {
        return Ok(vec![status_meter(
            "claude",
            Some(account),
            "failed",
            ReasonCode::TokenExpired,
            None,
        )]);
    }
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or("Claude usage read timed out")?;
    let curl = std::env::var("SWARM_CURL_CMD").unwrap_or_else(|_| "curl".into());
    let url = std::env::var("SWARM_CLAUDE_USAGE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com/api/oauth/usage".into());
    let mut child = Command::new(curl)
        // Disable curlrc so it cannot enable redirects, tracing, or a different destination.
        .args([
            "-q",
            "--silent",
            "--max-time",
            &remaining.as_secs_f64().to_string(),
            "--write-out",
            "\n%{http_code}",
            "-H",
            "@-",
            "--url",
            &url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Claude usage request is unavailable")?;
    let sent = child.stdin.take().is_some_and(|mut stdin| {
        writeln!(
            stdin,
            "Authorization: Bearer {token}\nanthropic-beta: oauth-2025-04-20"
        )
        .is_ok()
    });
    if !sent {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Claude usage request failed".into());
    }
    let (response, status) = read_stdout(child, 1024 * 1024, deadline);
    if !status.is_some_and(|status| status.success()) {
        return Err("Claude usage request failed".into());
    }
    let bytes = response
        .map_err(|_| "Claude usage read timed out")?
        .map_err(|_| "Claude usage read failed")?;
    let split = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or("Claude usage response is invalid")?;
    match &bytes[split + 1..] {
        b"401" | b"403" => {
            return Ok(vec![status_meter(
                "claude",
                Some(account),
                "failed",
                ReasonCode::ReloginRequired,
                None,
            )]);
        }
        b"200" => (),
        _ => return Err("Claude usage request failed".into()),
    }
    let payload =
        serde_json::from_slice(&bytes[..split]).map_err(|_| "Claude usage response is invalid")?;
    claude_windows(account, &payload, now_seconds())
}

pub(super) fn claude_windows(
    account: &Account,
    payload: &Value,
    now: i64,
) -> Result<Vec<UsageMeter>, String> {
    let probe = |value: &Value, names: &[&str]| {
        names
            .iter()
            .find_map(|name| {
                value
                    .get(*name)
                    .filter(|value| !value.is_null() && **value != false)
            })
            .cloned()
            .unwrap_or(Value::Null)
    };
    let mut fable = probe(
        payload,
        &["fable", "seven_day_fable", "fable_seven_day", "fable_week"],
    );
    if fable.is_null()
        && let Some(limits) = payload["limits"].as_array()
    {
        fable = limits
            .iter()
            .find(|entry| {
                entry["kind"] == "weekly_scoped"
                    && entry
                        .pointer("/scope/model/display_name")
                        .and_then(Value::as_str)
                        == Some("Fable")
            })
            .cloned()
            .unwrap_or(Value::Null);
    }
    let mut meters = Vec::new();
    for (window, minutes, value) in [
        (
            "5h",
            300,
            probe(payload, &["five_hour", "fiveHour", "five_hour_window"]),
        ),
        (
            "7d",
            10080,
            probe(payload, &["seven_day", "sevenDay", "seven_day_window"]),
        ),
        ("fb", 10080, fable),
    ] {
        if value.is_null() {
            continue;
        }
        let percent = probe(
            &value,
            &["used_percentage", "utilization", "used_percent", "percent"],
        );
        let percent = percent
            .as_f64()
            .or_else(|| percent.as_str()?.parse().ok())
            .filter(|percent| percent.is_finite() && (0.0..=100.0).contains(percent));
        let reset = probe(&value, &["resets_at", "reset_at", "resets"]);
        let reset_time = if reset.is_null() {
            Some(None)
        } else {
            claude_reset_seconds(&reset).map(Some)
        };
        let mut meter = status_meter(
            "claude",
            Some(account),
            "failed",
            ReasonCode::ReadFailed,
            Some("Claude usage window is invalid"),
        );
        meter.window = Some(window.into());
        meter.window_minutes = Some(minutes);
        if let (Some(percent), Some(reset_time)) = (percent, reset_time) {
            meter.used_pct = Some(percent.round() as i64);
            meter.reset_time_seconds = reset_time;
            meter.as_of_seconds = Some(now);
            meter.state = "fresh".into();
            meter.clear_reason();
        }
        meters.push(meter);
    }
    if meters.is_empty() {
        return Err("Claude usage response has no usage window".into());
    }
    Ok(meters)
}

pub(super) fn claude_reset_seconds(value: &Value) -> Option<i64> {
    if let Some(seconds) = value.as_i64() {
        return (0..=MAX_RESET_SECONDS)
            .contains(&seconds)
            .then_some(seconds);
    }
    let text = value.as_str()?;
    if let Ok(seconds) = text.parse::<i64>() {
        return (0..=MAX_RESET_SECONDS)
            .contains(&seconds)
            .then_some(seconds);
    }
    let text = text
        .strip_suffix('Z')
        .or_else(|| text.strip_suffix("+00:00"))?;
    let (text, fraction) = text
        .split_once('.')
        .map_or((text, None), |(text, fraction)| (text, Some(fraction)));
    if !text.is_ascii()
        || text.len() != 19
        || fraction.is_some_and(|value| {
            value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return None;
    }
    if [4, 7, 10, 13, 16].map(|index| text.as_bytes()[index]) != *b"--T::" {
        return None;
    }
    let number = |start, end| text[start..end].parse::<i64>().ok();
    let (year, month, day, hour, minute, second) = (
        number(0, 4)?,
        number(5, 7)?,
        number(8, 10)?,
        number(11, 13)?,
        number(14, 16)?,
        number(17, 19)?,
    );
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if !(1970..=9999).contains(&year)
        || !(1..=days).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..60).contains(&second)
    {
        return None;
    }
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some((era * 146097 + day_of_era - 719468) * 86400 + hour * 3600 + minute * 60 + second)
}
