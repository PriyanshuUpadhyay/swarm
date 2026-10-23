//! Live edit states and managed entry listing.

use super::*;

/// An item's live state in its file.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// It equals what swarm wrote.
    Present,
    /// Another value is there now.
    Changed(serde_json::Value),
    /// The file cannot be read or parsed, or a container on the path is not one (the reason).
    Unreadable(String),
    /// It is absent, and swarm did not remove it.
    Gone,
    /// It is as it was before the write (absent, or the value it replaced), because swarm set it
    /// back.
    Off,
}

impl State {
    /// The wire name. Open: a later build may add one (A4).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Changed(_) => "changed",
            Self::Unreadable(_) => "unreadable",
            Self::Gone => "gone",
            Self::Off => "off",
        }
    }
}

/// Read `edit`'s place in its file now. `off` says swarm set it back.
pub fn state(edit: &Edit, off: bool) -> State {
    if edit.kind == Kind::Symlink {
        return match link_leaf(&edit.file) {
            Ok(LinkLeaf::Absent) if off => State::Off,
            Ok(LinkLeaf::Absent) => State::Gone,
            Ok(LinkLeaf::Link(target)) => match target.to_str().map(serde_json::Value::from) {
                Some(value) if !off && value == edit.wrote => State::Present,
                Some(value) => State::Changed(value),
                None => unreachable!(),
            },
            Ok(LinkLeaf::Other(kind)) => State::Changed(kind.into()),
            Err(error) => State::Unreadable(error),
        };
    }
    match read_optional(&edit.file) {
        Ok(text) => state_in(edit, off, text.as_deref()),
        Err(error) => State::Unreadable(error),
    }
}

/// `state` in `text`, the file's text (None for a missing file).
pub(super) fn state_in(edit: &Edit, off: bool, text: Option<&str>) -> State {
    match live(edit, text) {
        Ok(Some(value)) if value == edit.wrote => State::Present,
        Ok(value) if off && value == edit.before => State::Off,
        Ok(Some(value)) => State::Changed(value),
        Ok(None) => State::Gone,
        Err(error) => State::Unreadable(error),
    }
}

/// The value at `edit`'s place in `text`, None when it is absent. An array item is either there or
/// absent. A file that cannot be parsed, or a container that is not one, is an error, so swarm
/// never takes it for absent (R4).
fn live(edit: &Edit, text: Option<&str>) -> Result<Option<serde_json::Value>, String> {
    let Some(text) = text else {
        return Ok(None);
    };
    if edit.kind == Kind::File {
        return Ok(Some(file_hash(text)));
    }
    let (keys, last) = match edit.kind {
        Kind::JsonArrayItem => (&edit.path[..], None),
        _ => match edit.path.split_last() {
            Some((last, keys)) => (keys, Some(last)),
            None => return Err("an empty path".into()),
        },
    };
    let not_container = |key: &str| {
        format!(
            "{} has a {key} that swarm cannot read into",
            edit.file.display()
        )
    };
    if edit.kind == Kind::TomlKey {
        let doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|error| format!("{} is not valid TOML: {error}", edit.file.display()))?;
        let mut table: &dyn toml_edit::TableLike = doc.as_table();
        for key in keys {
            match table.get(key) {
                None => return Ok(None),
                Some(item) => table = item.as_table_like().ok_or_else(|| not_container(key))?,
            }
        }
        return Ok(last.and_then(|last| table.get(last)).map(toml_json));
    }
    let mut value = json_object(&edit.file, Some(text))?;
    // Only the array of a JsonArrayItem is an array; any other container is an object.
    for (index, key) in keys.iter().enumerate() {
        let array = last.is_none() && index + 1 == keys.len();
        match value.get_mut(key.as_str()) {
            None => return Ok(None),
            Some(inner) if inner.is_object() || (array && inner.is_array()) => value = inner.take(),
            Some(_) => return Err(not_container(key)),
        }
    }
    Ok(match last {
        Some(last) => value.get(last.as_str()).cloned(),
        None => match value.as_array() {
            Some(items) => items.contains(&edit.wrote).then(|| edit.wrote.clone()),
            None => return Err(not_container(&edit.path.join("."))),
        },
    })
}

/// A TOML item as JSON: a string, integer, float, or bool keeps its type; anything else is its
/// TOML text.
fn toml_json(item: &toml_edit::Item) -> serde_json::Value {
    match item.as_value() {
        Some(toml_edit::Value::String(value)) => value.value().as_str().into(),
        Some(toml_edit::Value::Integer(value)) => (*value.value()).into(),
        Some(toml_edit::Value::Float(value)) => (*value.value()).into(),
        Some(toml_edit::Value::Boolean(value)) => (*value.value()).into(),
        _ => item.to_string().trim().into(),
    }
}

