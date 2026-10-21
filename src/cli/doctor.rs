//! CLI doctor.

use super::*;

/// The schema tag of `swarm doctor --json`, which `/api/doctor` checks.
pub(crate) const DOCTOR_SCHEMA: &str = "swarm.doctor.v1";

/// One `swarm.doctor.v1` row. `group` is the `swarm setup` item that fixes it, or None.
pub(crate) fn doctor_row(
    id: &str,
    title: &str,
    (state, reason): (&str, String),
    group: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({"id": id, "title": title, "state": state, "reason": reason, "group": group})
}

/// The rows of what `swarm setup` sets up. A row is `ok`, `setup` when `swarm setup <group>` can
/// fix it, or `broken` when the owner must act, with the reason.
pub(crate) fn setup_rows(
    home: &std::path::Path,
    files: &HookFiles,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let ok = |reason: &str| ("ok", reason.to_string());
    let setup = |reason: &str| ("setup", reason.to_string());
    let kit = swarm::paths::bundled("skills/kit").and_then(|kit| std::fs::canonicalize(kit).ok());
    let hud = swarm::hud::status(home);
    let mut rows = vec![
        doctor_row(
            "hooks",
            "Agent hooks",
            if files.hooks_status() {
                ok("Claude, Codex and AGY report to swarm")
            } else {
                setup("The agent CLIs do not report to swarm yet")
            },
            Some("hooks"),
        ),
        doctor_row(
            "guard",
            "Command guard",
            if files.guard_status(swarm::paths::guards_file()?.exists()) {
                ok("Agents ask before a guarded command")
            } else {
                setup("The guard hook is not in place")
            },
            Some("hooks"),
        ),
        doctor_row(
            "trust",
            "Launch consent",
            // Read here, not through trust_answer, so a file that cannot be read shows as broken.
            match read_consent(&swarm::paths::consent_file()?) {
                Err(error) => ("broken", error),
                Ok((_, value)) => {
                    match [STANDING, ASK].into_iter().find(|a| value[TRUST_KEY] == *a) {
                        Some(answer) => ("ok", format!("Answered: {answer}")),
                        None => setup("Not answered, so a launch asks for each folder"),
                    }
                }
            },
            Some("trust"),
        ),
        doctor_row(
            "plugin",
            "Plugin swarm",
            match kit {
                None => ("broken", "This build ships no kit".into()),
                Some(kit)
                    if std::fs::read_link(swarm::paths::root_dir()?.join("kit")).ok()
                        == Some(swarm::plugin::stable_kit(&kit)) =>
                {
                    ok("Installed from this build's kit")
                }
                Some(_) => setup("Not installed from this build's kit"),
            },
            Some("plugin"),
        ),
        doctor_row(
            "hud",
            "Usage HUD",
            match (hud.app.is_some(), hud.installed, hud.running) {
                (false, ..) => ("broken", "This build ships no HUD app".into()),
                (_, true, true) if !hud.strays.is_empty() => setup(&format!(
                    "Runs, with {} more {} that the login item did not start",
                    hud.strays.len(),
                    if hud.strays.len() == 1 {
                        "copy"
                    } else {
                        "copies"
                    }
                )),
                (_, true, true) => ok("Starts at login and runs"),
                (_, true, false) => ("broken", "Starts at login but does not run".into()),
                (_, false, _) => setup("Off"),
            },
            Some("hud"),
        ),
    ];
    for (index, copy) in stale_plugin_copies(home).iter().enumerate() {
        rows.push(doctor_row(
            &format!("plugin-{index}"),
            &format!("Plugin swarm in {}", copy.label()),
            ("setup", copy.reason()),
            Some("plugin"),
        ));
    }
    Ok(rows)
}

pub(crate) fn stale_plugin_copies(home: &std::path::Path) -> Vec<swarm::plugin::StaleCopy> {
    let Some(kit) = swarm::paths::bundled("skills/kit") else {
        return Vec::new();
    };
    let claude = env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    let Ok(swarm_home) = swarm::paths::home() else {
        return Vec::new();
    };
    let link = std::path::PathBuf::from(swarm_home).join(".swarm/kit");
    let found: Vec<&str> = swarm::plugin::CLIS
        .into_iter()
        .filter(|cli| installed(cli))
        .collect();
    swarm::plugin::stale_copies(
        &kit,
        &link,
        &found,
        &claude,
        &codex_homes(home).unwrap_or_default(),
        home,
    )
}

