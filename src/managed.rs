//! Every write swarm makes outside its home (ADR 0042): the plan types each writer fills, the
//! digest that apply checks, and the locked, link-following, compare-before-rename file writes.

use std::os::unix::fs::OpenOptionsExt;

/// An entry that swarm needs at a place where the file already holds another one (ADR 0036).
/// Swarm never writes over it: the owner removes it, or does without swarm's hooks.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Conflict {
    pub file: String,
    pub entry: String,
    pub found: String,
    pub wanted: String,
    pub fix: String,
}

/// What one writer would do to one file: its text now, its planned text, each conflict, and each
/// item that the planned text adds. An entry equal to swarm's stays as it is.
#[derive(Debug)]
pub struct FilePlan {
    pub path: std::path::PathBuf,
    pub before: String,
    pub after: String,
    pub conflicts: Vec<Conflict>,
    pub edits: Vec<Edit>,
}

impl FilePlan {
    /// A file that swarm cannot read or edit, as a conflict, so setup writes no file and the
    /// owner sees the fix (owner's choice, 2026-10-01).
    pub fn unreadable(path: std::path::PathBuf, error: String) -> Self {
        let file = path.display().to_string();
        Self {
            conflicts: vec![Conflict {
                file: file.clone(),
                entry: "the whole file".into(),
                found: error,
                wanted: "a file that swarm can read and edit".into(),
                fix: format!("repair {file}, or move it away"),
            }],
            path,
            before: String::new(),
            after: String::new(),
            edits: Vec::new(),
        }
    }
}

/// The three places swarm writes. Wire names are open: a later build may add one (A4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A key in a TOML table; `path` is the tables, then the key.
    TomlKey,
    /// A key in a JSON object; `path` is the keys, the last one the item's own.
    JsonKey,
    /// One item of a JSON array; `path` is the keys to the array.
    JsonArrayItem,
}

/// The swarm feature that wrote an item. Wire names are open: a later build may add one (A4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Writer {
    #[serde(rename = "hooks.state")]
    HooksState,
    #[serde(rename = "hooks.guard")]
    HooksGuard,
    #[serde(rename = "launch.trust")]
    LaunchTrust,
    #[serde(rename = "herdr")]
    Herdr,
}

/// One item that a writer adds to a file outside the swarm home.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub writer: Writer,
    /// The write target, links followed, so two names of one file are one place.
    pub file: std::path::PathBuf,
    pub kind: Kind,
    pub path: Vec<String>,
    /// The value written, as JSON; a TOML value keeps its type (string, integer, float, bool).
    pub wrote: serde_json::Value,
    /// The value the write replaced; None when the place was absent.
    pub before: Option<serde_json::Value>,
    /// How many containers at the end of `path` (tables, objects, the array) the file lacked
    /// before the write, so a revert removes them again once they are empty.
    pub created: u8,
    /// The id of an edit that is always reverted with this one.
    pub with: Option<String>,
}

impl Edit {
    /// A new item at `path` of `file`, with nothing before it.
    pub fn new(
        writer: Writer,
        file: &std::path::Path,
        kind: Kind,
        path: &[&str],
        wrote: serde_json::Value,
    ) -> Self {
        Self {
            writer,
            file: place(file),
            kind,
            path: path.iter().map(|part| part.to_string()).collect(),
            wrote,
            before: None,
            created: 0,
            with: None,
        }
    }

