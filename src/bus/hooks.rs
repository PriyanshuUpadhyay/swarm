// Provider state hooks and guard registration.
use super::*;

/// The command a provider hook runs to report agent state (ADR 0021).
pub fn state_hook_command(provider: &str) -> Result<String, String> {
    Ok(format!("{} hook {provider}", quoted_exe()?))
}

/// The flags that make a Claude or Codex CLI run `swarm hook`, which reports the agent's state
/// and its chat log; empty for a CLI with no per-process hook flag. `launch` adds them, and so
/// does `spawn` for a bare `claude` or `codex` command, so its pane can show the chat.
pub fn hook_args(
    provider: Provider,
    agent_id: &str,
    cwd: &std::path::Path,
) -> Result<Vec<String>, String> {
    match provider {
        Provider::Claude => {
            let handler = |command: String| serde_json::json!({"hooks": [{"type": "command", "command": command, "timeout": STATE_HOOK_TIMEOUT}]});
            let state = state_hook_command(provider.id())?;
            let mut hooks = serde_json::Map::new();
            for event in CLAUDE_STATE_EVENTS {
                let mut group = handler(state.clone());
                if event == "PermissionRequest" {
                    group["hooks"][0]["timeout"] = crate::permission::HOOK_TIMEOUT_SECONDS.into();
                }
                hooks.insert(event.into(), serde_json::json!([group]));
            }
            // `/clear` opens a new log, so every Claude agent reports SessionStart state; the chair
            // also records its new session id.
            let mut start = vec![handler(state.clone())];
            if agent_id == "orchestrator" {
                start.insert(0, handler(chair_hook_command(provider.id())?));
            }
            hooks.insert("SessionStart".into(), start.into());
            Ok(vec![
                "--settings".into(),
                owner_choice::claude_json(hooks, cwd),
            ])
        }
        Provider::Codex => {
            // `swarm setup hooks` trusts these for the owner (`codex_hook_trust`); the trust holds
            // only while each value stays byte-identical, so nothing per launch goes in it. The
            // pane env names the agent. The empty first group puts swarm's hook in group 1, so
            // its trust key differs from another tool's `-c` hook in group 0.
            let command = serde_json::to_string(&shared_hook_command(provider.id()))
                .expect("string serialization cannot fail");
            Ok(CODEX_STATE_EVENTS
                .iter()
                .flat_map(|event| {
                    [
                        "-c".into(),
                        format!(
                            "hooks.{event}=[{{hooks=[]}},{{hooks=[{{type=\"command\",command={command},timeout={STATE_HOOK_TIMEOUT}}}]}}]"
                        ),
                    ]
                })
                .collect())
        }
        Provider::Agy | Provider::Pi => Ok(Vec::new()),
    }
}

/// A `swarm spawn` command, with the flags of `hook_args` after a bare `claude` or `codex`, because
/// only `swarm hook` stores the chat log the app's pane reads. A Claude command with its own
/// `--settings` keeps it, and any other command stays as it is.
pub fn spawn_hook_command(
    command: &[String],
    agent_id: &str,
    cwd: &std::path::Path,
) -> Result<Vec<String>, String> {
    let Some((cli, rest)) = command.split_first() else {
        return Ok(Vec::new());
    };
    let provider = match Provider::parse(cli) {
        Some(provider @ (Provider::Claude | Provider::Codex)) => provider,
        _ => return Ok(command.to_vec()),
    };
    if provider == Provider::Claude
        && rest
            .iter()
            .take_while(|arg| *arg != "--")
            .any(|arg| arg == "--settings" || arg.starts_with("--settings="))
    {
        return Ok(command.to_vec());
    }
    let mut hooked = vec![cli.clone()];
    hooked.extend(hook_args(provider, agent_id, cwd)?);
    hooked.extend_from_slice(rest);
    Ok(hooked)
}

/// The state hook for Codex and AGY, whose hook config every swarm build on the Mac shares: Codex
/// trusts a hash of this text, and AGY's `hooks.json` is global. So the text names no build. It
/// runs the `runs/<session>/bin/swarm` link that `swarm launch` made for the pane, not `swarm` on
/// PATH, because a login shell can put another build first. Outside a swarm agent the link is
/// missing, and the hook prints `{}` as `swarm hook` does (ADR 0034). Codex runs a hook in the
/// user's `$SHELL`, which can be fish, so the script goes to `/bin/sh` as one single-quoted word
/// that every shell passes through as it is. `args` is `codex` or `agy <Event>`.
pub fn shared_hook_command(args: &str) -> String {
    format!(
        r#"/bin/sh -c '[ -x "$SWARM_HOME/.swarm/runs/$SWARM_SESSION_ID/bin/swarm" ] && exec "$SWARM_HOME/.swarm/runs/$SWARM_SESSION_ID/bin/swarm" hook {args}; printf "{{}}"'"#
    )
}

