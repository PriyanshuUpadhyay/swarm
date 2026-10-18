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

/// Commands the CLI runs in its TUI but does not list in its machine mode, from the SwiftUI app's
/// checked lists. Codex has no `/new`: it opens a worktree picker. pi's are its documented built-ins
/// (docs/slash-commands.md), which `get_commands` leaves out. `/new`, `/clone`, `/fork`, `/resume`,
/// and `/import` stay out because they move pi to a new session file, and swarm follows the chair's
/// log by its fixed id (ADR 0086); `/quit`, `/share`, and `/bug` end the chair or upload the session;
/// `/changelog`, `/hotkeys`, and `/session` write only to pi's own screen.
fn builtins(provider: Provider) -> &'static [(&'static str, &'static str)] {
    match provider {
        Provider::Claude => &[
            ("add-dir", "Add a working folder"),
            ("agents", "Manage agents"),
            ("clear", "Start a new conversation"),
            ("compact", "Compact the current conversation"),
            ("config", "Open settings"),
            ("context", "Show context use"),
            ("cost", "Show token use and cost"),
            ("diff", "Show the current changes"),
            ("export", "Export the conversation"),
            ("help", "Show help"),
            ("hooks", "Show hook settings"),
            ("init", "Create CLAUDE.md for this repository"),
            ("mcp", "Manage MCP servers"),
            ("memory", "Edit memory files"),
            ("model", "Choose the model"),
            ("permissions", "Change tool permissions"),
            ("plan", "Start plan mode"),
            ("plugin", "Manage plugins"),
            ("resume", "Resume an earlier conversation"),
            ("review", "Review the current changes"),
            ("rewind", "Rewind the conversation"),
            ("skills", "List skills"),
            ("status", "Show session status"),
            ("usage", "Show plan usage"),
        ],
        Provider::Codex => &[
            ("agents", "Manage agents"),
            ("clear", "Start a new conversation"),
            ("compact", "Compact the current conversation"),
            ("diff", "Show the current changes"),
            ("export", "Export the conversation"),
            ("fork", "Fork the conversation"),
            ("init", "Create agent instructions for this repository"),
            ("mcp", "Manage MCP servers"),
            ("mention", "Add a file to the conversation"),
            ("model", "Choose the model"),
            ("permissions", "Change approval permissions"),
            ("plan", "Start plan mode"),
            ("rename", "Rename the conversation"),
            ("resume", "Resume an earlier conversation"),
            ("review", "Review the current changes"),
            ("skills", "List skills"),
            ("status", "Show session status"),
            ("usage", "Show plan usage"),
        ],
        Provider::Agy => &[
            ("add-dir", "Add a working folder"),
            ("agents", "Manage agents"),
            ("btw", "Ask a side question"),
            ("clear", "Start a new conversation"),
            ("config", "Open settings"),
            ("context", "Show context use"),
            ("copy", "Copy the last answer"),
            ("diff", "Show the current changes"),
            ("effort", "Choose the reasoning effort"),
            ("fork", "Fork the conversation"),
            ("goal", "Set a goal"),
            ("help", "Show help"),
            ("hooks", "Show hook settings"),
            ("mcp", "Manage MCP servers"),
            ("model", "Choose the model"),
            ("new", "Start a new conversation"),
            ("permissions", "Change tool permissions"),
            ("plan", "Start plan mode"),
            ("plugin", "Manage plugins"),
            ("rename", "Rename the conversation"),
            ("resume", "Resume an earlier conversation"),
            ("rewind", "Rewind the conversation"),
            ("skills", "List skills"),
            ("tasks", "Show background tasks"),
            ("usage", "Show plan usage"),
        ],
        Provider::Pi => &[
            ("compact", "Compact the current context"),
            ("copy", "Copy the last answer"),
            ("export", "Export the session as HTML or JSONL"),
            ("login", "Add provider authentication"),
            ("logout", "Remove provider authentication"),
            ("model", "Choose the model"),
            ("name", "Set the session name"),
            ("reload", "Reload extensions, skills, and context files"),
            ("scoped-models", "Choose the models to cycle"),
            ("settings", "Open settings"),
            ("thinking", "Choose the thinking level"),
            ("tree", "Move within the session tree"),
            ("trust", "Save a project trust decision"),
        ],
    }
}

/// Commands that open a picker or a panel in the CLI's TUI when typed with no argument.
fn dialogs(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Claude => &[
            "add-dir",
            "agents",
            "config",
            "effort",
            "export",
            "help",
            "hooks",
            "ide",
            "login",
            "logout",
            "mcp",
            "memory",
            "model",
            "output-style",
            "permissions",
            "plugin",
            "resume",
            "rewind",
            "skills",
            "status",
            "theme",
            "usage",
        ],
        Provider::Codex => &[
            "agents",
            "mention",
            "model",
            "permissions",
            "rename",
            "resume",
            "review",
            "skills",
        ],
        Provider::Agy => &[
            "agents",
            "config",
            "effort",
            "help",
            "hooks",
            "mcp",
            "model",
            "permissions",
            "plugin",
            "resume",
            "rewind",
            "skills",
            "tasks",
        ],
        Provider::Pi => &[
            "login",
            "logout",
            "model",
            "scoped-models",
            "settings",
            "thinking",
            "tree",
            "trust",
        ],
    }
}

