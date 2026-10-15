//! `swarm setup plugin`: the commands that install the kit as the plugin `swarm` in each agent CLI
//! (ADR 0090). Every CLI installs from the link `~/.swarm/kit`, so a new kit source only moves the
//! link: Claude and pi read the kit through it, and AGY copies it again on install. Codex stores
//! the resolved folder of a marketplace and refuses a second folder under the same name, so a Codex
//! home whose `swarm` marketplace holds another folder removes it first.

use std::path::{Path, PathBuf};

/// The CLIs that get the plugin, in plan order.
pub const CLIS: [&str; 4] = ["claude", "codex", "agy", "pi"];

/// The skill folders where an owner's own copy of a kit skill would sit beside the plugin's.
const SKILL_FOLDERS: [&str; 4] = [
    ".claude/skills",
    ".agents/skills",
    ".gemini/config/skills",
    ".pi/agent/skills",
];

/// One command of the plan. `codex_home` sets `CODEX_HOME` for it.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub argv: Vec<String>,
    pub codex_home: Option<PathBuf>,
}

impl Step {
    fn new(argv: &[&str], codex_home: Option<&Path>) -> Self {
        Self {
            argv: argv.iter().map(|arg| arg.to_string()).collect(),
            codex_home: codex_home.map(Path::to_path_buf),
        }
    }

    /// The command as the owner would type it.
    pub fn line(&self) -> String {
        let command = crate::adapter::shell_line(&self.argv);
        match &self.codex_home {
            Some(home) => format!(
                "CODEX_HOME={} {command}",
                crate::adapter::shell_line(&[home.to_string_lossy().into()])
            ),
            None => command,
        }
    }
}

/// A Codex home and the folder of the `swarm` marketplace it has, if any.
pub struct CodexHome {
    pub home: PathBuf,
    pub swarm_root: Option<PathBuf>,
}

/// The install commands for each CLI in `found`, in `CLIS` order. `kit` is the resolved kit folder
/// that `link` points to. Each command is safe to run again, so a second run updates the copies
/// that Codex and AGY keep.
pub fn steps(link: &Path, kit: &Path, found: &[&str], codex_homes: &[CodexHome]) -> Vec<Step> {
    let link = &*link.to_string_lossy();
    let mut steps = Vec::new();
    for cli in CLIS.iter().filter(|cli| found.contains(cli)) {
        match *cli {
            "claude" => {
                steps.push(Step::new(
                    &["claude", "plugin", "marketplace", "add", link],
                    None,
                ));
                steps.push(Step::new(
                    &["claude", "plugin", "install", "swarm@swarm"],
                    None,
                ));
            }
            "codex" => {
                for codex in codex_homes {
                    let home = Some(codex.home.as_path());
                    if codex.swarm_root.as_deref().is_some_and(|root| root != kit) {
                        steps.push(Step::new(
                            &["codex", "plugin", "marketplace", "remove", "swarm"],
                            home,
                        ));
                    }
                    steps.push(Step::new(
                        &["codex", "plugin", "marketplace", "add", link],
                        home,
                    ));
                    steps.push(Step::new(&["codex", "plugin", "add", "swarm@swarm"], home));
                }
            }
            "agy" => steps.push(Step::new(&["agy", "plugin", "install", link], None)),
            _ => steps.push(Step::new(&["pi", "install", link], None)),
        }
    }
    steps
}

/// The path the link `~/.swarm/kit` names for the resolved `kit`. Homebrew deletes
/// `<prefix>/Cellar/swarm/<version>` on upgrade, and `<prefix>/opt/swarm` always names the current
/// version, so a kit in the Cellar is named through `opt` when that resolves to the same folder.
pub fn stable_kit(kit: &Path) -> PathBuf {
    let parts: Vec<_> = kit.iter().collect();
    let Some(cellar) = parts
        .windows(2)
        .position(|pair| pair[0] == "Cellar" && pair[1] == "swarm")
    else {
        return kit.to_path_buf();
    };
    let opt: PathBuf = parts[..cellar]
        .iter()
        .copied()
        .chain(["opt".as_ref(), "swarm".as_ref()])
        .chain(parts.iter().skip(cellar + 3).copied())
        .collect();
    if std::fs::canonicalize(&opt).is_ok_and(|resolved| resolved == kit) {
        opt
    } else {
        kit.to_path_buf()
    }
}

