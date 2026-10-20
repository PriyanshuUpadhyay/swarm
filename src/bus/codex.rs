// Codex hook hashes and hook-trust plans.
use super::*;

/// The Codex `hooks.state` key and trusted hash of each state hook `swarm launch` passes with
/// `-c`, as Codex computes them (codex-rs `hooks/src/engine/discovery.rs` `hook_hash` and
/// `config/src/fingerprint.rs` `version_for_toml`): the SHA-256 of the hook's identity as JSON with
/// sorted keys. `tests` pins the values a real Codex 0.159.0 app-server reported.
pub fn codex_hook_trust(command: &str) -> Vec<(String, String)> {
    CODEX_STATE_EVENTS
        .iter()
        .map(|event| {
            let label = codex_event_label(event);
            (
                format!("/<session-flags>/config.toml:{label}:1:0"),
                codex_hash(&label, None, command, STATE_HOOK_TIMEOUT, false),
            )
        })
        .collect()
}

/// The hash Codex trusts for one command hook: its handler and its group's matcher.
fn codex_hash(
    label: &str,
    matcher: Option<&str>,
    command: &str,
    timeout: u64,
    is_async: bool,
) -> String {
    use sha2::Digest;
    // Keys in sorted order; with `preserve_order` the map keeps them so.
    let mut identity = serde_json::json!({
        "event_name": label,
        "hooks": [{"async": is_async, "command": command, "timeout": timeout, "type": "command"}],
    });
    if let Some(matcher) = matcher {
        identity["matcher"] = matcher.into();
    }
    let digest = sha2::Sha256::digest(identity.to_string().as_bytes());
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256:{hash}")
}

/// The Codex trust entry of each guard handler in a Codex home's `hooks.json` text, keyed by its
/// group and handler place as Codex counts them (`codex app-server` 0.159.0 `hooks/list`,
/// 2026-10-05). The hash follows the handler as the file holds it, so an owner's own timeout or
/// matcher is trusted as it is. A handler with no timeout gets no entry, because Codex's default
/// is not known here; Codex then asks for review.
pub fn codex_guard_trust(home: &std::path::Path, hooks: &str) -> Vec<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(hooks).unwrap_or_default();
    let file = home.join("hooks.json");
    let mut entries = Vec::new();
    for event in crate::guard::EVENTS {
        let command = guard_command("codex", event);
        let label = codex_event_label(event);
        let groups = value["hooks"][event].as_array().into_iter().flatten();
        for (group_index, group) in groups.enumerate() {
            let handlers = group["hooks"].as_array().into_iter().flatten();
            for (handler_index, handler) in handlers.enumerate() {
                let Some(timeout) = handler["timeout"].as_u64() else {
                    continue;
                };
                if handler["command"] != command.as_str() {
                    continue;
                }
                entries.push((
                    format!("{}:{label}:{group_index}:{handler_index}", file.display()),
                    codex_hash(
                        &label,
                        group["matcher"].as_str(),
                        &command,
                        timeout,
                        handler["async"].as_bool().unwrap_or(false),
                    ),
                ));
            }
        }
    }
    entries
}

/// Codex's snake_case name for a hook event, as its trust keys use it.
fn codex_event_label(event: &str) -> String {
    let mut label = String::new();
    for (index, character) in event.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            label.push('_');
        }
        label.push(character.to_ascii_lowercase());
    }
    label
}

