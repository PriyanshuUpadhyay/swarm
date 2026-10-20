//! CLI managed hooks.

use super::*;

/// `swarm hooks status --json`: whether swarm's own Codex and AGY state hooks and the guard are
/// set up. `swarm setup hooks` sets them up (ADR 0092). Claude's state hooks need no step, because
/// `swarm launch` passes them with `--settings`.
pub(crate) fn hooks(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
    let has_list = swarm::paths::guards_file()?.exists();
    let files = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["status", "--json"] => print_json(&serde_json::json!({
            "codex": files.homes.iter().all(|home| swarm::bus::codex_hooks_trusted(home, &codex)),
            "agy": swarm::bus::agy_hooks_set(&files.agy_hooks),
            "guard": files.guard_status(has_list),
        })),
        ["setup", ..] => Err("swarm: `swarm hooks setup` is now `swarm setup hooks`".into()),
        _ => Err(USAGE.into()),
    }
}

/// The files that `setup hooks` writes: each Codex home, AGY's `hooks.json`, and, for the guard,
/// each Claude `settings.json` and Codex `hooks.json`, one name per file.
pub(crate) struct HookFiles {
    pub(crate) homes: Vec<std::path::PathBuf>,
    pub(crate) agy_hooks: std::path::PathBuf,
    pub(crate) claude_settings: Vec<std::path::PathBuf>,
    pub(crate) codex_hooks: Vec<std::path::PathBuf>,
}

impl HookFiles {
    pub(crate) fn of(user_home: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let homes = codex_homes(user_home)?;
        // A Claude home picked with `CLAUDE_CONFIG_DIR` is one more target, as `CODEX_HOME` is
        // for Codex.
        // An empty or missing folder names no home.
        let active = env::var_os("CLAUDE_CONFIG_DIR")
            .map(std::path::PathBuf::from)
            .filter(|dir| dir.is_absolute() && dir.is_dir())
            .map(|dir| dir.join("settings.json"));
        let claude_settings = unique_targets(
            std::iter::once(user_home.join(".claude/settings.json"))
                .chain(active)
                .chain(
                    std::fs::read_dir(user_home.join(".claude/.profiles"))
                        .into_iter()
                        .flatten()
                        .filter_map(Result::ok)
                        .map(|entry| entry.path().join("settings.json"))
                        .filter(|path| path.exists()),
                ),
        );
        let codex_hooks = unique_targets(homes.iter().map(|home| home.join("hooks.json")));
        Ok(Self {
            agy_hooks: user_home.join(".gemini/config/hooks.json"),
            homes,
            claude_settings,
            codex_hooks,
        })
    }

    /// Every item that `setup hooks` adds, with swarm's current text, so `managed` finds one that
    /// a build wrote before swarm kept a record (ADR 0042). The text names no build (ADR 0034), so
    /// an item equal to it is swarm's. A Codex key's table is swarm's own key, so it is created.
    pub(crate) fn items(&self) -> Vec<swarm::managed::Edit> {
        use swarm::managed::{Edit, Writer};
        let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
        let mut items = Vec::new();
        for home in &self.homes {
            // A hooks.json swarm cannot read finds no pre-record guard keys. That is safe: a
            // found item is only listed and reverted while present, and recorded keys still list.
            let hooks = std::fs::read_to_string(home.join("hooks.json")).unwrap_or_default();
            let guard = swarm::bus::codex_guard_trust(home, &hooks);
            let keys = codex.iter().map(|entry| (entry, Writer::HooksState));
            for ((key, hash), writer) in
                keys.chain(guard.iter().map(|entry| (entry, Writer::HooksGuard)))
            {
                items.push(Edit {
                    created: 1,
                    ..swarm::bus::codex_trust_edit(home, key, hash, writer)
                });
            }
        }
        let guarded = self.claude_settings.iter().map(|path| (path, "claude"));
        for (path, provider) in guarded.chain(self.codex_hooks.iter().map(|path| (path, "codex"))) {
            for event in swarm::guard::EVENTS {
                items.push(swarm::bus::guard_group_edit(path, provider, event));
            }
        }
        for name in ["swarm", "swarm-guard"] {
            items.push(swarm::bus::agy_group_edit(&self.agy_hooks, name));
        }
        items
    }
}