/// The plan for AGY's global `hooks.json`, which has no per-process hook flag. The `swarm` group,
/// and with `guard` the `swarm-guard` group, is added when missing, stays when it equals swarm's,
/// and is a conflict otherwise. Every other
/// group stays, and a file that is not a JSON object is refused. AGY sends no event name, so each
/// handler names it.
pub fn agy_hook_plan(path: &std::path::Path, guard: bool) -> Result<FilePlan, String> {
    let text = read_optional(path)?;
    let mut value = json_object(path, text.as_deref())?;
    let before = text.unwrap_or_default();
    let groups = value
        .as_object_mut()
        .expect("json_object returns an object");
    let file = path.display().to_string();
    let mut conflicts = Vec::new();
    let mut edits = Vec::new();
    // The guard is a group of its own (ADR 0040), so a Mac that has swarm's state group keeps it.
    let mut wanted = vec![agy_group_edit(path, "swarm")];
    if guard {
        wanted.push(agy_group_edit(path, "swarm-guard"));
    }
    for edit in wanted {
        let (name, group) = (edit.path[0].as_str(), &edit.wrote);
        match groups.get(name) {
            None => {
                groups.insert(name.into(), group.clone());
                edits.push(edit);
            }
            Some(found) if found == group => {}
            Some(found) => conflicts.push(Conflict {
                kind: ConflictKind::Taken,
                file: file.clone(),
                entry: format!("group {name:?}"),
                found: found.to_string(),
                wanted: group.to_string(),
                fix: format!("rename or delete the {name:?} group in {file}"),
            }),
        }
    }
    if !conflicts.is_empty() {
        edits.clear();
    }
    let after = if edits.is_empty() {
        before.clone()
    } else {
        json_text(&value)
    };
    if after != before {
        refuse_read_only(path)?;
    }
    Ok(FilePlan {
        path: path.to_path_buf(),
        before,
        after,
        conflicts,
        edits,
    })
}

/// The group `name` (`swarm` or `swarm-guard`) as `agy_hook_plan` adds it to AGY's `hooks.json`.
pub fn agy_group_edit(path: &std::path::Path, name: &str) -> Edit {
    let (writer, group) = match name {
        "swarm" => (Writer::HooksState, agy_group()),
        _ => (Writer::HooksGuard, agy_guard_group()),
    };
    Edit::new(writer, path, Kind::JsonKey, &[name], group)
}

/// Whether AGY's `hooks.json` holds swarm's group as `agy_hook_plan` adds it.
pub fn agy_hooks_set(path: &std::path::Path) -> bool {
    read_json_object(path).is_ok_and(|(_, value)| value.get("swarm") == Some(&agy_group()))
}

/// Whether AGY's `hooks.json` holds the guard group as `agy_hook_plan` adds it.
pub fn agy_guard_set(path: &std::path::Path) -> bool {
    read_json_object(path)
        .is_ok_and(|(_, value)| value.get("swarm-guard") == Some(&agy_guard_group()))
}

/// Whether AGY's `hooks.json` has a `swarm-guard` group in any form.
pub fn agy_guard_present(path: &std::path::Path) -> bool {
    read_json_object(path).is_ok_and(|(_, value)| value.get("swarm-guard").is_some())
}

/// Whether a Claude `settings.json` or a Codex `hooks.json` runs swarm's guard command anywhere.
pub fn guard_registered(path: &std::path::Path, provider: &str) -> bool {
    read_json_object(path).is_ok_and(|(_, value)| {
        crate::guard::EVENTS.iter().any(|event| {
            value["hooks"][event].as_array().is_some_and(|groups| {
                groups
                    .iter()
                    .any(|group| has_handler(group, &guard_command(provider, event)))
            })
        })
    })
}

/// The command a guard registration runs. It is `swarm` on PATH, not a session's link, because
/// a chair that the owner started by hand has no swarm session (ADR 0040).
pub fn guard_command(provider: &str, event: &str) -> String {
    format!("swarm guard {provider} {event}")
}

fn guard_handler(provider: &str, event: &str) -> serde_json::Value {
    serde_json::json!({"type": "command", "command": guard_command(provider, event), "timeout": crate::guard::REGISTRATION_TIMEOUT})
}

