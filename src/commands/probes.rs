//! Provider command probes and skill file parsing.

use super::*;

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
    // A SIGKILL can leave a lock behind, such as pi's `auth.json.lock`, which makes every later pi
    // wait up to 30 s. So the CLI gets its stdin end, then SIGTERM, which runs its exit handlers,
    // each with a second to exit, before the kill.
    drop(stdin);
    exit_within(&mut child, 1);
    if matches!(child.try_wait(), Ok(None)) {
        // SAFETY: kill has no memory preconditions, and the unreaped child still owns the pid.
        unsafe { kill(child.id() as i32, 15) };
        exit_within(&mut child, 1);
    }
    let _ = child.kill();
    let _ = child.wait();
    result
}

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

fn exit_within(child: &mut std::process::Child, seconds: u64) {
    let deadline = Instant::now() + std::time::Duration::from_secs(seconds);
    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Claude's `initialize` control request answers with the session's commands before any prompt,
/// so no model turn starts. Hooks and MCP servers stay off, as t3code's capability probe does
/// (pingdotgg/t3code `apps/server/src/provider/ClaudeProvider.ts`), so the probe runs no
/// SessionStart hook of the owner's.
pub(super) fn claude(
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

pub(super) fn claude_rows(commands: &Value) -> Vec<Row> {
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
            // Claude's list gives no skill path, and it already leaves out `user-invocable: false`.
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
pub(super) fn codex(
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

pub(super) fn codex_rows(result: &Value) -> Vec<Row> {
    let entries = result["data"].as_array().into_iter().flatten();
    entries
        .flat_map(|entry| entry["skills"].as_array().into_iter().flatten())
        .filter(|skill| skill["enabled"].as_bool() != Some(false))
        .filter_map(|skill| {
            let name = skill["name"].as_str()?.to_owned();
            let source: String = match name.split_once(':') {
                Some((plugin, _)) => plugin.into(),
                None => skill["scope"].as_str().unwrap_or("").into(),
            };
            let source = skill_menu_source(skill["path"].as_str(), &source)?;
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
pub(super) fn pi(cwd: &Path, deadline: Instant) -> Result<Vec<Row>, String> {
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

pub(super) fn pi_rows(commands: &Value) -> Vec<Row> {
    let Some(commands) = commands.as_array() else {
        return Vec::new();
    };
    commands
        .iter()
        .filter_map(|command| {
            let name = command["name"].as_str()?.to_owned();
            let source = command["source"].as_str().unwrap_or("");
            let label = if source == "skill" {
                skill_menu_source(command["sourceInfo"]["path"].as_str(), source)?
            } else {
                source.into()
            };
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
                source: label,
                dialog: false,
            })
        })
        .collect()
}

/// AGY's skills: its config folder, each plugin's skills under the plugin's name, and the
/// workspace folders from `cwd` up to the repository root. A skill with
/// `disable-slash-command: true` or `user-invocable: false` is hidden.
pub(super) fn agy(home: &Path, cwd: &Path) -> Vec<Row> {
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

pub(super) fn skills(dir: &Path, plugin: Option<&str>) -> Vec<Row> {
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
            let path = dir.join(&folder).join("SKILL.md");
            let text = std::fs::read_to_string(&path).ok()?;
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
                source: skill_menu_source(path.to_str(), plugin.unwrap_or("skill"))?,
                dialog: false,
            })
        })
        .collect()
}

/// The menu uses the skill's file for visibility and its owning plugin for the source label.
/// Missing paths or unreadable files keep the CLI's label and visibility.
fn skill_menu_source(path: Option<&str>, source: &str) -> Option<String> {
    let Some(path) = path else {
        return Some(source.into());
    };
    let path = Path::new(path);
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Ok(text) = std::fs::read_to_string(&path)
        && frontmatter(&text)
            .get("user-invocable")
            .is_some_and(|value| {
                let value = value.split(" #").next().unwrap_or("").trim();
                value.eq_ignore_ascii_case("false")
            })
    {
        return None;
    }
    for dir in path
        .ancestors()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "skills"))
    {
        let Some(root) = dir.parent() else { continue };
        for manifest in ["plugin.json", ".claude-plugin/plugin.json"] {
            if let Ok(text) = std::fs::read_to_string(root.join(manifest))
                && let Ok(plugin) = serde_json::from_str::<Value>(&text)
                && plugin["name"] == "swarm"
            {
                return Some("swarm".into());
            }
        }
    }
    Some(source.into())
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
        if line.starts_with([' ', '\t']) {
            continue; // a nested key such as `metadata:` children is not a top-level field
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