/// `swarm managed list [--json] | revert (<id>... | --all) [--plan [--json] | --digest <digest>]`:
/// each item swarm wrote outside its home, recorded or found, with its live state, and removing
/// items that still equal what swarm wrote (ADR 0042). Running `revert` is the consent, as running
/// `setup` is; `--plan` shows the diff first and `--digest` applies only that plan.
pub(crate) fn managed(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let store = swarm::store::open(&swarm::paths::sqlite_db()?)?;
    let found = HookFiles::of(&std::path::PathBuf::from(env_var("HOME")?))?.items();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["list", rest @ ..] if matches!(rest, [] | ["--json"]) => {
            let entries = swarm::managed::list(&store, &found)?;
            if rest.is_empty() {
                for entry in &entries {
                    println!(
                        "{}  {}  {}  {}  {}",
                        entry.edit.id(),
                        entry.state.name(),
                        swarm::managed::wire(&entry.edit.writer),
                        entry.edit.file.display(),
                        entry.edit.path.join(".")
                    );
                }
                return Ok(());
            }
            let entries: Vec<_> = entries.iter().map(swarm::managed::Entry::json).collect();
            print_json(&serde_json::json!({ "entries": entries }))
        }
        ["revert", rest @ ..] => {
            let split = rest
                .iter()
                .position(|arg| matches!(*arg, "--plan" | "--digest"))
                .unwrap_or(rest.len());
            let (targets, flags) = rest.split_at(split);
            let target = match targets {
                ["--all"] => swarm::managed::Target::All,
                [] => return Err(USAGE.into()),
                ids if ids.iter().all(|id| !id.starts_with('-')) => {
                    swarm::managed::Target::Ids(ids.iter().map(|id| id.to_string()).collect())
                }
                _ => return Err(USAGE.into()),
            };
            let plan = || swarm::managed::revert_plan(&store, &found, &target);
            match flags {
                ["--plan"] => {
                    print!(
                        "{}",
                        swarm::managed::plan_text(
                            &plan()?,
                            &format!("swarm managed revert {}", targets.join(" ")),
                            "Swarm has nothing to remove. No file changes."
                        )
                    );
                    Ok(())
                }
                ["--plan", "--json"] => print_json(&swarm::managed::plan_json(&plan()?)),
                [] | ["--digest", _] => {
                    if let Some(reason) = swarm::bus::revert_refusal(
                        env::var("SWARM_AGENT_ID").ok().as_deref(),
                        env::var("HERDR_AGENT_PANE").ok().as_deref(),
                    ) {
                        return Err(reason.into());
                    }
                    let lock = swarm::paths::trust_lock()?;
                    swarm::managed::with_lock(&lock, || {
                        let plans = plan()?;
                        if let ["--digest", digest] = flags
                            && swarm::managed::digest(&plans) != *digest
                        {
                            return Err("swarm: a managed file changed after the plan; check the plan again".into());
                        }
                        for path in swarm::managed::revert(&store, &plans)? {
                            println!("swarm: removed swarm's entries from {}", path.display());
                        }
                        Ok(())
                    })?;
                    Ok(())
                }
                _ => Err(USAGE.into()),
            }
        }
        _ => Err(USAGE.into()),
    }
}

impl HookFiles {
    /// Whether the guard registrations match the rule list: each one in place while the list
    /// exists, and none left once it is gone, because a registration with no list blocks every
    /// call.
    pub(crate) fn guard_status(&self, has_list: bool) -> bool {
        let files = || {
            let claude = self.claude_settings.iter().map(|path| (path, "claude"));
            claude.chain(self.codex_hooks.iter().map(|path| (path, "codex")))
        };
        if !has_list {
            return !swarm::bus::agy_guard_present(&self.agy_hooks)
                && files().all(|(path, provider)| !swarm::bus::guard_registered(path, provider));
        }
        files().all(|(path, provider)| {
            swarm::bus::guard_hooks_plan(path, provider)
                .is_ok_and(|plan| plan.conflicts.is_empty() && plan.after == plan.before)
        }) && self.homes.iter().all(|home| {
            let hooks = std::fs::read_to_string(home.join("hooks.json")).unwrap_or_default();
            let entries = swarm::bus::codex_guard_trust(home, &hooks);
            !entries.is_empty() && swarm::bus::codex_hooks_trusted(home, &entries)
        }) && swarm::bus::agy_guard_set(&self.agy_hooks)
    }

    /// Whether swarm's own Codex and AGY state hooks are set up.
    pub(crate) fn hooks_status(&self) -> bool {
        let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
        self.homes
            .iter()
            .all(|home| swarm::bus::codex_hooks_trusted(home, &codex))
            && swarm::bus::agy_hooks_set(&self.agy_hooks)
    }
}

/// The plan of every file `setup hooks` writes. A file that swarm cannot read or edit is a
/// conflict in the plan, not an error.
pub(crate) fn hook_plans(files: &HookFiles, has_list: bool) -> Vec<swarm::managed::FilePlan> {
    let codex = swarm::bus::codex_hook_trust(&swarm::bus::shared_hook_command("codex"));
    let unreadable = swarm::managed::FilePlan::unreadable;
    let guard_plan = |path: &std::path::PathBuf, provider: &str| {
        swarm::bus::guard_hooks_plan(path, provider)
            .unwrap_or_else(|error| unreadable(path.clone(), error))
    };
    let mut plans: Vec<_> = Vec::new();
    if has_list {
        plans.extend(
            files
                .claude_settings
                .iter()
                .map(|path| guard_plan(path, "claude")),
        );
        plans.extend(
            files
                .codex_hooks
                .iter()
                .map(|path| guard_plan(path, "codex")),
        );
    }
    let codex_plans: Vec<_> = files
        .homes
        .iter()
        .map(|home| {
            let mut guard = Vec::new();
            if has_list {
                // Trust the guard group where the planned hooks.json puts it.
                let target = canonical(&home.join("hooks.json"));
                let hooks = plans
                    .iter()
                    .find(|plan| canonical(&plan.path) == target)
                    .map_or("", |plan| plan.after.as_str());
                guard = swarm::bus::codex_guard_trust(home, hooks);
            }
            swarm::bus::codex_hook_plan(home, &codex, &guard)
                .unwrap_or_else(|error| unreadable(home.join("config.toml"), error))
        })
        .collect();
    plans.extend(codex_plans);
    plans.push(
        swarm::bus::agy_hook_plan(&files.agy_hooks, has_list)
            .unwrap_or_else(|error| unreadable(files.agy_hooks.clone(), error)),
    );
    plans
}
