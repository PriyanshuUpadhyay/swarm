//! CLI setup.

use super::*;

/// The key of `~/.swarm/consent.json` that holds the owner's answer for launch folder trust, and
/// its two answers (ADR 0043). Reader and writer share these, so a typo cannot read as `ask`.
pub(crate) const TRUST_KEY: &str = "trust";
pub(crate) const STANDING: &str = "standing";
pub(crate) const ASK: &str = "ask";

/// `~/.swarm/consent.json` as its text ("" for a missing file) and its object.
pub(crate) fn read_consent(path: &std::path::Path) -> Result<(String, serde_json::Value), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((String::new(), serde_json::json!({})));
        }
        Err(error) => return Err(format!("swarm: cannot read {}: {error}", path.display())),
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) if value.is_object() => Ok((text, value)),
        _ => Err(format!("swarm: {} is not a JSON object", path.display())),
    }
}

/// The plan that sets the launch consent in `~/.swarm/consent.json` to `answer`, as a managed
/// edit, so Managed Changes lists it and its undo puts back the answer before. With none before,
/// the undo records `ask`, so the owner stays answered and the app asks once (ADR 0043); a first
/// `ask` itself undoes to none, because an edit that wrote its own before value never reads as
/// set back. A file swarm cannot read is a conflict.
pub(crate) fn consent_plan(
    answer: &str,
) -> Result<swarm::managed::FilePlan, Box<dyn std::error::Error>> {
    use swarm::managed::{Edit, FilePlan, Kind, Writer};
    let path = swarm::paths::consent_file()?;
    let (before, mut value) = match read_consent(&path) {
        Ok(read) => read,
        Err(error) => return Ok(FilePlan::unreadable(path, error)),
    };
    let found = value.get(TRUST_KEY).cloned();
    let mut plan = FilePlan {
        after: before.clone(),
        path,
        before,
        conflicts: Vec::new(),
        edits: Vec::new(),
    };
    if found.as_ref().and_then(serde_json::Value::as_str) != Some(answer) {
        value[TRUST_KEY] = answer.into();
        plan.after = serde_json::to_string_pretty(&value)? + "\n";
        plan.edits.push(Edit {
            before: found
                .or_else(|| Some(ASK.into()))
                .filter(|before| *before != answer),
            ..Edit::new(
                Writer::LaunchTrust,
                &plan.path,
                Kind::JsonKey,
                &[TRUST_KEY],
                answer.into(),
            )
        });
    }
    Ok(plan)
}

