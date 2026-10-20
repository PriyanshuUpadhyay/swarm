//! CLI setup plan.

use super::*;

/// The items of `swarm setup`, in apply order (ADR 0092). The list may grow, so the app shows an
/// item it does not know by its name.
pub(crate) const SETUP_ITEMS: [&str; 4] = ["hooks", "trust", "plugin", "hud"];

/// One `swarm setup` plan: each file's plan in apply order, the item (`group`) of each, each item
/// part that was left out with its reason, and the commands the plugin and HUD items run.
pub(crate) struct SetupPlan {
    pub(crate) plans: Vec<swarm::managed::Plan>,
    pub(crate) selected: Vec<String>,
    pub(crate) groups: Vec<&'static str>,
    /// For each trust plan, how to plan that one file again when it is written (L-4), so a write
    /// under the trust lock reads no other file and runs no `git` (L-1).
    pub(crate) replans: Vec<Option<Replan>>,
    /// For each plan, whether it removes swarm's own items (`managed::revert`) instead of adding.
    pub(crate) removes: Vec<bool>,
    pub(crate) skipped: Vec<(&'static str, String)>,
    pub(crate) plugin: Option<PluginPlan>,
    /// The LaunchAgent text the HUD item writes, or None when the HUD already runs from it.
    pub(crate) hud: Option<String>,
    /// The HUDs that the login item did not start, which the HUD install stops.
    pub(crate) hud_strays: Vec<u32>,
}

pub(crate) type Replan = Box<dyn Fn() -> Result<swarm::managed::FilePlan, String>>;

impl SetupPlan {
    /// The plan of every pending write in `groups`. The trust group is the consent file set to
    /// `consent`, then the entries a launch in `cwd` would write, for a resuming seat when
    /// `resume`. A trust entry in a file that an earlier plan changes is planned on that plan's
    /// text, so the two apply one after the other.
    pub(crate) fn of(
        cwd: &std::path::Path,
        groups: &[&str],
        consent: &str,
        resume: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        use swarm::managed::FilePlan;
        let user_home = std::path::PathBuf::from(env_var("HOME")?);
        let files = HookFiles::of(&user_home)?;
        let mut setup = Self {
            plans: Vec::new(),
            selected: groups.iter().map(|group| group.to_string()).collect(),
            groups: Vec::new(),
            replans: Vec::new(),
            removes: Vec::new(),
            skipped: Vec::new(),
            plugin: None,
            hud: None,
            hud_strays: Vec::new(),
        };
        if groups.contains(&"hooks") {
            let has_list = swarm::paths::guards_file()?.exists();
            for plan in hook_plans(&files, has_list) {
                setup.push("hooks", plan);
            }
        }
        if groups.contains(&"trust") {
            let answer = consent.to_string();
            setup.push_trust(
                consent_plan(consent)?,
                Box::new(move || consent_plan(&answer).map_err(|error| error.to_string())),
            );
            match trust_target(cwd, &user_home) {
                Err(reason) => setup.skipped.push(("trust", reason)),
                Ok(target) => {
                    // A Codex home or Claude config swarm cannot edit is skipped with its reason,
                    // as a launch with no picked account does (`trust_each`).
                    let configs: Vec<_> = files
                        .homes
                        .iter()
                        .map(|home| home.join("config.toml"))
                        .collect();
                    let mut skipped = trust_each(&configs, false, |path| {
                        let earlier = setup
                            .plans
                            .iter()
                            .rev()
                            .filter_map(|plan| match plan {
                                swarm::managed::Plan::File(plan) => Some(plan),
                                _ => None,
                            })
                            .find(|plan| canonical(&plan.path) == canonical(path));
                        // A file the earlier plan cannot edit has no planned text to build on.
                        if let Some(conflict) = earlier.and_then(|plan| plan.conflicts.first()) {
                            return Err(conflict.found.clone());
                        }
                        let home = path.parent().unwrap_or(path).to_path_buf();
                        let plan = match earlier.map(|plan| plan.after.clone()) {
                            Some(text) => swarm::bus::codex_trust_plan_on(path, text, &target)?,
                            None => swarm::bus::codex_trust_plan(&home, &target)?,
                        };
                        if swarm::bus::codex_left_untrusted(&plan.before, &target) {
                            setup.skipped.push((
                                "trust",
                                format!(
                                    "{}: you marked this folder untrusted; the pane asks",
                                    path.display()
                                ),
                            ));
                        }
                        // At write time the hooks are on disk, so the replan reads the file.
                        let target = target.clone();
                        setup.push_trust(
                            plan,
                            Box::new(move || swarm::bus::codex_trust_plan(&home, &target)),
                        );
                        Ok(())
                    })?;
                    let settings = agy_settings(&user_home);
                    let plan = swarm::bus::agy_trust_plan(&settings, &target)
                        .unwrap_or_else(|error| FilePlan::unreadable(settings.clone(), error));
                    setup.push_trust(
                        plan,
                        Box::new(move || swarm::bus::agy_trust_plan(&settings, &target)),
                    );
                    // A seat's Claude runs from the pool under cwd, or in cwd when it resumes.
                    let flags = if resume {
                        vec!["--resume".into()]
                    } else {
                        Vec::new()
                    };
                    let (dir, _) = swarm::bus::claude_child("", cwd, &flags, &uuid::Uuid::nil());
                    skipped.extend(trust_each(&claude_configs(&user_home), false, |config| {
                        let plan = swarm::bus::claude_trust_plan(config, &dir)?;
                        let (config, dir) = (config.to_path_buf(), dir.clone());
                        setup.push_trust(
                            plan,
                            Box::new(move || swarm::bus::claude_trust_plan(&config, &dir)),
                        );
                        Ok(())
                    })?);
                    setup
                        .skipped
                        .extend(skipped.into_iter().map(|reason| ("trust", reason)));
                }
            }
        }
        if groups.contains(&"plugin") {
            // The skill links an older swarm made would list each kit skill twice (ADR 0092).
            let old_links = old_skill_links(&files)?;
            let leaving: Vec<_> = old_links
                .iter()
                .map(|plan| plan.path().to_path_buf())
                .collect();
            let plugin = plugin_plan(&user_home, &leaving)?;
            setup
                .skipped
                .extend(plugin.notes.iter().map(|note| ("plugin", note.clone())));
            for plan in old_links {
                if plan.conflicts().is_empty() {
                    setup.push("plugin", plan);
                    *setup.removes.last_mut().unwrap() = true;
                } else {
                    setup.skipped.push((
                        "plugin",
                        format!(
                            "{} is no longer swarm's link; swarm leaves it",
                            plan.path().display()
                        ),
                    ));
                }
            }
            setup.plugin = Some(plugin);
        }
        if groups.contains(&"hud") {
            let text = swarm::hud::planned()?;
            let now = swarm::hud::status(&user_home);
            let written = std::fs::read_to_string(swarm::hud::plist_path(&user_home)).ok();
            if !(now.running && written.as_ref() == Some(&text) && now.strays.is_empty()) {
                setup.hud = Some(text);
                setup.hud_strays = now.strays;
            }
        }
        Ok(setup)
    }