fn has_handler(group: &serde_json::Value, command: &str) -> bool {
    group["hooks"]
        .as_array()
        .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == command))
}

fn agy_guard_group() -> serde_json::Value {
    let events: serde_json::Map<_, _> = crate::guard::EVENTS
        .iter()
        .map(|event| {
            let group =
                serde_json::json!([{"matcher": "*", "hooks": [guard_handler("agy", event)]}]);
            (event.to_string(), group)
        })
        .collect();
    events.into()
}

/// The plan for the guard registration in a Claude `settings.json` or a Codex `hooks.json`, which
/// share one shape. A group with no matcher is added to each guard event that has no handler
/// running swarm's guard command; every other group stays as it is.
pub fn guard_hooks_plan(path: &std::path::Path, provider: &str) -> Result<FilePlan, String> {
    let (before, mut value) = read_json_object(path)?;
    let file = path.display().to_string();
    let had_hooks = value.get("hooks").is_some();
    let hooks = value
        .as_object_mut()
        .expect("json_object returns an object")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err(format!("swarm: {file} has hooks that is not an object"));
    };
    let mut edits = Vec::new();
    let mut conflicts = Vec::new();
    for event in crate::guard::EVENTS {
        let had_event = hooks.contains_key(event);
        let Some(groups) = hooks
            .entry(event)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
        else {
            return Err(format!(
                "swarm: {file} has hooks.{event} that is not a list"
            ));
        };
        let command = guard_command(provider, event);
        let handlers: Vec<serde_json::Value> = groups
            .iter()
            .filter_map(|group| group["hooks"].as_array())
            .flatten()
            .filter(|handler| handler["command"] == command.as_str())
            .cloned()
            .collect();
        if handlers.is_empty() {
            let edit = guard_group_edit(path, provider, event);
            groups.push(edit.wrote.clone());
            edits.push(Edit {
                created: u8::from(!had_hooks) + u8::from(!had_event),
                ..edit
            });
        }
        // A CLI that times the hook out before the runner answers lets the call through, and
        // swarm cannot hash a Codex handler whose timeout the file does not state (ADR 0040).
        for handler in handlers.into_iter().filter(|handler| {
            handler["timeout"]
                .as_u64()
                .is_none_or(|timeout| timeout < crate::guard::REGISTRATION_TIMEOUT)
        }) {
            conflicts.push(Conflict {
                kind: ConflictKind::Taken,
                file: file.clone(),
                entry: format!("hooks.{event} handler {command:?}"),
                found: handler.to_string(),
                wanted: guard_handler(provider, event).to_string(),
                fix: format!(
                    "set its timeout to {} or more in {file}, so the CLI waits for the runner's answer and swarm can check and trust the handler",
                    crate::guard::REGISTRATION_TIMEOUT
                ),
            });
        }
    }
    let after = if edits.is_empty() {
        before.clone()
    } else {
        json_text(&value)
    };
    if after != before {
        refuse_read_only(path)?;
    }
    Ok(FilePlan {
        path: path.to_path_buf(),
        before,
        after,
        conflicts,
        edits,
    })
}

/// The group that `guard_hooks_plan` adds to `event` in a Claude `settings.json` or a Codex
/// `hooks.json`. A Codex group must stay last, because Codex keys hook trust by place: groups
/// after it would move when it goes and lose their trust.
pub fn guard_group_edit(path: &std::path::Path, provider: &str, event: &str) -> Edit {
    let group = serde_json::json!({"hooks": [guard_handler(provider, event)]});
    Edit {
        last: provider == "codex",
        ..Edit::new(
            Writer::HooksGuard,
            path,
            Kind::JsonArrayItem,
            &["hooks", event],
            group,
        )
    }
}

fn agy_group() -> serde_json::Value {
    let handler = |event: &str| serde_json::json!({"type": "command", "command": shared_hook_command(&format!("agy {event}")), "timeout": STATE_HOOK_TIMEOUT});
    // PostToolUse takes matcher groups; PreInvocation and Stop take a flat handler list.
    // Not PreToolUse: that is AGY's permission gate, which needs a `decision`, and the `{}` that
    // `swarm hook` prints makes AGY refuse every tool call in every AGY session.
    serde_json::json!({
        "PreInvocation": [handler("PreInvocation")],
        "PostToolUse": [{"matcher": "*", "hooks": [handler("PostToolUse")]}],
        "Stop": [handler("Stop")],
    })
}
