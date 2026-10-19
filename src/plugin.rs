//! `swarm setup plugin`: the commands that install the kit as the plugin `swarm` in each agent CLI
//! (ADR 0090). Every CLI installs from the link `~/.swarm/kit`, so a new kit source only moves the
//! link. Claude and pi read the link in place; Codex and AGY copy it again on install.
//! Claude update syncs its version record when the manifest version changes.
//! Codex stores the resolved folder of a marketplace and refuses a second folder under the same
//! name, so a Codex home whose `swarm` marketplace holds another folder removes it first.

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
                steps.push(Step::new(
                    &[
                        "claude",
                        "plugin",
                        "update",
                        "swarm@swarm",
                        "--scope",
                        "user",
                    ],
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

fn read_json(path: &Path) -> Option<serde_json::Value> {
    use std::io::Read;
    // These small local records must not turn a launch notice into an unbounded read.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    serde_json::from_reader(file.take(1024 * 1024)).ok()
}

pub fn kit_version(kit: &Path) -> Option<String> {
    read_json(&kit.join("plugin.json"))?["version"]
        .as_str()
        .filter(|version| !version.is_empty())
        .map(str::to_owned)
}

#[derive(Debug, PartialEq)]
pub struct StaleCopy {
    pub cli: &'static str,
    pub home: PathBuf,
    pub installed: String,
    pub expected: String,
}

impl StaleCopy {
    pub fn label(&self) -> String {
        format!("{} ({})", self.cli, self.home.display())
    }

    pub fn reason(&self) -> String {
        format!(
            "Plugin {} differs from kit {}; run `swarm setup plugin`",
            self.installed, self.expected
        )
    }
}

/// Read only local plugin records. Versions identify content, so compare the whole string,
/// including build metadata. Old cache folders do not make a current copy stale.
/// Skip CLIs absent from `found`. Claude and pi read the kit link directly; a Claude
/// marketplace at another path can still have a stale installed version.
pub fn stale_copies(
    kit: &Path,
    link: &Path,
    found: &[&str],
    claude_home: &Path,
    codex_homes: &[PathBuf],
    user_home: &Path,
) -> Vec<StaleCopy> {
    let Some(expected) = kit_version(kit) else {
        return Vec::new();
    };
    let mut stale = Vec::new();
    let mut compare = |cli, home: &Path, installed: String| {
        if installed != expected {
            stale.push(StaleCopy {
                cli,
                home: home.to_path_buf(),
                installed,
                expected: expected.clone(),
            });
        }
    };
    // Compare the lexical link, since resolving it would hide a package move.
    let claude_reads_link = found.contains(&"claude")
        && read_json(&claude_home.join("plugins/known_marketplaces.json")).is_some_and(|markets| {
            markets["swarm"]["source"]["source"] == "directory"
                && markets["swarm"]["source"]["path"].as_str().map(Path::new) == Some(link)
        });
    if found.contains(&"claude")
        && !claude_reads_link
        && let Some(index) = read_json(&claude_home.join("plugins/installed_plugins.json"))
        && let Some(installs) = index["plugins"]["swarm@swarm"].as_array()
    {
        for install in installs.iter().filter(|install| install["scope"] == "user") {
            if let Some(version) = install["version"].as_str() {
                compare("Claude", claude_home, version.to_string());
            }
        }
    }
    for home in codex_homes.iter().filter(|_| found.contains(&"codex")) {
        let Ok(entries) = std::fs::read_dir(home.join("plugins/cache/swarm/swarm")) else {
            continue;
        };
        let mut versions: Vec<_> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| kit_version(&entry.path()))
            .collect();
        versions.sort();
        versions.dedup();
        // Codex resolves the current marketplace version to this cache path. There is no
        // active-version field in config.toml; plugin add fills that version's cache.
        if !versions.is_empty() && !versions.contains(&expected) {
            compare("Codex", home, versions.join(", "));
        }
    }
    if found.contains(&"agy")
        && let Some(version) = kit_version(&user_home.join(".gemini/config/plugins/swarm"))
    {
        compare("AGY", user_home, version);
    }
    stale
}

/// One line, even when several CLIs or Codex homes need a refresh.
pub fn stale_notice(copies: &[StaleCopy]) -> Option<String> {
    if copies.is_empty() {
        return None;
    }
    let mut names: Vec<_> = copies.iter().map(|copy| copy.cli).collect();
    names.dedup();
    Some(format!(
        "swarm: stale plugin copies in {}; run `swarm setup plugin`",
        names.join(", ")
    ))
}

/// Each skill in the owner's skill folders that has the name of a kit skill. The plugin leaves
/// them, but a CLI then lists both, and pi, which has no plugin prefix, loads only one. `active`
/// adds the skill folders of homes picked by `CLAUDE_CONFIG_DIR` or `PI_CODING_AGENT_DIR`.
pub fn clashes(kit: &Path, user_home: &Path, active: &[PathBuf]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(kit.join("skills")) else {
        return Vec::new();
    };
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .map(|entry| entry.file_name())
        .collect();
    names.sort();
    let mut folders: Vec<PathBuf> = SKILL_FOLDERS
        .iter()
        .map(|folder| user_home.join(folder))
        .collect();
    for folder in active {
        if !folders.contains(folder) {
            folders.push(folder.clone());
        }
    }
    folders
        .iter()
        .flat_map(|folder| names.iter().map(move |name| folder.join(name)))
        .filter(|path| path.symlink_metadata().is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clashes_also_read_the_active_skill_folders() {
        let root = std::env::temp_dir().join(format!("swarm-clash-active-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let kit = root.join("kit");
        std::fs::create_dir_all(kit.join("skills/swarm-voice")).unwrap();
        std::fs::write(kit.join("skills/swarm-voice/SKILL.md"), "x").unwrap();
        let home = root.join("home");
        let work = root.join("claude-work/skills");
        std::fs::create_dir_all(work.join("swarm-voice")).unwrap();
        std::fs::create_dir_all(home.join(".pi/agent/skills/swarm-voice")).unwrap();
        assert_eq!(
            clashes(&kit, &home, &[]),
            [home.join(".pi/agent/skills/swarm-voice")]
        );
        assert_eq!(
            clashes(
                &kit,
                &home,
                &[work.clone(), work.clone(), home.join(".pi/agent/skills")]
            ),
            [
                home.join(".pi/agent/skills/swarm-voice"),
                work.join("swarm-voice")
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn versions_compare_content_ids_and_ignore_unused_old_caches() {
        let root = std::env::temp_dir().join(format!("swarm-copy-versions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let kit = root.join("kit");
        let claude = root.join(".claude");
        let codex = root.join(".codex");
        let manifest = |folder: &Path, version: &str| {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::write(
                folder.join("plugin.json"),
                format!(r#"{{"version":"{version}"}}"#),
            )
            .unwrap();
        };
        let read_stale = |homes: &[PathBuf]| {
            stale_copies(&kit, &root.join(".swarm/kit"), &CLIS, &claude, homes, &root)
        };
        manifest(&kit, "0.1.0+new");
        assert!(read_stale(std::slice::from_ref(&codex)).is_empty());
        let cache = codex.join("plugins/cache/swarm/swarm");
        manifest(&cache.join("0.1.0+old"), "0.1.0+old");
        let copies = read_stale(std::slice::from_ref(&codex));
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].cli, "Codex");
        assert_eq!(copies[0].installed, "0.1.0+old");
        manifest(&cache.join("0.1.0+new"), "0.1.0+new");
        assert!(read_stale(std::slice::from_ref(&codex)).is_empty());

        std::fs::create_dir_all(claude.join("plugins")).unwrap();
        let index = claude.join("plugins/installed_plugins.json");
        std::fs::write(
            &index,
            r#"{"plugins":{"swarm@swarm":[{"scope":"user","version":"0.1.0+new"}]}}"#,
        )
        .unwrap();
        manifest(&claude.join("plugins/cache/swarm/swarm/old"), "0.1.0+old");
        assert!(read_stale(&[]).is_empty());
        std::fs::write(&index, "invalid JSON").unwrap();
        assert!(read_stale(&[]).is_empty());
        // An unstamped checkout still compares its ordinary manifest version.
        manifest(&kit, "0.1.0");
        manifest(&root.join(".gemini/config/plugins/swarm"), "0.1.0");
        assert!(read_stale(&[]).is_empty());
        manifest(&root.join(".gemini/config/plugins/swarm"), "0.0.9");
        let copies = read_stale(&[]);
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].cli, "AGY");
        assert!(stale_notice(&[]).is_none());
        assert_eq!(
            stale_notice(&copies).unwrap(),
            "swarm: stale plugin copies in AGY; run `swarm setup plugin`"
        );
        std::fs::remove_file(kit.join("plugin.json")).unwrap();
        assert!(read_stale(&[]).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

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
            &["pi", "agy", "codex", "claude"],
            &codex,
        );
        assert_eq!(
            lines(&steps),
            [
                "'claude' 'plugin' 'marketplace' 'add' '/h/.swarm/kit'",
                "'claude' 'plugin' 'install' 'swarm@swarm'",
                "'claude' 'plugin' 'update' 'swarm@swarm' '--scope' 'user'",
                "CODEX_HOME='/h/.codex' 'codex' 'plugin' 'marketplace' 'add' '/h/.swarm/kit'",
                "CODEX_HOME='/h/.codex' 'codex' 'plugin' 'add' 'swarm@swarm'",
                "'agy' 'plugin' 'install' '/h/.swarm/kit'",
                "'pi' 'install' '/h/.swarm/kit'",
            ]
        );
        assert!(super::steps(Path::new("/l"), Path::new("/kit"), &[], &[]).is_empty());
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
