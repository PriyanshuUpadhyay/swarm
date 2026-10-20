//! The commands and skills a chat's CLI offers, for the web composer's `/` menu.
//! Claude, Codex and pi each list what they load, so swarm asks them and copies none of their
//! loading rules. AGY has no list request, so its skills are read from disk as the old SwiftUI
//! menu did (`58beca2a^:ui/Sources/SwarmCore/Composer/ComposerCommands.swift`).

use crate::providers::Provider;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Row {
    pub name: String,
    /// The exact text a pick puts in the draft, such as `/clear`, `$ponytail:ponytail` or
    /// `/skill:research`.
    pub insert: String,
    pub kind: Kind,
    pub description: String,
    pub hint: String,
    /// The plugin, `user`, `project` or `built-in` that the row comes from, when known.
    pub source: String,
    /// The command opens a picker or a panel in the CLI. The web pane shows only the transcript,
    /// so the next Enter would choose in a picker the owner cannot see. The menu inserts such a
    /// row and never sends it.
    pub dialog: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Builtin,
    Command,
    Skill,
}

/// The rows, and a note when the CLI's own list could not be read and only built-ins show.
#[derive(Debug, Serialize)]
pub struct List {
    pub commands: Vec<Row>,
    pub note: Option<String>,
}

/// Lists what `provider` offers in `cwd`, run with `env` (the chat's account variables, with every
/// key of `Provider::account_env_keys` cleared first). A failed probe keeps the built-ins.
pub fn list(
    provider: Provider,
    env: &BTreeMap<String, String>,
    cwd: &Path,
    deadline: Instant,
) -> List {
    let read = match provider {
        Provider::Claude => claude(env, cwd, deadline),
        Provider::Codex => codex(env, cwd, deadline),
        Provider::Pi => pi(cwd, deadline),
        Provider::Agy => std::env::var_os("HOME")
            .map(|home| agy(Path::new(&home), cwd))
            .ok_or_else(|| "HOME is not set".into()),
    };
    let (rows, note) = match read {
        Ok(rows) => (rows, None),
        Err(error) => (
            Vec::new(),
            Some(format!("{} list unavailable: {error}", provider.id())),
        ),
    };
    List {
        commands: finish(provider, rows),
        note,
    }
}

