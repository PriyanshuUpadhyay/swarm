//! CLI trust.

use super::*;

/// A path as its link target, so two names of one file are one file. Two plans of one missing
/// file both write the same text, which `write_text` accepts.
pub(crate) fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Each path whose link target no earlier path shares, because two plans of one file would each
/// find it changed by the other at apply.
pub(crate) fn unique_targets(
    paths: impl Iterator<Item = std::path::PathBuf>,
) -> Vec<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    paths.filter(|path| seen.insert(canonical(path))).collect()
}

/// Folder trust in each Codex home or Claude config a pane may read. With no account picked, one
/// that swarm cannot edit is skipped, and its path and reason returned, so one broken spare
/// profile does not stop every launch or setup; the one of a picked account must take the entry.
pub(crate) fn trust_each(
    paths: &[std::path::PathBuf],
    picked: bool,
    mut ensure: impl FnMut(&std::path::Path) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    let mut skipped = Vec::new();
    for path in paths {
        match ensure(path) {
            Err(error) if !picked => skipped.push(format!("{}: {error}", path.display())),
            result => result?,
        }
    }
    Ok(skipped)
}

/// What a launch may do with the trust entries its pane needs (ADR 0043).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TrustConsent {
    /// The owner gave standing consent: write each entry and name it.
    Standing,
    /// A chair in the folder the owner picked, with no standing consent: write each entry for
    /// this folder, and show its diff, so the write is never silent (owner answer I1).
    Picked,
    /// No consent: write nothing; show each pending diff and the command that approves it.
    Ask,
}

/// The owner's answer for launch folder trust in `~/.swarm/consent.json`: `standing`, `ask`, or
/// None for no answer. A file swarm cannot read is no answer (R4), and is named.
pub(crate) fn trust_answer() -> Option<&'static str> {
    let path = swarm::paths::consent_file().ok()?;
    match read_consent(&path) {
        Ok((_, value)) => [STANDING, ASK]
            .into_iter()
            .find(|answer| value[TRUST_KEY] == *answer),
        Err(error) => {
            eprintln!("{error}; folder trust has no consent");
            None
        }
    }
}

/// The owner's standing consent for launch folder trust. Anything else is no consent (R4).
pub(crate) fn standing_consent() -> bool {
    trust_answer() == Some(STANDING)
}

/// The trust entries one launch needs, and what it may do with them.
pub(crate) struct LaunchTrust<'a> {
    pub(crate) store: &'a rusqlite::Connection,
    pub(crate) consent: TrustConsent,
    /// Each file must take the entry, as a picked account's or AGY's only one does (see
    /// `trust_each`).
    pub(crate) required: bool,
    /// The launch's folder, for the command that approves a pending entry.
    pub(crate) cwd: &'a std::path::Path,
}

impl LaunchTrust<'_> {
    /// Plan `dir` trusted for `provider` in each of `files`. With consent, write each plan
    /// through the managed-edits module under the trust lock, and print the bare
    /// `trusted <provider> <dir>` line the app reads. With none, write nothing, and print the bare
    /// `trust-pending <provider> <dir>` line, each diff, and the command that approves it.
    pub(crate) fn run(
        &self,
        provider: &str,
        dir: &std::path::Path,
        files: &[std::path::PathBuf],
        plan: fn(&std::path::Path, &std::path::Path) -> Result<swarm::managed::FilePlan, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.consent == TrustConsent::Ask {
            let mut diffs = String::new();
            for file in files {
                match plan(file, dir) {
                    Ok(plan) => diffs += &swarm::managed::diff(&plan.into()),
                    Err(error) => eprintln!("swarm: skipped {}: {error}", file.display()),
                }
            }
            if !diffs.is_empty() {
                eprintln!("trust-pending {provider} {}", dir.display());
                eprint!("{diffs}");
                eprintln!(
                    "swarm: no consent for folder trust, so swarm wrote nothing; the pane asks instead."
                );
                // A Claude seat runs in cwd only when it resumes (`claude_child`).
                let resume = if provider == "claude" && dir == self.cwd {
                    " --resume"
                } else {
                    ""
                };
                eprintln!(
                    "swarm: approve with `swarm setup trust --plan --cwd {}{resume}`, then the `swarm setup trust --digest …` it prints.",
                    swarm::adapter::shell_line(&[self.cwd.to_string_lossy().into_owned()])
                );
            }
            return Ok(());
        }
        let mut written = Vec::new();
        let skipped = swarm::managed::with_lock(&swarm::paths::trust_lock()?, || {
            trust_each(files, self.required, |file| {
                written.extend(swarm::bus::write_trust(self.store, || plan(file, dir))?);
                Ok(())
            })
        })?;
        for reason in skipped {
            eprintln!("swarm: skipped {reason}");
        }
        for plan in &written {
            if self.consent == TrustConsent::Picked {
                eprint!("{}", swarm::managed::diff(&plan.clone().into()));
            }
            eprintln!(
                "swarm: trusted {} for {provider} in {}",
                dir.display(),
                plan.path.display()
            );
        }
        if !written.is_empty() {
            eprintln!("trusted {provider} {}", dir.display());
        }
        Ok(())
    }
}