/// Starts `program`, writes `request` as one line, and returns the first stdout line that `done`
/// accepts. The child is killed and reaped on every path.
fn ask(
    mut command: Command,
    request: &Value,
    deadline: Instant,
    done: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("{program} did not start: {error}"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let (sender, lines) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    // stdin stays open until the answer comes: a CLI in stream mode exits when its input ends.
    let mut stdin = child.stdin.take().expect("piped stdin");
    let result = writeln!(stdin, "{request}")
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("{program} request failed: {error}"))
        .and_then(|_| {
            loop {
                let line = lines
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| format!("{program} did not answer in time"))?
                    .map_err(|error| format!("{program} output failed: {error}"))?;
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && done(&value)
                {
                    break Ok(value);
                }
            }
        });
    // Closing stdin lets the CLI exit on its own; a kill at once can leave a lock behind, such as
    // pi's `auth.json.lock`, which makes every later pi wait up to 30 s.
    drop(stdin);
    let grace = Instant::now() + std::time::Duration::from_secs(1);
    while Instant::now() < grace && matches!(child.try_wait(), Ok(None)) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// Claude's `initialize` control request answers with the session's commands before any prompt,
/// so no model turn starts. Hooks and MCP servers stay off, as t3code's capability probe does
/// (pingdotgg/t3code `apps/server/src/provider/ClaudeProvider.ts`), so the probe runs no
/// SessionStart hook of the owner's.
fn claude(
    env: &BTreeMap<String, String>,
    cwd: &Path,
    deadline: Instant,
) -> Result<Vec<Row>, String> {
    let executable = std::env::var("SWARM_CLAUDE_CMD").unwrap_or_else(|_| "claude".into());
    let mut command = Command::new(executable);
    command.args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--settings",
        r#"{"disableAllHooks":true}"#,
    ]);
    // Claude refuses --strict-mcp-config beside an administrator's managed MCP file.
    if !Path::new("/Library/Application Support/ClaudeCode/managed-mcp.json").exists() {
        command.args([
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
        ]);
    }
    for key in Provider::Claude.account_env_keys() {
        command.env_remove(key);
    }
    command
        .envs(env)
        .env("ENABLE_CLAUDEAI_MCP_SERVERS", "false")
        .env("CLAUDE_CODE_AUTO_CONNECT_IDE", "0")
        .env("CLAUDE_CODE_IDE_SKIP_AUTO_INSTALL", "1")
        .current_dir(cwd);
    let request = json!({"type":"control_request","request_id":"swarm-commands","request":{"subtype":"initialize"}});
    let reply = ask(command, &request, deadline, |value| {
        value["type"] == "control_response" && value["response"]["request_id"] == "swarm-commands"
    })?;
    if reply["response"]["subtype"] != "success" {
        return Err("claude refused the initialize request".into());
    }
    Ok(claude_rows(&reply["response"]["response"]["commands"]))
}