/// Adds the built-ins the CLI did not list, marks dialog rows, and drops internal names.
fn finish(provider: Provider, mut rows: Vec<Row>) -> Vec<Row> {
    let listed: BTreeSet<String> = rows.iter().map(|row| row.insert.clone()).collect();
    rows.extend(
        builtins(provider)
            .iter()
            .filter(|(name, _)| !listed.contains(&format!("/{name}")))
            .map(|(name, description)| Row {
                name: (*name).into(),
                insert: format!("/{name}"),
                kind: Kind::Builtin,
                description: (*description).into(),
                hint: String::new(),
                source: "built-in".into(),
                dialog: false,
            }),
    );
    let dialogs = dialogs(provider);
    rows.retain(|row| !row.name.starts_with("__"));
    for row in &mut rows {
        row.dialog = row.insert.starts_with('/') && dialogs.contains(&row.name.as_str());
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows.dedup_by(|a, b| a.insert == b.insert);
    rows
}

mod catalog;
mod probes;

use catalog::{builtins, dialogs};
use probes::{agy, claude, codex, pi};

#[cfg(test)]
use probes::{claude_rows, codex_rows, pi_rows, skills};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_rows_keep_the_cli_name_and_move_the_source_out_of_the_text() {
        let rows = claude_rows(&json!([
            {"name":"clear","description":"Clear the chat","argumentHint":"[name]","builtin":true},
            {"name":"superset:automation","description":"(superset) Manage one automation","argumentHint":"<id>"},
            {"name":"research","description":"Research one question. (user)","argumentHint":""},
        ]));
        assert_eq!(rows[0].kind, Kind::Builtin);
        assert_eq!(
            (rows[0].insert.as_str(), rows[0].hint.as_str()),
            ("/clear", "[name]")
        );
        assert_eq!(
            (rows[1].source.as_str(), rows[1].description.as_str()),
            ("superset", "Manage one automation")
        );
        assert_eq!(
            (rows[2].source.as_str(), rows[2].description.as_str()),
            ("user", "Research one question.")
        );
    }

    #[test]
    fn codex_skills_are_called_with_a_dollar_and_disabled_ones_are_hidden() {
        let rows = codex_rows(&json!({"data":[{"cwd":"/w","skills":[
            {"name":"ponytail:ponytail","description":"Lazy","scope":"user","enabled":true},
            {"name":"research","description":"R","scope":"repo","enabled":true},
            {"name":"off","description":"O","scope":"user","enabled":false},
        ]}]}));
        let inserts: Vec<_> = rows.iter().map(|row| row.insert.as_str()).collect();
        assert_eq!(inserts, ["$ponytail:ponytail", "$research"]);
        assert_eq!(rows[0].source, "ponytail");
    }

    #[test]
    fn pi_skills_keep_their_skill_prefix() {
        let rows = pi_rows(&json!([
            {"name":"skill:research","description":"R","source":"skill"},
            {"name":"llama","description":"L","source":"extension"},
        ]));
        assert_eq!(
            (rows[0].insert.as_str(), rows[0].kind),
            ("/skill:research", Kind::Skill)
        );
        assert_eq!(rows[1].kind, Kind::Command);
    }

    #[test]
    fn skill_paths_set_the_source_and_hide_model_only_skills_for_every_provider() {
        let root = std::env::temp_dir().join(format!("swarm-menu-{}", uuid::Uuid::now_v7()));
        let kit = root.join(".swarm/kit");
        let visible = kit.join("skills/plan/SKILL.md");
        let hidden = kit.join("skills/decisions/SKILL.md");
        let other = root.join("other/SKILL.md");
        for path in [&visible, &hidden, &other] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "---\ndescription: Test\n---\n").unwrap();
        }
        std::fs::write(kit.join("plugin.json"), r#"{"name":"swarm"}"#).unwrap();
        std::fs::write(&hidden, "---\nuser-invocable: false\n---\n").unwrap();
        let codex = codex_rows(&json!({"data":[{"skills":[
            {"name":"plain-name","path":visible,"scope":"user"},
            {"name":"hidden","path":hidden,"scope":"repo"},
            {"name":"swarm:unrelated","path":other,"scope":"user"}
        ]}]}));
        assert_eq!(
            codex.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["plain-name", "swarm:unrelated"]
        );
        assert_eq!(codex[0].source, "swarm");
        let pi = pi_rows(&json!([
            {"name":"skill:plan","source":"skill","sourceInfo":{"path":visible}},
            {"name":"skill:decisions","source":"skill","sourceInfo":{"path":hidden}},
            {"name":"skill:swarm","source":"skill","sourceInfo":{"path":other}},
            {"name":"skill:no-path","source":"skill"},
            {"name":"extension","source":"extension","sourceInfo":{"path":hidden}}
        ]));
        assert_eq!(
            pi.iter().map(|r| r.source.as_str()).collect::<Vec<_>>(),
            ["swarm", "skill", "skill", "extension"]
        );
        let claude = claude_rows(&json!([
            {"name":"swarm:plan","description":"(swarm) Plan","argumentHint":""}
        ]));
        assert_eq!(claude[0].source, "swarm");
        let agy = skills(&kit.join("skills"), None);
        assert_eq!(agy.len(), 1);
        assert_eq!(agy[0].source, "swarm");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_a_top_level_false_hides_a_skill() {
        let root = std::env::temp_dir().join(format!("swarm-menu-flag-{}", uuid::Uuid::now_v7()));
        let cases = [
            ("---\nmetadata:\n  user-invocable: false\n---\n", true),
            ("---\nuser-invocable: False\n---\n", false),
            ("---\nuser-invocable: false # model only\n---\n", false),
            ("---\nuser-invocable: true\n---\n", true),
        ];
        for (i, (text, shown)) in cases.into_iter().enumerate() {
            let file = root.join(format!("{i}/SKILL.md"));
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, text).unwrap();
            let rows =
                pi_rows(&json!([{"name":"skill:x","source":"skill","sourceInfo":{"path":file}}]));
            assert_eq!(rows.len() == 1, shown, "{text}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plugin_skill_paths_follow_links_and_use_the_manifest_not_the_folder_name() {
        let root = std::env::temp_dir().join(format!("swarm-menu-plugin-{}", uuid::Uuid::now_v7()));
        let plugin = root.join("cache/some-market/swarm/1.0");
        let file = plugin.join("skills/plan/SKILL.md");
        let link = root.join("linked/SKILL.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(plugin.join(".claude-plugin")).unwrap();
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&file, "---\ndescription: Plan\n---\n").unwrap();
        std::fs::write(
            plugin.join(".claude-plugin/plugin.json"),
            r#"{"name":"swarm"}"#,
        )
        .unwrap();
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let rows = pi_rows(&json!([
            {"name":"skill:plan","source":"skill","sourceInfo":{"path":link}}
        ]));
        assert_eq!(rows[0].source, "swarm");
        std::fs::write(
            plugin.join(".claude-plugin/plugin.json"),
            r#"{"name":"other"}"#,
        )
        .unwrap();
        let rows = pi_rows(&json!([
            {"name":"skill:plan","source":"skill","sourceInfo":{"path":file}}
        ]));
        assert_eq!(rows[0].source, "skill");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_adds_missing_builtins_marks_dialogs_and_drops_internal_names() {
        let listed = claude_rows(&json!([
            {"name":"model","description":"Set the model","argumentHint":"<model>","builtin":true},
            {"name":"__remote-workflow","description":"internal","builtin":true},
        ]));
        let rows = finish(Provider::Claude, listed);
        let model: Vec<_> = rows.iter().filter(|row| row.name == "model").collect();
        assert_eq!(model.len(), 1, "a listed built-in is not added twice");
        assert_eq!(model[0].description, "Set the model");
        assert!(model[0].dialog);
        assert!(rows.iter().any(|row| row.name == "resume" && row.dialog));
        assert!(rows.iter().any(|row| row.name == "clear" && !row.dialog));
        assert!(!rows.iter().any(|row| row.name.starts_with("__")));
        let codex = finish(Provider::Codex, Vec::new());
        assert!(
            !codex.iter().any(|row| row.name == "new"),
            "Codex /new opens a worktree picker"
        );
        let pi = finish(Provider::Pi, Vec::new());
        assert!(pi.iter().any(|row| row.name == "compact" && !row.dialog));
        assert!(pi.iter().any(|row| row.name == "model" && row.dialog));
        for name in ["quit", "new", "resume"] {
            assert!(!pi.iter().any(|row| row.name == name), "{name} is left out");
        }
    }

    #[test]
    fn a_codex_skill_named_like_a_dialog_is_not_a_dialog() {
        let rows = finish(
            Provider::Codex,
            codex_rows(&json!({"data":[{"skills":[
            {"name":"model","description":"a skill","enabled":true}]}]})),
        );
        assert!(rows.iter().any(|row| row.insert == "$model" && !row.dialog));
        assert!(rows.iter().any(|row| row.insert == "/model" && row.dialog));
    }

    #[test]
    fn agy_reads_config_plugin_and_workspace_skills_and_hides_disabled_ones() {
        let root = std::env::temp_dir().join(format!("swarm-agy-skills-{}", uuid::Uuid::now_v7()));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "home/.gemini/config/skills/plain/SKILL.md",
            "---\nname: plain\ndescription: >\n  One\n  line\n---\n",
        );
        write(
            "home/.gemini/config/skills/hidden/SKILL.md",
            "---\ndisable-slash-command: true\n---\n",
        );
        write(
            "home/.gemini/config/plugins/swarm/skills/plan/SKILL.md",
            "---\ndescription: \"Plan it\"\n---\n",
        );
        write("repo/.git/HEAD", "");
        write("repo/.agents/skills/local/SKILL.md", "no front matter\n");
        std::fs::create_dir_all(root.join("repo/sub")).unwrap();
        let rows = agy(&root.join("home"), &root.join("repo/sub"));
        let names: Vec<_> = rows
            .iter()
            .map(|row| (row.insert.as_str(), row.description.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("/plain", "One line"),
                ("/swarm:plan", "Plan it"),
                ("/local", "")
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
