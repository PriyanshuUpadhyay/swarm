//! Agent profiles: which runners start each role, in order (ADRs 0031, 0032). The file is
//! `$SWARM_HOME/.swarm/profiles.json`. With no file, the first read imports the old agent-routing
//! `roles.json` if one exists, else the built-in `default-profiles.json` is used and nothing is
//! written.

use crate::providers::Provider;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const DEFAULT: &str = include_str!("../default-profiles.json");

/// Below this share of usage left on every signed-in account, a runner is skipped. Low enough
/// that a nearly spent account still runs a short task, high enough to skip one that would stop
/// mid-turn.
const DEFAULT_MIN_USAGE_LEFT_PCT: u8 = 5;

mod profiles;
mod storage;

pub use profiles::*;
pub use storage::*;

#[cfg(test)]
use storage::{apply_action, update_at, write};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_names_match_the_native_directory_rule() {
        for name in [
            "work",
            "personal-2",
            "spare.account",
            "work_profile",
            "-work",
            ".work",
        ] {
            assert!(valid_account_name(name), "rejected {name:?}");
        }
        for name in [
            "",
            "auto",
            "default",
            "current",
            ".",
            "..",
            "work..personal",
            "Work",
            "../work",
            "work/personal",
            "work\\personal",
            "work personal",
            "work\n",
            "é",
            "work$(id)",
        ] {
            assert!(!valid_account_name(name), "accepted {name:?}");
        }
    }

    fn runner(provider: Provider, model: &str) -> Runner {
        Runner {
            provider,
            model: model.into(),
            effort: "high".into(),
            sandbox: None,
            approval: None,
            permission: None,
        }
    }

    fn config(profiles: Vec<Profile>) -> Config {
        Config {
            version: 1,
            min_usage_left_pct: 5,
            imported: None,
            profiles,
        }
    }

    fn chat() -> Profile {
        Profile {
            name: "chat".into(),
            runners: vec![runner(Provider::Claude, "opus")],
        }
    }

    struct ActionFixture {
        path: PathBuf,
        config: Config,
        bytes: Vec<u8>,
    }

    impl ActionFixture {
        fn new(name: &str) -> Self {
            let folder = std::env::temp_dir().join(format!(
                "swarm-profile-action-{name}-{}",
                uuid::Uuid::now_v7()
            ));
            std::fs::create_dir(&folder).unwrap();
            let path = folder.join("profiles.json");
            let config = built_in();
            let bytes = write(&path, &config).unwrap();
            Self {
                path,
                config,
                bytes,
            }
        }

        fn apply(
            &mut self,
            action: ProfileAction<'_>,
            expected: Option<&str>,
        ) -> Result<String, String> {
            let expected = expected
                .map(str::to_owned)
                .unwrap_or_else(|| revision(&self.bytes));
            let result = update_at(
                &self.path,
                self.config.clone(),
                &self.bytes,
                &expected,
                |config| apply_action(config, action),
            );
            let stored = std::fs::read(&self.path).unwrap();
            if let Ok(saved) = &result {
                assert_eq!(*saved, revision(&stored));
                self.config = parse(std::str::from_utf8(&stored).unwrap()).unwrap();
                self.bytes = stored;
            } else {
                assert_eq!(
                    stored, self.bytes,
                    "a refused action must leave the file unchanged"
                );
            }
            result
        }
    }

    impl Drop for ActionFixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(self.path.parent().unwrap()).unwrap();
        }
    }

    #[test]
    fn action_fixture_removes_its_directory_and_backups() {
        let folder;
        {
            let mut fixture = ActionFixture::new("cleanup");
            folder = fixture.path.parent().unwrap().to_path_buf();
            fixture.apply(ProfileAction::New("custom"), None).unwrap();
            assert!(std::fs::read_dir(&folder).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "bak")
            }));
        }
        assert!(!folder.exists());
    }

    #[test]
    fn new_copies_only_the_first_chat_runner_and_rejects_existing_or_invalid_names() {
        let mut fixture = ActionFixture::new("new");
        let primary = fixture.config.profile("chat").unwrap().runners[0].clone();
        fixture.apply(ProfileAction::New("custom"), None).unwrap();
        assert_eq!(
            fixture.config.profile("custom").unwrap().runners,
            vec![primary]
        );
        assert!(
            fixture
                .apply(ProfileAction::New("custom"), None)
                .unwrap_err()
                .contains("already exists")
        );
        for invalid in ["", "two words", "path/name", "Uppercase"] {
            assert!(fixture.apply(ProfileAction::New(invalid), None).is_err());
        }
    }

    #[test]
    fn rename_keeps_runners_and_order_and_rejects_missing_or_taken_names() {
        let mut fixture = ActionFixture::new("rename");
        let original = fixture.config.profiles[1].clone();
        fixture
            .apply(ProfileAction::Rename(&original.name, "renamed"), None)
            .unwrap();
        assert_eq!(fixture.config.profiles[1].name, "renamed");
        assert_eq!(fixture.config.profiles[1].runners, original.runners);
        assert!(fixture.config.profile(&original.name).is_err());
        assert!(
            fixture
                .apply(ProfileAction::Rename("missing", "other"), None)
                .unwrap_err()
                .contains("not defined")
        );
        assert!(
            fixture
                .apply(ProfileAction::Rename("renamed", "chat"), None)
                .unwrap_err()
                .contains("already exists")
        );
        assert!(
            fixture
                .apply(ProfileAction::Rename("renamed", "path/name"), None)
                .is_err()
        );
        assert!(
            fixture
                .apply(ProfileAction::Rename("chat", "other"), None)
                .unwrap_err()
                .contains("first profile")
        );
    }

    #[test]
    fn copy_keeps_every_runner_and_rejects_missing_taken_or_invalid_names() {
        let mut fixture = ActionFixture::new("copy");
        let original = fixture.config.profile("chat").unwrap().clone();
        fixture
            .apply(ProfileAction::Copy("chat", "copied"), None)
            .unwrap();
        assert_eq!(
            fixture.config.profile("copied").unwrap().runners,
            original.runners
        );
        assert_eq!(fixture.config.profile("chat").unwrap(), &original);
        assert!(
            fixture
                .apply(ProfileAction::Copy("missing", "other"), None)
                .unwrap_err()
                .contains("not defined")
        );
        assert!(
            fixture
                .apply(ProfileAction::Copy("chat", "copied"), None)
                .unwrap_err()
                .contains("already exists")
        );
        assert!(
            fixture
                .apply(ProfileAction::Copy("chat", "two words"), None)
                .is_err()
        );
    }

    #[test]
    fn delete_removes_one_profile_and_refuses_chat_and_missing_names() {
        let mut fixture = ActionFixture::new("delete");
        let removed = fixture.config.profiles[1].name.clone();
        let count = fixture.config.profiles.len();
        fixture
            .apply(ProfileAction::Delete(&removed), None)
            .unwrap();
        assert!(fixture.config.profile(&removed).is_err());
        assert_eq!(fixture.config.profiles.len(), count - 1);
        assert!(
            fixture
                .apply(ProfileAction::Delete("chat"), None)
                .unwrap_err()
                .contains("cannot be deleted")
        );
        assert!(
            fixture
                .apply(ProfileAction::Delete("missing"), None)
                .unwrap_err()
                .contains("not defined")
        );
    }

    #[test]
    fn reset_replaces_all_profiles_with_bundled_and_keeps_min_usage() {
        let mut fixture = ActionFixture::new("reset");
        fixture.apply(ProfileAction::New("custom"), None).unwrap();
        fixture.apply(ProfileAction::SetMinUsage(42), None).unwrap();
        fixture.apply(ProfileAction::Reset, None).unwrap();
        assert_eq!(fixture.config.profiles, built_in().profiles);
        assert_eq!(fixture.config.min_usage_left_pct, 42);
    }

    #[test]
    fn min_usage_accepts_zero_to_100_and_refuses_values_outside_it() {
        let mut fixture = ActionFixture::new("min-usage");
        for pct in [0, 37, 100] {
            let profiles = fixture.config.profiles.clone();
            fixture
                .apply(ProfileAction::SetMinUsage(pct), None)
                .unwrap();
            assert_eq!(i64::from(fixture.config.min_usage_left_pct), pct);
            assert_eq!(fixture.config.profiles, profiles);
        }
        for pct in [-1, 101, 256, i64::MAX] {
            assert!(
                fixture
                    .apply(ProfileAction::SetMinUsage(pct), None)
                    .unwrap_err()
                    .contains("0 to 100")
            );
        }
    }

    #[test]
    fn every_action_refuses_a_stale_revision_before_changing_the_file() {
        let mut fixture = ActionFixture::new("stale");
        let stale = revision(&fixture.bytes);
        fixture.apply(ProfileAction::New("custom"), None).unwrap();
        for action in [
            ProfileAction::New("other"),
            ProfileAction::Rename("custom", "other"),
            ProfileAction::Copy("custom", "other"),
            ProfileAction::Delete("custom"),
            ProfileAction::Reset,
            ProfileAction::SetMinUsage(50),
        ] {
            assert_eq!(
                fixture.apply(action, Some(&stale)).unwrap_err(),
                "profiles changed on disk; reload and try again"
            );
        }
    }

    #[test]
    fn the_built_in_profiles_are_valid_and_start_with_chat() {
        let built_in = built_in();
        assert_eq!(built_in.profiles[0].name, "chat");
        assert!(built_in.imported.is_none());
    }

    #[test]
    fn validate_names_each_broken_rule() {
        assert_eq!(validate(&config(vec![chat()])), Vec::<String>::new());
        let mut codex = runner(Provider::Codex, "--yolo");
        codex.effort = "huge".into();
        codex.permission = Some("auto".into());
        codex.sandbox = Some("two words".into());
        let broken = Config {
            version: 2,
            min_usage_left_pct: 101,
            imported: None,
            profiles: vec![
                Profile {
                    name: "Code".into(),
                    runners: vec![codex, runner(Provider::Claude, "claude-fable-5")],
                },
                Profile {
                    name: "chat".into(),
                    runners: vec![],
                },
                chat(),
            ],
        };
        assert_eq!(
            validate(&broken),
            [
                "version must be 1, not 2.",
                "min_usage_left_pct must be 0 to 100.",
                "The first profile must be 'chat'.",
                "Profile name 'Code' must use a-z, 0-9, '.', '_', '-'.",
                "Profile 'Code' runner 1 has invalid model '--yolo'.",
                "Profile 'Code' runner 1 has effort 'huge', which Codex does not take.",
                "Profile 'Code' runner 1 has invalid sandbox 'two words'.",
                "Profile 'Code' runner 1: Codex has no permission.",
                "Profile 'Code' runner 2: Fable is allowed only in a review.* or council.* profile.",
                "Profile 'chat' must have at least one runner.",
                "Profile 'chat' is defined twice.",
            ]
        );
        let mut repeated = chat();
        repeated.runners.push(runner(Provider::Claude, "opus"));
        assert_eq!(
            validate(&config(vec![repeated])),
            ["Profile 'chat' runner 2 repeats an earlier runner."]
        );
    }

    fn account(signed_in: bool, remaining_pct: Option<i64>) -> AccountState {
        AccountState {
            signed_in,
            remaining_pct,
        }
    }

    #[test]
    fn accounts_skip_a_runner_only_when_signed_out_or_low_on_every_account() {
        let claude = Provider::Claude;
        assert_eq!(account_skip(claude, None, 5), None);
        assert_eq!(account_skip(claude, Some(&[]), 5), None);
        assert_eq!(
            account_skip(claude, Some(&[account(false, Some(90))]), 5),
            Some((SkipCode::SignedOut, "no claude account is signed in".into()))
        );
        let spent = [
            account(true, Some(2)),
            account(true, Some(4)),
            account(false, Some(80)),
        ];
        assert_eq!(
            account_skip(claude, Some(&spent), 5),
            Some((SkipCode::LowUsage, "usage 4% left (threshold 5%)".into()))
        );
        assert_eq!(
            account_skip(
                claude,
                Some(&[account(true, Some(2)), account(true, Some(5))]),
                5
            ),
            None
        );
        assert_eq!(
            account_skip(
                claude,
                Some(&[account(true, Some(2)), account(true, None)]),
                5
            ),
            None
        );
    }

    #[test]
    fn select_takes_the_first_runner_the_probe_passes_and_lists_each_skip() {
        let profile = Profile {
            name: "code.complex".into(),
            runners: vec![
                runner(Provider::Claude, "opus"),
                runner(Provider::Codex, "gpt-6.1-sol"),
                runner(Provider::Agy, "flash"),
            ],
        };
        let no_claude = |runner: &Runner| {
            (runner.provider == Provider::Claude)
                .then(|| (SkipCode::LowUsage, "usage 2% left".into()))
        };
        let selection = select(&profile, None, no_claude);
        assert_eq!(selection.pick, Some(1));
        assert_eq!(
            selection.skipped,
            [Skip {
                index: 0,
                code: SkipCode::LowUsage,
                text: "usage 2% left".into()
            }]
        );
        assert_eq!(
            select(&profile, Some(Provider::Agy), no_claude).pick,
            Some(2)
        );
        let nothing = select(&profile, None, |_| {
            Some((SkipCode::CliMissing, "gone".into()))
        });
        assert_eq!(nothing.pick, None);
        assert_eq!(
            nothing
                .skipped
                .iter()
                .map(|skip| skip.index)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
    }

    #[test]
    fn a_one_off_chat_takes_the_chat_profiles_runner_of_that_provider_or_the_defaults() {
        let mut codex = runner(Provider::Codex, "gpt-6.1-sol");
        codex.effort = "xhigh".into();
        codex.sandbox = Some("read-only".into());
        let chat = Profile {
            name: "chat".into(),
            runners: vec![runner(Provider::Claude, "opus"), codex],
        };
        let picked = one_off(&chat, Provider::Codex, "gpt-6-luna").unwrap();
        assert_eq!(
            (
                picked.model.as_str(),
                picked.effort.as_str(),
                picked.sandbox.as_deref()
            ),
            ("gpt-6-luna", "xhigh", Some("read-only"))
        );
        let agy = one_off(&chat, Provider::Agy, "flash").unwrap();
        assert_eq!((agy.effort.as_str(), agy.permission), ("medium", None));
        let claude_only = Profile {
            name: "chat".into(),
            runners: vec![runner(Provider::Claude, "opus")],
        };
        let fresh = one_off(&claude_only, Provider::Codex, "gpt-6-luna").unwrap();
        assert_eq!(fresh.sandbox.as_deref(), Some("workspace-write"));
        assert!(one_off(&chat, Provider::Claude, "--bad").is_err());
    }
}
