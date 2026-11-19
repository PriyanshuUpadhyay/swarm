//! The supported Pi CLI and extension API series.

use std::{process::Command, time::Duration};

const SERIES_PREFIX: &str = "1.5.";

pub(crate) fn state() -> (&'static str, String) {
    let mut command = Command::new("pi");
    command.arg("--version");
    let version = swarm::process::bounded_output(command, Duration::from_secs(3));
    let version = version.as_deref().map(str::trim).filter(|version| {
        let mut parts = version.split('.');
        let numeric = |part: Option<&str>| {
            part.is_some_and(|part| {
                !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
            })
        };
        numeric(parts.next())
            && numeric(parts.next())
            && numeric(parts.next())
            && parts.next().is_none()
    });
    let supported = format!("{SERIES_PREFIX}x");
    match version {
        Some(version) if version.starts_with(SERIES_PREFIX) => {
            ("ok", format!("Pi {version} (supported: {supported})"))
        }
        Some(version) => (
            "broken",
            format!("Pi {version} is outside the supported range {supported}; launch continues"),
        ),
        None => (
            "broken",
            format!(
                "Could not read the Pi version; supported range is {supported}; launch continues"
            ),
        ),
    }
}