/// AGY's settings file, which holds its folder trust.
pub(crate) fn agy_settings(user_home: &std::path::Path) -> std::path::PathBuf {
    user_home.join(".gemini/antigravity-cli/settings.json")
}

/// Native and legacy Claude config files for the setup trust plan.
pub(crate) fn claude_configs(user_home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut configs = vec![user_home.join(".claude.json")];
    if let Ok(profiles) = std::fs::read_dir(user_home.join(".claude/.profiles")) {
        configs.extend(
            profiles
                .filter_map(Result::ok)
                // Hidden dirs hold helper data, not account config files.
                .filter(|entry| {
                    !entry.file_name().to_string_lossy().starts_with('.') && entry.path().is_dir()
                })
                .map(|entry| entry.path().join(".claude.json")),
        );
    }
    configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_spare_codex_home_is_skipped_unless_its_account_was_picked() {
        let root = std::env::temp_dir().join(format!("swarm-spare-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (main, spare) = (root.join(".codex"), root.join(".codex-spare"));
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&spare).unwrap();
        std::fs::write(spare.join("config.toml"), "not toml = =\n").unwrap();
        let project = std::path::Path::new("/project");

        let store = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let codex = |home: &std::path::Path| {
            swarm::bus::write_trust(&store, || swarm::bus::codex_trust_plan(home, project))
                .map(|_| ())
        };
        trust_each(&[spare.clone(), main.clone()], false, codex).unwrap();
        assert!(
            std::fs::read_to_string(main.join("config.toml"))
                .unwrap()
                .contains("/project")
        );
        assert_eq!(
            std::fs::read_to_string(spare.join("config.toml")).unwrap(),
            "not toml = =\n"
        );
        assert!(trust_each(std::slice::from_ref(&spare), true, codex).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_read_only_spare_claude_profile_is_skipped_unless_its_account_was_picked() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("swarm-spare-claude-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (main, spare) = (root.join("main.json"), root.join("spare.json"));
        std::fs::write(&spare, "{}\n").unwrap();
        std::fs::set_permissions(&spare, std::fs::Permissions::from_mode(0o444)).unwrap();
        let project = std::path::Path::new("/project");
        let store = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        let claude = |config: &std::path::Path| {
            swarm::bus::write_trust(&store, || swarm::bus::claude_trust_plan(config, project))
                .map(|_| ())
        };

        trust_each(&[spare.clone(), main.clone()], false, claude).unwrap();
        assert!(std::fs::read_to_string(&main).unwrap().contains("/project"));
        assert_eq!(std::fs::read_to_string(&spare).unwrap(), "{}\n");
        assert!(trust_each(std::slice::from_ref(&spare), true, claude).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn default_codex_trust_uses_codex_home_or_login_home() {
        let root =
            std::env::temp_dir().join(format!("swarm-default-codex-test-{}", std::process::id()));
        let login = root.join("login");
        let codex = default_codex_home_with_env(|name| match name {
            "HOME" => Some(login.clone().into_os_string()),
            _ => None,
        })
        .unwrap();
        let store = swarm::store::open(std::path::Path::new(":memory:")).unwrap();
        swarm::bus::write_trust(&store, || {
            swarm::bus::codex_trust_plan(&codex, std::path::Path::new("/project"))
        })
        .unwrap();
        assert!(login.join(".codex/config.toml").exists());

        let override_home = root.join("override");
        assert_eq!(
            default_codex_home_with_env(
                |name| (name == "CODEX_HOME").then(|| override_home.clone().into_os_string())
            )
            .unwrap(),
            override_home
        );
    }
}