/// `swarm setup [--json] | status --json | <items> [--plan [--json] | --digest <digest> | --yes]
/// [--cwd <dir>] [--consent <standing|ask>] [--resume]` (ADR 0092). With no item it lists each
/// row and writes nothing. `<items>` is a comma list of `SETUP_ITEMS` or `all`, the items that
/// need setup. Every write is one plan with one digest (ADR 0043): `hooks`, as the hook files;
/// `trust`, the launch consent (`--consent`, by default the recorded answer, else standing) and the
/// trust entries a launch in `--cwd` would write, for a resuming seat with `--resume`; `plugin`,
/// the old skill links and each CLI's install command; `hud`, the LaunchAgent. A terminal run shows
/// the plan and asks; `--yes` or the plan's digest is the owner's consent, so a child pane may plan
/// but not apply.
pub(crate) fn setup(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => {
            doctor(&[])?;
            let needed = needed_items()?;
            if needed.is_empty() {
                println!("\nNothing to set up.");
            } else {
                println!(
                    "\nSet up:    swarm setup {}   (or swarm setup all)",
                    needed.join(",")
                );
            }
            println!(
                "Items:     {}\nPlan only: swarm setup <item> --plan",
                SETUP_ITEMS.join(", ")
            );
            return Ok(());
        }
        ["--json"] => return doctor(&["--json".into()]),
        ["status", "--json"] => {
            let files = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?;
            let has_list = swarm::paths::guards_file()?.exists();
            return print_json(&serde_json::json!({
                "hooks": files.hooks_status(),
                "guard": files.guard_status(has_list),
                // The owner answered, standing or ask, so the app does not ask again.
                "trust": trust_answer().is_some(),
            }));
        }
        _ => {}
    }
    if args.contains(&"--only") {
        return Err(
            "swarm: `--only` is gone; name the items, as in `swarm setup hooks,trust`".into(),
        );
    }
    let [list, flags @ ..] = args.as_slice() else {
        return Err(USAGE.into());
    };
    let (mut plan, mut json, mut yes, mut digest, mut cwd) = (false, false, false, None, None);
    let (mut consent, mut resume) = (None, false);
    let mut rest = flags.iter().copied();
    while let Some(arg) = rest.next() {
        match arg {
            "--plan" => plan = true,
            "--json" => json = true,
            "--yes" => yes = true,
            "--resume" => resume = true,
            "--digest" | "--cwd" | "--consent" => {
                let value = Some(rest.next().ok_or(USAGE)?);
                match arg {
                    "--digest" => digest = value,
                    "--cwd" => cwd = value,
                    _ => consent = value,
                }
            }
            _ => return Err(USAGE.into()),
        }
    }
    if list.starts_with('-')
        || (json && !plan)
        || [plan, yes, digest.is_some()]
            .iter()
            .filter(|set| **set)
            .count()
            > 1
        || consent.is_some_and(|answer| ![STANDING, ASK].contains(&answer))
    {
        return Err(USAGE.into());
    }
    let groups: Vec<&str> = if *list == "all" {
        needed_items()?
    } else {
        let named: Vec<&str> = list.split(',').collect();
        if let Some(unknown) = named.iter().find(|item| !SETUP_ITEMS.contains(item)) {
            return Err(format!(
                "swarm: unknown setup item {unknown}; items: {}, or all",
                SETUP_ITEMS.join(", ")
            )
            .into());
        }
        SETUP_ITEMS
            .into_iter()
            .filter(|item| named.contains(item))
            .collect()
    };
    if groups.is_empty() && !plan {
        println!("swarm: everything is set up. Nothing to do.");
        return Ok(());
    }
    // No --consent keeps the owner's answer, so the approve command a seat prints under `ask`
    // approves the folder and not standing consent; with no answer yet, setup offers standing.
    // An apply reads it under the trust lock, so an answer recorded while it waits is kept (L-4).
    let answer = || consent.unwrap_or_else(|| trust_answer().unwrap_or(STANDING));
    let dir = cwd.map_or_else(env::current_dir, |dir| Ok(dir.into()))?;
    let dir = std::fs::canonicalize(&dir)
        .map_err(|error| format!("swarm: bad --cwd {}: {error}", dir.display()))?;
    let refusal = swarm::bus::setup_refusal(
        env::var("SWARM_AGENT_ID").ok().as_deref(),
        env::var("HERDR_AGENT_PANE").ok().as_deref(),
    );
    if digest.is_none() {
        let setup = SetupPlan::of(&dir, &groups, answer(), resume)?;
        let plan_digest = setup.digest();
        if json {
            let group_of = |value: serde_json::Value, group: &str| {
                let mut value = value;
                value["group"] = group.into();
                value
            };
            let mut files: Vec<_> = setup
                .grouped()
                .filter(|(_, plan)| plan.changes())
                .map(|(group, plan)| {
                    serde_json::json!({
                        "group": group,
                        "path": plan.path().to_string_lossy(),
                        "diff": swarm::managed::diff(plan),
                    })
                })
                .collect();
            if let Some(diff) = Some(setup.hud_diff()).filter(|diff| !diff.is_empty()) {
                let home = std::path::PathBuf::from(env_var("HOME")?);
                files.push(serde_json::json!({
                    "group": "hud",
                    "path": swarm::hud::plist_path(&home).to_string_lossy(),
                    "diff": diff,
                }));
            }
            return print_json(&serde_json::json!({
                "digest": plan_digest,
                "consent": answer(),
                "items": groups,
                "files": files,
                "commands": setup.commands().iter()
                    .map(|(group, line)| serde_json::json!({"group": group, "line": line}))
                    .collect::<Vec<_>>(),
                "conflicts": setup.grouped()
                    .flat_map(|(group, plan)| plan.conflicts().iter().map(move |conflict| (group, conflict)))
                    .map(|(group, conflict)| group_of(serde_json::json!(conflict), group))
                    .collect::<Vec<_>>(),
                "skipped": setup.skipped.iter()
                    .map(|(group, reason)| serde_json::json!({"group": group, "reason": reason}))
                    .collect::<Vec<_>>(),
            }));
        }
        // The resolved items, not `all`, so the digest still names the plan shown.
        let mut apply = format!("swarm setup {} --digest {plan_digest}", groups.join(","));
        if let Some(cwd) = cwd {
            apply += &format!(" --cwd {}", swarm::adapter::shell_line(&[cwd.into()]));
        }
        if let Some(consent) = consent {
            apply += &format!(" --consent {consent}");
        }
        if resume {
            apply += " --resume";
        }
        if !plan && let Some(conflicts) = swarm::managed::conflicts_text(&setup.plans) {
            return Err(conflicts.into());
        }
        print!("{}", setup.text(plan.then_some(&*apply)));
        if plan || setup.is_empty() {
            return Ok(());
        }
        if let Some(reason) = refusal {
            return Err(reason.into());
        }
        if !yes {
            use std::io::{BufRead, IsTerminal, Write};
            if !std::io::stdin().is_terminal() {
                return Err(format!(
                    "swarm: nothing changed; run `swarm setup {list} --yes` to apply"
                )
                .into());
            }
            print!("Apply this plan? [y/N] ");
            std::io::stdout().flush()?;
            let mut reply = String::new();
            std::io::stdin().lock().read_line(&mut reply)?;
            if !matches!(reply.trim(), "y" | "Y" | "yes") {
                println!("swarm: nothing changed");
                return Ok(());
            }
        }
        // `--yes` applies what is pending when the lock is free, as the plan was only shown; an
        // answer to the question applies only the plan it answered.
        let shown = (!yes).then_some(&*plan_digest);
        return apply_setup(&dir, &groups, answer, resume, shown);
    }
    if let Some(reason) = refusal {
        return Err(reason.into());
    }
    apply_setup(&dir, &groups, answer, resume, digest)
}