/// The plan for a Codex home's `config.toml`, with the trust keys of swarm's `state` hooks and of
/// its `guard` registration. A missing trust entry is added; an entry with swarm's hash stays, in
/// any TOML form or key spelling; any other entry at swarm's key is a conflict. The file is edited
/// as TOML, so an entry is never added twice: a second table with the same name makes the whole
/// file unreadable to Codex.
pub fn codex_hook_plan(
    home: &std::path::Path,
    state: &[(String, String)],
    guard: &[(String, String)],
) -> Result<FilePlan, String> {
    let path = home.join("config.toml");
    let (before, mut config) = read_codex_config(&path)?;
    let file = path.display().to_string();
    let not_table = |name: &str| {
        format!("{file} has a {name} that is not a table; set swarm's trusted_hash by hand")
    };
    // The tables the file lacks now, so a revert removes them again once they are empty.
    let had_hooks = config.get("hooks").is_some();
    let had_state = config
        .get("hooks")
        .and_then(|hooks| hooks.get("state"))
        .is_some();
    let created = 1 + u8::from(!had_hooks) + u8::from(!had_state);
    let hooks =
        toml_table(config.as_table_mut(), "hooks", true).ok_or_else(|| not_table("hooks"))?;
    let table = toml_table(hooks, "state", true).ok_or_else(|| not_table("hooks.state"))?;
    let mut conflicts = Vec::new();
    let mut edits = Vec::new();
    let writers = state.iter().map(|entry| (entry, Writer::HooksState));
    for ((key, hash), writer) in
        writers.chain(guard.iter().map(|entry| (entry, Writer::HooksGuard)))
    {
        let found = match table.get(key) {
            Some(entry) => entry.get("trusted_hash").and_then(toml_edit::Item::as_str),
            None => {
                toml_table(&mut *table, key, false)
                    .expect("a missing key becomes a table")
                    .insert("trusted_hash", toml_edit::value(hash.as_str()));
                edits.push(Edit {
                    created,
                    ..codex_trust_edit(home, key, hash, writer)
                });
                continue;
            }
        };
        if found != Some(hash.as_str()) {
            conflicts.push(Conflict {
                kind: ConflictKind::Taken,
                file: file.clone(),
                entry: format!("[hooks.state.{key:?}]"),
                found: found.unwrap_or("an entry with no trusted_hash").to_string(),
                wanted: hash.clone(),
                fix: format!(
                    "delete [hooks.state.{key:?}] from {file}, or do not use swarm's Codex hooks"
                ),
            });
        }
    }
    // TOML output ends in a newline, so a file that only lacks one is kept as it is.
    let after = if edits.is_empty() {
        before.clone()
    } else {
        config.to_string()
    };
    if after != before {
        refuse_read_only(&path)?;
    }
    Ok(FilePlan {
        path,
        before,
        after,
        conflicts,
        edits,
    })
}

/// The trust key `key` with `hash` in a Codex home's `config.toml`. A guard key goes with the
/// guard group it trusts, so a revert of either removes both: Codex asks again for a group whose
/// trust is gone, and a trust key left without its group trusts nothing.
pub fn codex_trust_edit(home: &std::path::Path, key: &str, hash: &str, writer: Writer) -> Edit {
    let mut edit = Edit::new(
        writer,
        &home.join("config.toml"),
        Kind::TomlKey,
        &["hooks", "state", key, "trusted_hash"],
        hash.into(),
    );
    if writer == Writer::HooksGuard {
        // A guard key is `<hooks.json>:<event label>:<group>:<handler>` (`codex_guard_trust`).
        let label = key.rsplit(':').nth(2).unwrap_or_default();
        edit.with = crate::guard::EVENTS
            .iter()
            .find(|event| codex_event_label(event) == label)
            .map(|event| guard_group_edit(&home.join("hooks.json"), "codex", event).id());
    }
    edit
}

/// Whether a Codex home's `config.toml` trusts every entry.
pub fn codex_hooks_trusted(home: &std::path::Path, entries: &[(String, String)]) -> bool {
    let text = std::fs::read_to_string(home.join("config.toml")).unwrap_or_default();
    let Ok(config) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    let state = config.get("hooks").and_then(|hooks| hooks.get("state"));
    entries.iter().all(|(key, hash)| {
        state
            .and_then(|state| state.get(key.as_str()))
            .and_then(|entry| entry.get("trusted_hash"))
            .and_then(toml_edit::Item::as_str)
            == Some(hash.as_str())
    })
}