    /// The first 12 hex digits of a hash of the place, so a found item and a recorded one of one
    /// place share an id. An array item adds its value, because two items share one array.
    pub fn id(&self) -> String {
        use sha2::Digest;
        let mut digest = sha2::Sha256::new();
        digest.update(self.file.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(wire(&self.kind).as_bytes());
        for part in &self.path {
            digest.update([0]);
            digest.update(part.as_bytes());
        }
        if self.kind == Kind::JsonArrayItem {
            digest.update([0]);
            digest.update(self.wrote.to_string().as_bytes());
        }
        digest.finalize()[..6]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// The file a write to `file` lands in, with its folder's links resolved too, so two names of one
/// file, or of one missing file, are one place. The nearest folder that exists is resolved and the
/// missing rest joined to it, so a file whose folder the write makes has one place before and after.
fn place(file: &std::path::Path) -> std::path::PathBuf {
    let target = write_target(file).unwrap_or_else(|_| file.to_path_buf());
    let mut missing = Vec::new();
    let mut dir = target.as_path();
    while let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) {
        missing.push(name);
        if let Ok(real) = std::fs::canonicalize(parent) {
            return missing
                .iter()
                .rev()
                .fold(real, |path, name| path.join(name));
        }
        dir = parent;
    }
    target
}

/// The wire name of a `Kind` or `Writer`.
fn wire(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => unreachable!("Kind and Writer serialize as strings"),
    }
}

/// Write each planned file and record its edits, one transaction per file, so a row exists only
/// for a file that was written. Refuses any conflict in any plan before any write. Returns each
/// file that changed. The caller holds `trust.lock` from the plan to here and checks the digest.
pub fn apply(
    store: &rusqlite::Connection,
    plans: &[FilePlan],
) -> Result<Vec<std::path::PathBuf>, String> {
    commit(store, plans, false)
}

/// `apply` for the plans of `revert_plan`: each edit's row is set off, not added.
pub fn revert(
    store: &rusqlite::Connection,
    plans: &[FilePlan],
) -> Result<Vec<std::path::PathBuf>, String> {
    commit(store, plans, true)
}

fn commit(
    store: &rusqlite::Connection,
    plans: &[FilePlan],
    off: bool,
) -> Result<Vec<std::path::PathBuf>, String> {
    if let Some(conflicts) = conflicts_text(plans) {
        return Err(conflicts);
    }
    let failed = |error: rusqlite::Error| format!("swarm: cannot record a managed edit: {error}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    let mut changed = Vec::new();
    for plan in plans {
        let writes = plan.after != plan.before;
        if !writes && plan.edits.is_empty() {
            continue;
        }
        let tx =
            rusqlite::Transaction::new_unchecked(store, rusqlite::TransactionBehavior::Immediate)
                .map_err(failed)?;
        for edit in &plan.edits {
            if off {
                tx.execute("UPDATE managed_edit SET off = 1 WHERE id = ?1", [edit.id()])
            } else {
                tx.execute(
                    "INSERT INTO managed_edit
                         (id, writer, file, kind, path, wrote, before, created, with_id, at_s, off)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)
                     ON CONFLICT (id) DO UPDATE SET writer = excluded.writer,
                         file = excluded.file, kind = excluded.kind, path = excluded.path,
                         wrote = excluded.wrote, before = excluded.before,
                         created = excluded.created, with_id = excluded.with_id,
                         at_s = excluded.at_s, off = 0",
                    rusqlite::params![
                        edit.id(),
                        wire(&edit.writer),
                        edit.file.to_string_lossy(),
                        wire(&edit.kind),
                        serde_json::Value::from(edit.path.clone()).to_string(),
                        edit.wrote.to_string(),
                        edit.before.as_ref().map(serde_json::Value::to_string),
                        edit.created,
                        edit.with,
                        now,
                    ],
                )
            }
            .map_err(failed)?;
        }
        if writes {
            write_text(&plan.path, &plan.before, &plan.after)?;
        }
        tx.commit().map_err(|error| match writes {
            true => format!(
                "swarm: wrote {} but could not record it: {error}; swarm managed list shows it as found",
                plan.path.display()
            ),
            false => failed(error),
        })?;
        if writes {
            changed.push(plan.path.clone());
        }
    }
    Ok(changed)
}

/// A digest of each planned file's path, text, and planned text, so apply refuses a file that
/// changed after the owner saw the plan, and a swarm whose entries differ from the plan's.
pub fn digest(plans: &[FilePlan]) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    for plan in plans {
        digest.update(plan.path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(plan.before.as_bytes());
        digest.update([0]);
        digest.update(plan.after.as_bytes());
        digest.update([0]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// An item's live state in its file.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// It equals what swarm wrote.
    Present,
    /// Another value is there now, or the file cannot be read (the error, as a string).
    Changed(serde_json::Value),
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
            Self::Gone => "gone",
            Self::Off => "off",
        }
    }
}

/// Read `edit`'s place in its file now. `off` says swarm set it back.
pub fn state(edit: &Edit, off: bool) -> State {
    match live(edit) {
        Ok(Some(value)) if value == edit.wrote => State::Present,
        Ok(value) if off && value == edit.before => State::Off,
        Ok(Some(value)) => State::Changed(value),
        Ok(None) => State::Gone,
        Err(error) => State::Changed(error.into()),
    }
}

/// The value at `edit`'s place, None when it is absent. An array item is either there or absent.
/// A file that cannot be read or parsed, or a container that is not one, is an error, so swarm
/// never takes it for absent (R4).
fn live(edit: &Edit) -> Result<Option<serde_json::Value>, String> {
    let Some(text) = read_optional(&edit.file)? else {
        return Ok(None);
    };
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
    let mut value = json_object(&edit.file, Some(&text))?;
    for key in keys {
        match value.get_mut(key.as_str()) {
            None => return Ok(None),
            Some(inner) if inner.is_object() || inner.is_array() => value = inner.take(),
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
    /// The wire form. `found` is the live value only for `changed` (A3); `before` null and absent
    /// mean the same: the place was absent (A5, J2).
    pub fn json(&self) -> serde_json::Value {
        let found = match &self.state {
            State::Changed(value) => value.clone(),
            _ => serde_json::Value::Null,
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
            "recorded": self.recorded,
            "at_s": self.at_s,
            "with": self.edit.with,
        })
    }
}

/// Each recorded item with its live state, then each of `found` that equals swarm's text in its
/// file and has no row. `found` is every item swarm's writers own today; list writes no row.
pub fn list(store: &rusqlite::Connection, found: &[Edit]) -> Result<Vec<Entry>, String> {
    let failed = |error: rusqlite::Error| format!("swarm: cannot read managed edits: {error}");
    let mut query = store
        .prepare(
            "SELECT writer, file, kind, path, wrote, before, created, with_id, at_s, off
             FROM managed_edit ORDER BY file, path",
        )
        .map_err(failed)?;
    let rows = query
        .query_map([], |row| {
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
        let edit = Edit {
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
        };
        let state = state(&edit, off);
        entries.push(Entry {
            edit,
            state,
            recorded: true,
            at_s: Some(at_s),
        });
    }
    let mut ids: std::collections::HashSet<String> =
        entries.iter().map(|entry| entry.edit.id()).collect();
    for edit in found {
        if state(edit, false) == State::Present && ids.insert(edit.id()) {
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
) -> Result<Vec<FilePlan>, String> {
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
    Ok(files
        .into_iter()
        .map(|file| {
            let items: Vec<&Entry> = chosen
                .iter()
                .copied()
                .filter(|entry| entry.edit.file == *file)
                .collect();
            removal(file, &items).unwrap_or_else(|error| FilePlan::unreadable(file.clone(), error))
        })
        .collect())
}

/// The plan for one file that removes each present item of `items`.
fn removal(file: &std::path::Path, items: &[&Entry]) -> Result<FilePlan, String> {
    let before = read_text(file)?;
    let mut conflicts = Vec::new();
    let mut gone = Vec::new();
    for entry in items {
        let edit = &entry.edit;
        match &entry.state {
            State::Present => gone.push(edit),
            State::Changed(found) => conflicts.push(Conflict {
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
            State::Gone | State::Off => {}
        }
    }
    let after = match gone.is_empty() {
        true => before.clone(),
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
/// object or array the write made removed again once it is empty. An item of a Codex `hooks.json`
/// array with another item after it is a conflict, because Codex keys hook trust by place, so the
/// later groups would move and lose their trust.
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
                if index + 1 < items.len()
                    && file.file_name().is_some_and(|name| name == "hooks.json")
                    && edit.path.first().is_some_and(|key| key == "hooks")
                {
                    conflicts.push(Conflict {
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

/// Run `change` while holding `lock`, so two launches cannot both read a settings file and the
/// second write drop the first one's trust entry.
pub fn with_lock<T>(
    lock: &std::path::Path,
    change: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(lock)
        .map_err(|error| format!("swarm: cannot open {}: {error}", lock.display()))?;
    file.lock()
        .map_err(|error| format!("swarm: cannot lock {}: {error}", lock.display()))?;
    change()
}

/// A file's text, or "" for a missing file.
pub(crate) fn read_text(path: &std::path::Path) -> Result<String, String> {
    Ok(read_optional(path)?.unwrap_or_default())
}

/// A file's text, or None for a missing file. A file that cannot be read, such as one that is not
/// UTF-8, is an error, so it is never written over whole.
pub(crate) fn read_optional(path: &std::path::Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("swarm: cannot read {}: {error}", path.display())),
    }
}

/// A JSON file's text ("" for a missing file) and its object.
pub(crate) fn read_json_object(
    path: &std::path::Path,
) -> Result<(String, serde_json::Value), String> {
    let text = read_optional(path)?;
    let value = json_object(path, text.as_deref())?;
    Ok((text.unwrap_or_default(), value))
}

/// `text` as a JSON object; a missing file is an empty object. An empty file is refused, because
/// another program may be writing it now.
pub(crate) fn json_object(
    path: &std::path::Path,
    text: Option<&str>,
) -> Result<serde_json::Value, String> {
    let Some(text) = text else {
        return Ok(serde_json::json!({}));
    };
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("swarm: cannot parse {}: {error}", path.display()))?;
    value
        .is_object()
        .then_some(value)
        .ok_or_else(|| format!("swarm: {} is not a JSON object", path.display()))
}

/// Replace `path` in one rename, with the old file's permissions, because `~/.claude.json` holds
/// credentials and a running CLI may read it at any moment.
pub(crate) fn write_json(
    path: &std::path::Path,
    before: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    write_text(path, before, &json_text(value))
}

pub(crate) fn json_text(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON serialization cannot fail") + "\n"
}

/// The end of the error of a write that found its file changed since the read.
const CHANGED: &str = "changed while swarm edited it; run the command again";

pub(crate) fn changed_error(path: &std::path::Path) -> String {
    format!("swarm: {} {CHANGED}", path.display())
}

/// A launch edits a file that a running CLI may rewrite at any moment, such as `~/.claude.json`,
/// so an edit that found its file changed reads it again and tries again, up to 3 more times.
pub(crate) fn retried<T>(mut edit: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    for _ in 0..3 {
        match edit() {
            Err(error) if error.ends_with(CHANGED) => {}
            result => return result,
        }
    }
    edit()
}

/// Where a write to `path` lands: the file at the end of its links, or `path` itself. A link, or a
/// chain of links, to a missing file gets that file made, as a write through the link would, but
/// never a missing folder. A chain longer than the system's 32 hops is refused.
fn write_target(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let fail = |error: std::io::Error| format!("swarm: cannot write {}: {error}", path.display());
    if !std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()) {
        return Ok(path.to_path_buf());
    }
    match std::fs::canonicalize(path) {
        Ok(target) => Ok(target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut target = path.to_path_buf();
            for _ in 0..32 {
                if !std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_symlink()) {
                    break;
                }
                let link = std::fs::read_link(&target).map_err(fail)?;
                target = target.parent().unwrap_or(&target).join(link);
            }
            if std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_symlink()) {
                return Err(format!("swarm: {} links too many times", path.display()));
            }
            match target.parent() {
                Some(dir) if dir.is_dir() => Ok(target),
                _ => Err(format!(
                    "swarm: {} links to {}, whose folder is missing",
                    path.display(),
                    target.display()
                )),
            }
        }
        Err(error) => Err(fail(error)),
    }
}

/// A file that swarm cannot edit is the owner's lock, and a rename would replace it anyway, so
/// swarm refuses it. An open for write, with no truncate, changes nothing and fails for a
/// read-only mode, Finder's Locked box, an ACL, or another user's file. The folder needs its
/// write bit for the rename. Checks the file at the end of `path`'s links.
pub(crate) fn refuse_read_only(path: &std::path::Path) -> Result<(), String> {
    let target = write_target(path)?;
    let refused =
        |error: std::io::Error| format!("swarm: cannot edit {}: {error}", target.display());
    match std::fs::OpenOptions::new().write(true).open(&target) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(refused(error)),
        _ => {}
    }
    // ponytail: mode bits only; a locked or ACL-denied folder fails at the rename, add an
    // access(2) check if that case is seen.
    match target.parent() {
        Some(dir) if std::fs::metadata(dir).is_ok_and(|meta| meta.permissions().readonly()) => {
            Err(format!("swarm: {} is read-only", dir.display()))
        }
        _ => Ok(()),
    }
}

/// Replace `path` in one rename, with the old file's permissions; a hard-linked TOML file is written
/// in place. A linked file is replaced at its target, because a rename onto the link itself would
/// replace the owner's link (ADR 0036).
/// `before` is the text the edit was made from ("" for a missing file); a file that another
/// program changed since then is refused, not written over, and a file that already holds `text`
/// is done, as when an earlier plan of the same setup wrote it through another link.
pub(crate) fn write_text(path: &std::path::Path, before: &str, text: &str) -> Result<(), String> {
    let fail = |error: std::io::Error| format!("swarm: cannot write {}: {error}", path.display());
    let target = write_target(path)?;
    let path = target.as_path();
    refuse_read_only(path)?;
    let now = read_optional(path)?.unwrap_or_default();
    if now == text {
        return Ok(());
    }
    if now != before {
        return Err(changed_error(path));
    }
    // A rename would split a hard-linked Codex config.toml from the owner's other name, so it is
    // written in place, as the base did. A JSON file is renamed, as at the base, because a running
    // CLI may read `~/.claude.json` at any moment.
    let toml = path
        .extension()
        .is_some_and(|extension| extension == "toml");
    if toml
        && std::fs::metadata(path)
            .is_ok_and(|meta| std::os::unix::fs::MetadataExt::nlink(&meta) > 1)
    {
        return std::fs::write(path, text).map_err(fail);
    }
    let dir = path
        .parent()
        .ok_or_else(|| format!("swarm: {} has no parent", path.display()))?;
    // A linked file's folder exists already (`write_target`); this makes a missing ~/.gemini/config.
    std::fs::create_dir_all(dir).map_err(fail)?;
    let tmp = dir.join(format!(
        ".{}.swarm-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let permissions = std::fs::metadata(path)
        .map(|meta| meta.permissions())
        .unwrap_or_else(|_| std::os::unix::fs::PermissionsExt::from_mode(0o600));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
        .and_then(|()| std::fs::set_permissions(&tmp, permissions))
        .map_err(fail)
        .and_then(
            |()| match read_optional(path)?.unwrap_or_default() == before {
                true => std::fs::rename(&tmp, path).map_err(fail),
                false => Err(changed_error(path)),
            },
        )
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
}

/// The unified diff of one planned file.
pub fn diff(plan: &FilePlan) -> String {
    crate::diff::unified(&plan.path.to_string_lossy(), &plan.before, &plan.after)
}

/// Each conflict with its fix, and a last line that says no file was written; None without one.
pub fn conflicts_text(plans: &[FilePlan]) -> Option<String> {
    let conflicts: Vec<_> = plans.iter().flat_map(|plan| &plan.conflicts).collect();
    if conflicts.is_empty() {
        return None;
    }
    let mut text: String = conflicts
        .iter()
        .map(|conflict| {
            format!(
                "conflict: {} {}\n  found:  {}\n  wanted: {}\n  fix:    {}\n\n",
                conflict.file, conflict.entry, conflict.found, conflict.wanted, conflict.fix
            )
        })
        .collect();
    let plural = if conflicts.len() == 1 { "" } else { "s" };
    text += &format!("{} conflict{plural}. No file written.", conflicts.len());
    Some(text)
}

/// Each diff, then the conflicts, or else `unchanged` when no file changes, or else the line that
/// names `apply`, the command that writes the plan.
pub fn plan_text(plans: &[FilePlan], apply: &str, unchanged: &str) -> String {
    let diffs: String = plans.iter().map(diff).collect();
    let last = match conflicts_text(plans) {
        Some(conflicts) => format!("\n{conflicts}"),
        None if diffs.is_empty() => unchanged.into(),
        None => format!("\nPlan only. No file written. Run `{apply}` to apply."),
    };
    format!("{diffs}{last}\n")
}

/// The plan for the app: the digest that apply checks, each file that changes with its diff, and
/// each conflict.
pub fn plan_json(plans: &[FilePlan]) -> serde_json::Value {
    serde_json::json!({
        "digest": digest(plans),
        "files": plans
            .iter()
            .filter(|plan| plan.after != plan.before)
            .map(|plan| serde_json::json!({"path": plan.path.to_string_lossy(), "diff": diff(plan)}))
            .collect::<Vec<_>>(),
        "conflicts": plans.iter().flat_map(|plan| &plan.conflicts).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A TOML bool keeps its type: a writer that turns `ui.sound.enabled` from true to false, as
    /// the Herdr switch will, is present while the file says false, and a revert puts true back
    /// with every other byte as it was.
    #[test]
    fn a_typed_toml_value_is_recorded_compared_and_set_back() {
        let dir = std::env::temp_dir().join(format!("swarm-managed-bool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        let owners = "# my herdr\n[ui.sound]\nenabled = true # loud\nvolume = 3\n";
        std::fs::write(&file, owners).unwrap();
        let store = crate::store::open(std::path::Path::new(":memory:")).unwrap();
        let edit = Edit {
            before: Some(true.into()),
            ..Edit::new(
                Writer::Herdr,
                &file,
                Kind::TomlKey,
                &["ui", "sound", "enabled"],
                false.into(),
            )
        };
        let quiet = owners.replace("enabled = true # loud", "enabled = false # loud");
        let plan = FilePlan {
            path: file.clone(),
            before: owners.into(),
            after: quiet.clone(),
            conflicts: Vec::new(),
            edits: vec![edit.clone()],
        };
        apply(&store, &[plan]).unwrap();
        assert_eq!(state(&edit, false), State::Present);
        let entries = list(&store, &[]).unwrap();
        assert_eq!(entries[0].json()["writer"], "herdr");
        assert_eq!(entries[0].json()["wrote"], false);
        assert_eq!(entries[0].json()["before"], true);

        let plans = revert_plan(&store, &[], &Target::Ids(vec![edit.id()])).unwrap();
        assert!(plans[0].conflicts.is_empty());
        revert(&store, &plans).unwrap();
        let back = std::fs::read_to_string(&file).unwrap();
        assert_eq!(back, owners);
        assert_eq!(state(&edit, true), State::Off);

        // A string where swarm wrote a bool is another value, not swarm's.
        std::fs::write(&file, quiet.replace("false", "\"false\"")).unwrap();
        assert_eq!(state(&edit, false), State::Changed("false".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