/// Plan `groups` again under the trust lock and apply it, only while it still has `digest` when
/// one is given: the files first, then the plugin's commands and the HUD's LaunchAgent once the
/// lock is free.
pub(crate) fn apply_setup<'a>(
    dir: &std::path::Path,
    groups: &[&str],
    answer: impl Fn() -> &'a str,
    resume: bool,
    digest: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Swarm makes no write that it cannot record (ADR 0042), so the store opens first.
    let store = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let setup = swarm::managed::with_lock(&swarm::paths::trust_lock()?, || {
        let setup =
            SetupPlan::of(dir, groups, answer(), resume).map_err(|error| error.to_string())?;
        // A retry after a timeout finds nothing to do, and that is not a failure.
        if setup.is_empty() {
            return Ok(None);
        }
        if digest.is_some_and(|digest| setup.digest() != digest) {
            return Err("swarm: a file changed after the plan; check the plan again".into());
        }
        if let Some(conflicts) = swarm::managed::conflicts_text(&setup.plans) {
            return Err(conflicts);
        }
        // A running CLI may rewrite its trust file at any moment, such as `~/.claude.json`, and
        // the digest covers only the trust entries, so each trust file is planned again and
        // written as launch's `write_trust` does (L-4).
        for (((group, plan), replan), removes) in
            setup.grouped().zip(&setup.replans).zip(&setup.removes)
        {
            let written = match replan {
                Some(replan) => swarm::bus::write_trust(&store, replan)
                    .map(|written| written.into_iter().map(|plan| plan.path).collect()),
                None if *removes => swarm::managed::revert(&store, std::slice::from_ref(plan)),
                None => swarm::managed::apply(&store, std::slice::from_ref(plan)),
            };
            match written {
                Ok(changed) => {
                    for path in changed {
                        let verb = if *removes { "removed" } else { "wrote" };
                        println!("swarm: {group}: {verb} {}", path.display());
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "{error}\nswarm: stopped; each file named above is written and recorded, see `swarm managed list`"
                    ));
                }
            }
        }
        Ok(Some(setup))
    })?;
    let Some(setup) = setup else {
        println!("swarm: already set up. Nothing to do.");
        return Ok(());
    };
    if let Some(plugin) = &setup.plugin {
        plugin.run()?;
        println!("swarm: plugin: the plugin swarm is installed; a new session loads it");
    }
    if setup.hud.is_some() {
        let status = swarm::hud::install(&std::path::PathBuf::from(env_var("HOME")?))?;
        let running = if status.running {
            "runs"
        } else {
            "does not run yet"
        };
        println!("swarm: hud: the usage HUD starts at login and {running}");
    }
    Ok(())
}