/// One item of `managed list`: a recorded row, or an item found equal to swarm's text.
#[derive(Clone, Debug)]
pub struct Entry {
    pub edit: Edit,
    pub state: State,
    pub recorded: bool,
    /// Unix seconds of the last apply; None for a found item.
    pub at_s: Option<i64>,
}

impl Entry {
    /// The wire form. `found` is the live value only for `changed`, and `error` the reason only
    /// for `unreadable` (A3); `before` null and absent mean the same: the place was absent (A5, J2).
    pub fn json(&self) -> serde_json::Value {
        let (found, error) = match &self.state {
            State::Changed(value) => (value.clone(), None),
            State::Unreadable(error) => (serde_json::Value::Null, Some(error)),
            _ => (serde_json::Value::Null, None),
        };
        serde_json::json!({
            "id": self.edit.id(),
            "writer": self.edit.writer,
            "file": self.edit.file.to_string_lossy(),
            "kind": self.edit.kind,
            "path": self.edit.path,
            "wrote": self.edit.wrote,
            "before": self.edit.before,
            "state": self.state.name(),
            "found": found,
            "error": error,
            "recorded": self.recorded,
            "at_s": self.at_s,
            "with": self.edit.with,
        })
    }
}

/// Each recorded item with its live state, then each of `found` that equals swarm's text in its
/// file and has no row. `found` is every item swarm's writers own today; list writes no row.
pub fn list(store: &rusqlite::Connection, found: &[Edit]) -> Result<Vec<Entry>, String> {
    let mut entries = recorded(store, false)?;
    let mut ids: std::collections::HashSet<String> =
        entries.iter().map(|entry| entry.edit.id()).collect();
    for edit in found {
        if edit.kind != Kind::Symlink
            && state(edit, false) == State::Present
            && ids.insert(edit.id())
        {
            entries.push(Entry {
                edit: edit.clone(),
                state: State::Present,
                recorded: false,
                at_s: None,
            });
        }
    }
    Ok(entries)
}

/// Background pruning reads only active trust records, never found items or reverted rows.
pub(super) fn recorded(
    store: &rusqlite::Connection,
    active_trust_only: bool,
) -> Result<Vec<Entry>, String> {
    let failed = |error: rusqlite::Error| format!("swarm: cannot read managed edits: {error}");
    let mut query = store
        .prepare(
            "SELECT writer, file, kind, path, wrote, before, created, with_id, at_s, off
             FROM managed_edit
             WHERE NOT ?1 OR (writer = 'launch.trust' AND off = 0)
             ORDER BY file, path",
        )
        .map_err(failed)?;
    let rows = query
        .query_map([active_trust_only], |row| {
            Ok((
                [
                    row.get::<_, String>(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ],
                row.get::<_, Option<String>>(5)?,
                row.get::<_, u8>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, bool>(9)?,
            ))
        })
        .map_err(failed)?;
    let mut entries = Vec::new();
    for row in rows {
        let ([writer, file, kind, path, wrote], before, created, with, at_s, off) =
            row.map_err(failed)?;
        let unknown = |what: &str, value: &str| {
            format!(
                "swarm: a managed edit has {what} {value:?}, which this build does not know; use a newer swarm"
            )
        };
        let parse = |text: &str| serde_json::from_str::<serde_json::Value>(text);
        let mut edit = Edit {
            writer: serde_json::from_value(writer.clone().into())
                .map_err(|_| unknown("writer", &writer))?,
            file: file.into(),
            kind: serde_json::from_value(kind.clone().into())
                .map_err(|_| unknown("kind", &kind))?,
            path: serde_json::from_str(&path).map_err(|_| unknown("path", &path))?,
            wrote: parse(&wrote).map_err(|_| unknown("value", &wrote))?,
            before: match before {
                Some(before) => Some(parse(&before).map_err(|_| unknown("value", &before))?),
                None => None,
            },
            created,
            with,
            last: false,
        };
        // managed_edit has no column for it, so a row takes it from what it wrote: only the Codex
        // guard group (`bus::guard_group_edit`) sets it, and its handler names Codex. The file
        // name cannot tell, because `place` keeps a linked `hooks.json` under its target's name.
        edit.last = edit.writer == Writer::HooksGuard
            && edit.kind == Kind::JsonArrayItem
            && edit.path.last().is_some_and(|event| {
                edit.wrote["hooks"][0]["command"] == crate::bus::guard_command("codex", event)
            });
        let state = state(&edit, off);
        entries.push(Entry {
            edit,
            state,
            recorded: true,
            at_s: Some(at_s),
        });
    }
    Ok(entries)
}