/// `swarm doctor [--json]`, also `swarm setup [--json]`: the rows of `setup_rows`, then each thing
/// swarm ships or runs. Once it can read HOME and the swarm home it exits 0, so the page can show
/// every row.
pub(crate) fn doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let json = match args {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => return Err("usage: swarm doctor [--json]".into()),
    };
    let home = std::path::PathBuf::from(env_var("HOME")?);
    let mut items = setup_rows(&home, &HookFiles::of(&home)?)?;
    items.push(doctor_row(
        "transcript",
        "Transcript tool",
        if env::var_os("SWARM_TRANSCRIPT_TOOL").is_some()
            || swarm::paths::bundled("transcript").is_some()
        {
            ("ok", "Chats can show their rows".into())
        } else {
            (
                "broken",
                "This build ships no transcript tool, so chats show no rows".into(),
            )
        },
        None,
    ));
    items.push(doctor_row("notices", "Notices", notices_state(), None));
    #[cfg(feature = "serve")]
    items.push(doctor_row("ui", "UI server", serve::ui_state(), None));
    if json {
        return print_json(&serde_json::json!({"schema": DOCTOR_SCHEMA, "items": items}));
    }
    for item in &items {
        println!(
            "{:<6} {}: {}",
            item["state"].as_str().unwrap_or_default(),
            item["title"].as_str().unwrap_or_default(),
            item["reason"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

/// The items whose row reads `setup`, in apply order: what `swarm setup all` sets up.
pub(crate) fn needed_items() -> Result<Vec<&'static str>, Box<dyn std::error::Error>> {
    let home = std::path::PathBuf::from(env_var("HOME")?);
    let rows = setup_rows(&home, &HookFiles::of(&home)?)?;
    Ok(SETUP_ITEMS
        .into_iter()
        .filter(|item| {
            rows.iter()
                .any(|row| row["group"] == *item && row["state"] == "setup")
        })
        .collect())
}

/// The notices row: the notice health that the app also shows (ADR 0102).
pub(crate) fn notices_state() -> (&'static str, String) {
    let health = health();
    let state = if health["state"] == "ok" {
        "ok"
    } else {
        "broken"
    };
    (
        state,
        health["reason"].as_str().unwrap_or_default().to_string(),
    )
}

/// The plugin item: install the kit that ships beside `swarm` as the plugin `swarm` in each of
/// Claude, Codex, AGY and pi on PATH (ADR 0090). The commands go through each CLI's own plugin
/// command, so swarm records no managed edit for them.
/// A skill in `leaving` is a link this plan removes, so it is not named as the owner's own.
pub(crate) fn plugin_plan(
    user_home: &std::path::Path,
    leaving: &[std::path::PathBuf],
) -> Result<PluginPlan, Box<dyn std::error::Error>> {
    let kit = swarm::paths::bundled("skills/kit")
        .and_then(|kit| std::fs::canonicalize(kit).ok())
        .ok_or("swarm: this build ships no kit in libexec/skills/kit")?;
    if !kit.join(".claude-plugin/marketplace.json").is_file() {
        return Err(format!(
            "swarm: {} is not the swarm kit: it has no .claude-plugin/marketplace.json",
            kit.display()
        )
        .into());
    }
    let link = swarm::paths::root_dir()?.join("kit");
    let found: Vec<&str> = swarm::plugin::CLIS
        .into_iter()
        .filter(|cli| installed(cli))
        .collect();
    let mut codex = Vec::new();
    if found.contains(&"codex") {
        for home in codex_homes(user_home)? {
            if !home.is_dir() {
                continue;
            }
            let listed = std::process::Command::new("codex")
                .args(["plugin", "marketplace", "list", "--json"])
                .env("CODEX_HOME", &home)
                .output()?;
            codex.push(swarm::plugin::CodexHome {
                swarm_root: swarm::plugin::codex_swarm_root(&listed.stdout),
                home,
            });
        }
    }
    let steps = swarm::plugin::steps(&link, &kit, &found, &codex);
    if steps.is_empty() {
        return Err(
            "swarm: none of claude, codex, agy or pi is on PATH; nothing to install".into(),
        );
    }
    let target = swarm::plugin::stable_kit(&kit);
    let mut notes: Vec<String> = swarm::plugin::CLIS
        .iter()
        .filter(|cli| !found.contains(cli))
        .map(|cli| format!("{cli} is not on PATH"))
        .collect();
    let active: Vec<std::path::PathBuf> = [
        ("CLAUDE_CONFIG_DIR", "skills"),
        ("PI_CODING_AGENT_DIR", "skills"),
    ]
    .iter()
    .filter_map(|(variable, folder)| {
        env::var_os(variable)
            .map(std::path::PathBuf::from)
            .filter(|dir| dir.is_absolute() && dir.is_dir())
            .map(|dir| dir.join(folder))
    })
    .collect();
    let clashes = swarm::plugin::clashes(&kit, user_home, &active);
    notes.extend(
        clashes
            .iter()
            .filter(|clash| !leaving.contains(clash))
            .map(|clash| {
                format!(
                    "{} is your own skill; it stays, and the CLI lists it beside the plugin's",
                    clash.display()
                )
            }),
    );
    Ok(PluginPlan {
        relink: std::fs::read_link(&link).ok().as_ref() != Some(&target),
        link,
        target,
        steps,
        notes,
    })
}

/// The plans that remove each skill link an older swarm's `skills` setup made and recorded.
pub(crate) fn old_skill_links(
    files: &HookFiles,
) -> Result<Vec<swarm::managed::Plan>, Box<dyn std::error::Error>> {
    use swarm::managed::{State, Target, Writer};
    let database = swarm::paths::sqlite_db()?;
    if !database.try_exists()? {
        return Ok(Vec::new());
    }
    // Read only, so a plan creates no database and changes no schema.
    let store = rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let found = files.items();
    let ids: Vec<String> = swarm::managed::list(&store, &found)?
        .into_iter()
        .filter(|entry| entry.edit.writer == Writer::Skills && entry.state == State::Present)
        .map(|entry| entry.edit.id())
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(swarm::managed::revert_plan(
        &store,
        &found,
        &Target::Ids(ids),
    )?)
}