/// The root of the `swarm` marketplace in the output of `codex plugin marketplace list --json`.
pub fn codex_swarm_root(json: &[u8]) -> Option<PathBuf> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    value["marketplaces"]
        .as_array()?
        .iter()
        .find(|marketplace| marketplace["name"] == "swarm")?["root"]
        .as_str()
        .map(PathBuf::from)
}

/// Each skill in the owner's skill folders that has the name of a kit skill. The plugin leaves
/// them, but a CLI then lists both, and pi, which has no plugin prefix, loads only one.
pub fn clashes(kit: &Path, user_home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(kit.join("skills")) else {
        return Vec::new();
    };
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .map(|entry| entry.file_name())
        .collect();
    names.sort();
    SKILL_FOLDERS
        .iter()
        .flat_map(|folder| {
            names
                .iter()
                .map(move |name| user_home.join(folder).join(name))
        })
        .filter(|path| path.symlink_metadata().is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(steps: &[Step]) -> Vec<String> {
        steps.iter().map(Step::line).collect()
    }

    #[test]
    fn plans_each_found_cli_in_order_and_skips_the_rest() {
        let codex = [CodexHome {
            home: "/h/.codex".into(),
            swarm_root: None,
        }];
        let steps = steps(
            Path::new("/h/.swarm/kit"),
            Path::new("/kit"),
            &["pi", "codex", "claude"],
            &codex,
        );
        assert_eq!(
            lines(&steps),
            [
                "'claude' 'plugin' 'marketplace' 'add' '/h/.swarm/kit'",
                "'claude' 'plugin' 'install' 'swarm@swarm'",
                "CODEX_HOME='/h/.codex' 'codex' 'plugin' 'marketplace' 'add' '/h/.swarm/kit'",
                "CODEX_HOME='/h/.codex' 'codex' 'plugin' 'add' 'swarm@swarm'",
                "'pi' 'install' '/h/.swarm/kit'",
            ]
        );
    }

    #[test]
    fn a_codex_home_with_another_swarm_folder_removes_it_first() {
        let codex = [
            CodexHome {
                home: "/h/.codex".into(),
                swarm_root: Some("/kit".into()),
            },
            CodexHome {
                home: "/h/.codex-work".into(),
                swarm_root: Some("/old/kit".into()),
            },
        ];
        let steps = steps(Path::new("/l"), Path::new("/kit"), &["codex"], &codex);
        let removes: Vec<_> = steps
            .iter()
            .filter(|step| step.argv.contains(&"remove".to_string()))
            .map(|step| step.codex_home.clone().unwrap())
            .collect();
        assert_eq!(removes, [PathBuf::from("/h/.codex-work")]);
        assert_eq!(steps.len(), 5);
    }

    #[test]
    fn a_cellar_kit_is_named_through_opt() {
        let root = std::env::temp_dir().join(format!("swarm-stable-kit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cellar = root.join("Cellar/swarm/0.0.11/libexec/skills/kit");
        std::fs::create_dir_all(&cellar).unwrap();
        std::fs::create_dir_all(root.join("opt")).unwrap();
        std::os::unix::fs::symlink("../Cellar/swarm/0.0.11", root.join("opt/swarm")).unwrap();
        let kit = std::fs::canonicalize(&cellar).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        assert_eq!(stable_kit(&kit), root.join("opt/swarm/libexec/skills/kit"));
        // opt names another version, so the Cellar path stays.
        std::fs::remove_file(root.join("opt/swarm")).unwrap();
        std::fs::create_dir_all(root.join("Cellar/swarm/0.0.12/libexec/skills/kit")).unwrap();
        std::os::unix::fs::symlink("../Cellar/swarm/0.0.12", root.join("opt/swarm")).unwrap();
        assert_eq!(stable_kit(&kit), kit);
        // A kit outside a Cellar stays as it is.
        assert_eq!(
            stable_kit(Path::new("/x/libexec/skills/kit")),
            Path::new("/x/libexec/skills/kit")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reads_the_swarm_root_from_codex_json() {
        let json = br#"{"marketplaces": [{"name": "other", "root": "/o"}, {"name": "swarm", "root": "/kit"}]}"#;
        assert_eq!(codex_swarm_root(json), Some("/kit".into()));
        assert_eq!(codex_swarm_root(br#"{"marketplaces": []}"#), None);
        assert_eq!(codex_swarm_root(b"not json"), None);
    }
}