fn claude_rows(commands: &Value) -> Vec<Row> {
    let Some(commands) = commands.as_array() else {
        return Vec::new();
    };
    commands
        .iter()
        .filter_map(|command| {
            let name = command["name"].as_str()?.to_owned();
            let builtin = command["builtin"].as_bool().unwrap_or(false);
            let mut description = command["description"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_owned();
            // Claude labels the source in the text: "(superset) Manage…" or "…paths. (user)".
            let mut source = String::new();
            if let Some(rest) = description.strip_prefix('(')
                && let Some((label, text)) = rest.split_once(") ")
            {
                source = label.into();
                description = text.into();
            } else if description.ends_with(')')
                && let Some(open) = description.rfind(" (")
            {
                source = description[open + 2..description.len() - 1].into();
                description.truncate(open);
            }
            if builtin {
                source = "built-in".into();
            } else if let Some((plugin, _)) = name.split_once(':') {
                source = plugin.into();
            }
            Some(Row {
                insert: format!("/{name}"),
                name,
                kind: if builtin {
                    Kind::Builtin
                } else {
                    Kind::Command
                },
                description,
                hint: command["argumentHint"].as_str().unwrap_or("").into(),
                source,
                dialog: false,
            })
        })
        .collect()
}

/// Codex's app server lists the skills a session in `cwd` loads, with plugin prefixes and enabled
/// flags. A skill is called with `$name`; Codex lists no slash commands, so those are built-ins.
fn codex(
    env: &BTreeMap<String, String>,
    cwd: &Path,
    deadline: Instant,
) -> Result<Vec<Row>, String> {
    let mut server = crate::profiles::native::AppServer::start(env, deadline)
        .map_err(|error| error.to_string())?;
    let result = server
        .request(2, "skills/list", json!({"cwds":[cwd]}))
        .map_err(|error| error.to_string())?;
    Ok(codex_rows(&result))
}

fn codex_rows(result: &Value) -> Vec<Row> {
    let entries = result["data"].as_array().into_iter().flatten();
    entries
        .flat_map(|entry| entry["skills"].as_array().into_iter().flatten())
        .filter(|skill| skill["enabled"].as_bool() != Some(false))
        .filter_map(|skill| {
            let name = skill["name"].as_str()?.to_owned();
            let source = match name.split_once(':') {
                Some((plugin, _)) => plugin.into(),
                None => skill["scope"].as_str().unwrap_or("").into(),
            };
            Some(Row {
                insert: format!("${name}"),
                name,
                kind: Kind::Skill,
                description: skill["description"].as_str().unwrap_or("").into(),
                hint: String::new(),
                source,
                dialog: false,
            })
        })
        .collect()
}

/// pi's RPC mode lists its extension commands, prompt templates and `skill:` commands. Its TUI
/// built-ins are not listed, and none are added until each is checked.
fn pi(cwd: &Path, deadline: Instant) -> Result<Vec<Row>, String> {
    let mut command = Command::new("pi");
    command
        .args(["--mode", "rpc", "--no-session"])
        .current_dir(cwd);
    let reply = ask(
        command,
        &json!({"type":"get_commands"}),
        deadline,
        |value| value["type"] == "response" && value["command"] == "get_commands",
    )?;
    if reply["success"] != true {
        return Err("pi refused get_commands".into());
    }
    Ok(pi_rows(&reply["data"]["commands"]))
}

fn pi_rows(commands: &Value) -> Vec<Row> {
    let Some(commands) = commands.as_array() else {
        return Vec::new();
    };
    commands
        .iter()
        .filter_map(|command| {
            let name = command["name"].as_str()?.to_owned();
            let source = command["source"].as_str().unwrap_or("");
            Some(Row {
                insert: format!("/{name}"),
                name,
                kind: if source == "skill" {
                    Kind::Skill
                } else {
                    Kind::Command
                },
                description: command["description"].as_str().unwrap_or("").into(),
                hint: String::new(),
                source: source.into(),
                dialog: false,
            })
        })
        .collect()
}

/// AGY's skills: its config folder, each plugin's skills under the plugin's name, and the
/// workspace folders from `cwd` up to the repository root. A skill with
/// `disable-slash-command: true` is hidden.
fn agy(home: &Path, cwd: &Path) -> Vec<Row> {
    let config = home.join(".gemini/config");
    let mut rows = skills(&config.join("skills"), None);
    let mut plugins: Vec<_> = std::fs::read_dir(config.join("plugins"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    plugins.sort();
    for plugin in plugins {
        rows.extend(skills(
            &config.join("plugins").join(&plugin).join("skills"),
            Some(&plugin),
        ));
    }
    for folder in workspace_folders(cwd) {
        for name in [".agents", ".agent", "_agents", "_agent"] {
            rows.extend(skills(&folder.join(name).join("skills"), None));
        }
    }
    rows
}

/// `cwd` and its parents up to the repository root, or `cwd` alone outside a repository.
fn workspace_folders(cwd: &Path) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for folder in cwd.ancestors() {
        folders.push(folder.to_path_buf());
        if folder.join(".git").exists() {
            return folders;
        }
    }
    folders.truncate(1);
    folders
}

fn skills(dir: &Path, plugin: Option<&str>) -> Vec<Row> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|folder| {
            let text = std::fs::read_to_string(dir.join(&folder).join("SKILL.md")).ok()?;
            let fields = frontmatter(&text);
            if fields.get("disable-slash-command").map(String::as_str) == Some("true") {
                return None;
            }
            let base = fields.get("name").cloned().unwrap_or(folder);
            let name = plugin.map_or(base.clone(), |plugin| format!("{plugin}:{base}"));
            Some(Row {
                insert: format!("/{name}"),
                name,
                kind: Kind::Skill,
                description: fields.get("description").cloned().unwrap_or_default(),
                hint: String::new(),
                source: plugin.unwrap_or("skill").into(),
                dialog: false,
            })
        })
        .collect()
}

/// The top-level `key: value` fields of a `---` front matter block. A `|` or `>` block value is
/// joined into one line.
fn frontmatter(text: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return fields;
    }
    let mut block: Option<(String, Vec<String>)> = None;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((_, parts)) = &mut block
            && (line.starts_with(' ') || line.trim().is_empty())
        {
            parts.push(line.trim().to_owned());
            continue;
        }
        if let Some((key, parts)) = block.take() {
            fields.insert(key, parts.join(" ").trim().to_owned());
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if ["|", ">", "|-", ">-", "|+", ">+"].contains(&value) {
            block = Some((key.trim().to_owned(), Vec::new()));
        } else {
            fields.insert(key.trim().to_owned(), value.to_owned());
        }
    }
    if let Some((key, parts)) = block {
        fields.insert(key, parts.join(" ").trim().to_owned());
    }
    fields
}

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
