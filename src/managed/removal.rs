//! Revert planning and removal of managed text entries.

use super::*;

/// What `revert_plan` takes: named ids, or every item that is present now.
pub enum Target {
    Ids(Vec<String>),
    All,
}

/// The plans that remove each targeted item, one per file, with the item's linked edit too. An
/// item goes only while it equals what swarm wrote; one with another value is a conflict, and a
/// gone one changes no file and only turns off. `found` is as for `list`.
pub fn revert_plan(
    store: &rusqlite::Connection,
    found: &[Edit],
    target: &Target,
) -> Result<Vec<Plan>, String> {
    let entries = list(store, found)?;
    let mut ids: Vec<String> = match target {
        Target::All => entries
            .iter()
            .filter(|entry| entry.state == State::Present)
            .map(|entry| entry.edit.id())
            .collect(),
        Target::Ids(ids) => {
            for id in ids {
                if !entries.iter().any(|entry| entry.edit.id() == *id) {
                    return Err(format!(
                        "swarm: no managed entry {id}; run swarm managed list"
                    ));
                }
            }
            ids.clone()
        }
    };
    // A linked pair goes together, whichever of the two was named.
    loop {
        let linked: Vec<String> = entries
            .iter()
            .filter(|entry| {
                let id = entry.edit.id();
                !ids.contains(&id)
                    && (entry
                        .edit
                        .with
                        .as_ref()
                        .is_some_and(|with| ids.contains(with))
                        || entries.iter().any(|other| {
                            ids.contains(&other.edit.id()) && other.edit.with.as_ref() == Some(&id)
                        }))
            })
            .map(|entry| entry.edit.id())
            .collect();
        if linked.is_empty() {
            break;
        }
        ids.extend(linked);
    }
    let chosen: Vec<&Entry> = entries
        .iter()
        .filter(|entry| ids.contains(&entry.edit.id()))
        .collect();
    let mut files: Vec<&std::path::PathBuf> = chosen.iter().map(|entry| &entry.edit.file).collect();
    files.sort();
    files.dedup();
    files
        .into_iter()
        .map(|file| {
            let items: Vec<&Entry> = chosen
                .iter()
                .copied()
                .filter(|entry| entry.edit.file == *file)
                .collect();
            if let Some(entry) = items.iter().find(|entry| entry.edit.kind == Kind::Symlink) {
                return link_removal(store, entry).map(Plan::Link);
            }
            Ok(removal(file, &items)
                .unwrap_or_else(|error| FilePlan::unreadable(file.clone(), error))
                .into())
        })
        .collect::<Result<Vec<_>, _>>()
}

/// The plan for one file that removes each present item of `items`. Each item's state is read
/// again in the text that the plan edits, so a value that changed after `list` read it is a
/// conflict, not removed.
pub(super) fn removal(file: &std::path::Path, items: &[&Entry]) -> Result<FilePlan, String> {
    let text = read_optional(file)?;
    let before = text.clone().unwrap_or_default();
    let mut conflicts = Vec::new();
    let mut gone = Vec::new();
    for entry in items {
        let edit = &entry.edit;
        match state_in(edit, entry.state == State::Off, text.as_deref()) {
            State::Present => gone.push(edit),
            State::Changed(found) => conflicts.push(Conflict {
                kind: ConflictKind::Changed,
                file: file.display().to_string(),
                entry: describe(edit),
                found: found.to_string(),
                wanted: edit.wrote.to_string(),
                fix: format!(
                    "swarm leaves it; delete {} from {} by hand, or set it back to swarm's value and run again",
                    describe(edit),
                    file.display()
                ),
            }),
            State::Unreadable(error) => conflicts.push(Conflict {
                kind: ConflictKind::Unreadable,
                file: file.display().to_string(),
                entry: describe(edit),
                found: error,
                wanted: "a file that swarm can read and edit".into(),
                fix: format!(
                    "swarm leaves it; make {} readable again and run again",
                    file.display()
                ),
            }),
            State::Gone | State::Off => {}
        }
    }
    let after = match gone.is_empty() {
        true => before.clone(),
        // An empty plan text removes the file (`commit`).
        false if gone[0].kind == Kind::File => String::new(),
        false if gone[0].kind == Kind::TomlKey => remove_toml(file, &before, &gone)?,
        false => remove_json(file, &before, &gone, &mut conflicts)?,
    };
    if after != before {
        refuse_read_only(file)?;
    }
    Ok(FilePlan {
        path: file.to_path_buf(),
        before,
        after,
        conflicts,
        edits: items.iter().map(|entry| entry.edit.clone()).collect(),
    })
}

/// An item's place as the owner reads it in the file.
fn describe(edit: &Edit) -> String {
    let quoted = |part: &String| match part
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
    {
        true => part.clone(),
        false => format!("{part:?}"),
    };
    match edit.kind {
        Kind::TomlKey => match edit.path.split_last() {
            Some((key, tables)) => format!(
                "[{}] {}",
                tables.iter().map(quoted).collect::<Vec<_>>().join("."),
                quoted(key)
            ),
            None => String::new(),
        },
        Kind::JsonKey => edit.path.iter().map(quoted).collect::<Vec<_>>().join("."),
        Kind::JsonArrayItem => format!(
            "{} item {}",
            edit.path.iter().map(quoted).collect::<Vec<_>>().join("."),
            edit.wrote
        ),
        Kind::Symlink => "the symlink".into(),
        Kind::File => "the whole file".into(),
    }
}