    pub(crate) fn push(&mut self, group: &'static str, plan: impl Into<swarm::managed::Plan>) {
        self.groups.push(group);
        self.plans.push(plan.into());
        self.replans.push(None);
        self.removes.push(false);
    }

    pub(crate) fn push_trust(&mut self, plan: swarm::managed::FilePlan, replan: Replan) {
        self.push("trust", plan);
        *self.replans.last_mut().unwrap() = Some(replan);
    }

    /// Each plan with its group.
    pub(crate) fn grouped(&self) -> impl Iterator<Item = (&'static str, &swarm::managed::Plan)> {
        self.groups.iter().copied().zip(&self.plans)
    }

    /// The digest that apply checks. Every running CLI rewrites a trust file such as
    /// `~/.claude.json`, so a trust plan's part is only each entry it adds and the value it
    /// replaces, and a change to one of those still changes the digest (L-4). Every other part is
    /// the file's whole text, as in `managed::digest`.
    pub(crate) fn digest(&self) -> String {
        use sha2::Digest;
        let mut digest = sha2::Sha256::new();
        digest.update(serde_json::to_vec(&self.selected).unwrap());
        digest.update([0]);
        for line in self
            .commands()
            .into_iter()
            .map(|(_, line)| line)
            .chain(self.hud.clone())
        {
            digest.update(line.as_bytes());
            digest.update([0]);
        }
        for (group, plan) in self.grouped() {
            let mut parts = vec![
                group.to_string(),
                plan.path().to_string_lossy().into_owned(),
            ];
            if group == "trust" {
                for edit in plan.edits() {
                    let before = edit.before.as_ref().map(serde_json::Value::to_string);
                    parts.extend([edit.id(), edit.wrote.to_string(), format!("{before:?}")]);
                }
            } else {
                parts.push(swarm::managed::digest(std::slice::from_ref(plan)));
            }
            for part in parts {
                digest.update(part.as_bytes());
                digest.update([0]);
            }
        }
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Each command line the plugin and HUD items run after the files, with its item.
    pub(crate) fn commands(&self) -> Vec<(&'static str, String)> {
        let mut lines: Vec<_> = self
            .plugin
            .iter()
            .flat_map(PluginPlan::lines)
            .map(|line| ("plugin", line))
            .collect();
        if self.hud.is_some() {
            if !self.hud_strays.is_empty() {
                let pids: Vec<_> = self.hud_strays.iter().map(u32::to_string).collect();
                lines.push((
                    "hud",
                    format!(
                        "run  kill {}, the usage HUDs that the login item did not start",
                        pids.join(" ")
                    ),
                ));
            }
            lines.push((
                "hud",
                "run  launchctl bootstrap, so the usage HUD starts at login".into(),
            ));
        }
        lines
    }

    /// The HUD's LaunchAgent diff, or "" without one.
    pub(crate) fn hud_diff(&self) -> String {
        let Some(text) = &self.hud else {
            return String::new();
        };
        let path = std::path::PathBuf::from(env_var("HOME").unwrap_or_default());
        let path = swarm::hud::plist_path(&path);
        let before = std::fs::read_to_string(&path).unwrap_or_default();
        swarm::diff::unified(&path.to_string_lossy(), &before, text)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.plans
            .iter()
            .all(|plan| !plan.changes() && plan.conflicts().is_empty())
            && self.plugin.is_none()
            && self.hud.is_none()
    }

    /// Each skipped part, each diff and command, then the conflicts, or else `apply`, the command
    /// that applies the plan, when this is only the plan.
    pub(crate) fn text(&self, apply: Option<&str>) -> String {
        let mut text: String = self
            .skipped
            .iter()
            .map(|(group, reason)| format!("skipped ({group}): {reason}\n"))
            .collect();
        if self.is_empty() {
            return text + "Swarm is already set up. Nothing to do.\n";
        }
        text.extend(self.plans.iter().map(swarm::managed::diff));
        text += &self.hud_diff();
        for (_, line) in self.commands() {
            text += &format!("{line}\n");
        }
        match (swarm::managed::conflicts_text(&self.plans), apply) {
            (Some(conflicts), _) => text + &format!("\n{conflicts}\n"),
            (None, Some(apply)) => {
                text + &format!("\nPlan only. Nothing changed. Run `{apply}` to apply.\n")
            }
            (None, None) => text,
        }
    }
}

/// The plugin item: the link `~/.swarm/kit` to this build's kit, and each CLI's install command.
pub(crate) struct PluginPlan {
    pub(crate) link: std::path::PathBuf,
    pub(crate) target: std::path::PathBuf,
    pub(crate) relink: bool,
    pub(crate) steps: Vec<swarm::plugin::Step>,
    pub(crate) notes: Vec<String>,
}

impl PluginPlan {
    pub(crate) fn lines(&self) -> Vec<String> {
        let link = self
            .relink
            .then(|| format!("link {} -> {}", self.link.display(), self.target.display()));
        link.into_iter()
            .chain(
                self.steps
                    .iter()
                    .map(|step| format!("run  {}", step.line())),
            )
            .collect()
    }

    pub(crate) fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.relink {
            // Only a link of swarm's own is replaced; a real folder there is the owner's.
            if self
                .link
                .symlink_metadata()
                .is_ok_and(|meta| meta.is_symlink())
            {
                std::fs::remove_file(&self.link)?;
            }
            std::os::unix::fs::symlink(&self.target, &self.link)
                .map_err(|error| format!("swarm: cannot link {}: {error}", self.link.display()))?;
        }
        for step in &self.steps {
            let mut command = std::process::Command::new(&step.argv[0]);
            command.args(&step.argv[1..]);
            if let Some(home) = &step.codex_home {
                command.env("CODEX_HOME", home);
            }
            let status = command.status()?;
            if !status.success() {
                return Err(format!(
                    "swarm: `{}` failed ({status}); the commands before it ran",
                    step.line()
                )
                .into());
            }
        }
        Ok(())
    }
}