/// `before` with each TOML key of `gone` removed, or set back to the value it replaced, and each
/// table the write made removed again once it is empty. toml_edit keeps every other byte.
fn remove_toml(file: &std::path::Path, before: &str, gone: &[&Edit]) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = before
        .parse()
        .map_err(|error| format!("{} is not valid TOML: {error}", file.display()))?;
    for edit in gone {
        let (key, tables) = edit.path.split_last().ok_or("an empty path")?;
        let table = toml_at(&mut doc, tables).ok_or("a table that changed since it was read")?;
        match &edit.before {
            Some(value) => {
                let mut value = json_toml(value).ok_or("a value that TOML cannot hold")?;
                // The owner's spacing and comment around the value stay.
                if let Some(old) = table.get(key).and_then(toml_edit::Item::as_value) {
                    *value.decor_mut() = old.decor().clone();
                }
                table.insert(key, toml_edit::Item::Value(value));
            }
            None => {
                table.remove(key);
                for level in
                    (tables.len() - usize::from(edit.created).min(tables.len())..tables.len()).rev()
                {
                    let empty = toml_at(&mut doc, &edit.path[..=level])
                        .is_some_and(|table| table.is_empty());
                    if !empty {
                        break;
                    }
                    if let Some(parent) = toml_at(&mut doc, &edit.path[..level]) {
                        parent.remove(&edit.path[level]);
                    }
                }
            }
        }
    }
    Ok(doc.to_string())
}

/// The table at `keys` in `doc`; the root for no keys.
fn toml_at<'a>(
    doc: &'a mut toml_edit::DocumentMut,
    keys: &[String],
) -> Option<&'a mut dyn toml_edit::TableLike> {
    let mut table: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for key in keys {
        table = table.get_mut(key)?.as_table_like_mut()?;
    }
    Some(table)
}

/// A JSON scalar as the TOML value of the same type.
fn json_toml(value: &serde_json::Value) -> Option<toml_edit::Value> {
    Some(match value {
        serde_json::Value::String(text) => text.as_str().into(),
        serde_json::Value::Bool(flag) => (*flag).into(),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(integer) => integer.into(),
            None => number.as_f64()?.into(),
        },
        _ => return None,
    })
}

/// `before` with each JSON item of `gone` removed, or set back to the value it replaced, and each
/// object or array the write made removed again once it is empty. An item that must stay last
/// (`Edit::last`) with another item after it is a conflict.
fn remove_json(
    file: &std::path::Path,
    before: &str,
    gone: &[&Edit],
    conflicts: &mut Vec<Conflict>,
) -> Result<String, String> {
    let mut value = json_object(file, (!before.is_empty()).then_some(before))?;
    let mut changed = false;
    for edit in gone {
        let array = edit.kind == Kind::JsonArrayItem;
        let (containers, key) = match array {
            true => (&edit.path[..], None),
            false => {
                let (key, keys) = edit.path.split_last().ok_or("an empty path")?;
                (keys, Some(key))
            }
        };
        let container =
            json_at(&mut value, containers).ok_or("a container that changed since it was read")?;
        match (key, &edit.before) {
            (Some(key), Some(previous)) => {
                container[key.as_str()] = previous.clone();
            }
            (Some(key), None) => {
                container
                    .as_object_mut()
                    .and_then(|object| object.remove(key.as_str()));
            }
            (None, _) => {
                let items = container
                    .as_array_mut()
                    .ok_or("an array that changed since it was read")?;
                let index = items
                    .iter()
                    .position(|item| *item == edit.wrote)
                    .ok_or("an item that changed since it was read")?;
                if edit.last && index + 1 < items.len() {
                    conflicts.push(Conflict {
                        kind: ConflictKind::Order,
                        file: file.display().to_string(),
                        entry: describe(edit),
                        found: format!("{} more group(s) after swarm's", items.len() - index - 1),
                        wanted: "swarm's group last".into(),
                        fix: format!(
                            "move swarm's group last in {}, or remove it by hand",
                            file.display()
                        ),
                    });
                    continue;
                }
                items.remove(index);
            }
        }
        changed = true;
        if edit.before.is_some() {
            continue;
        }
        let depth = containers.len();
        for level in (depth - usize::from(edit.created).min(depth)..depth).rev() {
            let empty =
                json_at(&mut value, &edit.path[..=level]).is_some_and(|inner| match inner {
                    serde_json::Value::Object(object) => object.is_empty(),
                    serde_json::Value::Array(items) => items.is_empty(),
                    _ => false,
                });
            if !empty {
                break;
            }
            if let Some(parent) =
                json_at(&mut value, &edit.path[..level]).and_then(serde_json::Value::as_object_mut)
            {
                parent.remove(&edit.path[level]);
            }
        }
    }
    Ok(match changed {
        true => json_text(&value),
        false => before.to_string(),
    })
}

/// The value at `keys` in `value`.
fn json_at<'a>(
    value: &'a mut serde_json::Value,
    keys: &[String],
) -> Option<&'a mut serde_json::Value> {
    keys.iter()
        .try_fold(value, |inner, key| inner.get_mut(key.as_str()))
}
