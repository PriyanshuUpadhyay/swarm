use super::{
    fields::RowFieldLists,
    tabs::{Grouping, TabStrip},
    tree::Tree,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn null_default<'de, D, T>(decoder: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(decoder)?.unwrap_or_default())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OwnerChoices {
    #[serde(deserialize_with = "null_default")]
    pub pinned: BTreeSet<String>,
    #[serde(deserialize_with = "null_default")]
    pub archived: BTreeSet<String>,
    #[serde(deserialize_with = "null_default")]
    pub names: BTreeMap<String, String>,
    #[serde(deserialize_with = "null_default")]
    pub chat_names: BTreeMap<String, String>,
    #[serde(deserialize_with = "null_default")]
    pub project_names: BTreeMap<String, String>,
    #[serde(deserialize_with = "null_default")]
    pub project_paths: Vec<String>,
    #[serde(deserialize_with = "null_default")]
    pub removed_projects: BTreeSet<String>,
    #[serde(deserialize_with = "null_default")]
    pub plain_folders: BTreeSet<String>,
    #[serde(deserialize_with = "null_default")]
    pub project_defaults: BTreeMap<String, Value>,
    #[serde(deserialize_with = "null_default")]
    pub workspace_order: BTreeMap<String, Vec<String>>,
    #[serde(deserialize_with = "null_default")]
    pub tabs: BTreeMap<String, TabStrip>,
    /// Chat keys started in a tab. They show only as tabs, not in the sidebar (ADR 0085).
    #[serde(deserialize_with = "null_default")]
    pub tab_chats: BTreeSet<String>,
    /// Chat keys from another adapter that the owner imported, so the app lists them (ADR 0085).
    #[serde(deserialize_with = "null_default")]
    pub imported: BTreeSet<String>,
    #[serde(deserialize_with = "null_default")]
    pub fields: RowFieldLists,
    #[serde(default = "object")]
    pub prefs: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
fn object() -> Value {
    serde_json::json!({})
}

impl Default for OwnerChoices {
    fn default() -> Self {
        Self {
            pinned: Default::default(),
            archived: Default::default(),
            names: Default::default(),
            chat_names: Default::default(),
            project_names: Default::default(),
            project_paths: Default::default(),
            removed_projects: Default::default(),
            plain_folders: Default::default(),
            project_defaults: Default::default(),
            workspace_order: Default::default(),
            tabs: Default::default(),
            tab_chats: Default::default(),
            imported: Default::default(),
            fields: Default::default(),
            prefs: object(),
            extra: Default::default(),
        }
    }
}

impl OwnerChoices {
    /// Drops the imported and tab-only keys of chats that are gone from the tree, such as an
    /// archived chat. Returns whether a key was dropped.
    pub fn forget_gone_chats(&mut self, tree: &Tree) -> bool {
        let known: BTreeSet<&str> = tree
            .projects
            .iter()
            .flat_map(|p| &p.workspaces)
            .flat_map(|w| w.chats.iter().chain(&w.unlisted))
            .chain(tree.importable.iter().map(|(_, c)| c))
            .map(|c| c.key.as_str())
            .collect();
        let before = self.imported.len() + self.tab_chats.len();
        self.imported.retain(|k| known.contains(k.as_str()));
        self.tab_chats.retain(|k| known.contains(k.as_str()));
        before != self.imported.len() + self.tab_chats.len()
    }

    pub fn remove_project(&mut self, path: &str, workspaces: &[String]) {
        let removed: BTreeSet<_> = workspaces
            .iter()
            .cloned()
            .chain([path.into(), format!("{path}#removed")])
            .collect();
        self.pinned.retain(|k| !removed.contains(k));
        self.archived.retain(|k| !removed.contains(k));
        self.names.retain(|k, _| !removed.contains(k));
        self.project_names.remove(path);
        self.plain_folders.remove(path);
        self.project_defaults.remove(path);
        self.workspace_order.remove(path);
        self.tabs.retain(|k, _| !removed.contains(k));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ViewState {
    pub selected_workspace: Option<String>,
    pub selected_chats: BTreeMap<String, String>,
    pub tab_history: BTreeMap<String, Vec<String>>,
    pub dismissed_children: BTreeMap<String, Vec<String>>,
    pub collapsed: BTreeSet<String>,
    pub last_seen: BTreeMap<String, i64>,
    pub expanded_lists: BTreeSet<String>,
    pub selected_child: Option<String>,
    pub pending: Vec<PendingTab>,
    pub closing: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingTab {
    pub id: String,
    pub workspace: String,
    pub session: Option<String>,
    pub state: String,
}

impl ViewState {
    fn collapse_key(id: &str) -> String {
        if id.starts_with("chat:") {
            format!("expanded:{id}")
        } else if id.starts_with('/') {
            format!("workspace:{id}")
        } else {
            id.into()
        }
    }
    pub fn is_collapsed(&self, id: &str) -> bool {
        let stored = self.collapsed.contains(&Self::collapse_key(id));
        if id.starts_with("chat:") {
            !stored
        } else {
            stored
        }
    }
    pub fn fold(&mut self, id: &str, collapsed: bool) {
        let key = Self::collapse_key(id);
        if collapsed != id.starts_with("chat:") {
            self.collapsed.insert(key);
        } else {
            self.collapsed.remove(&key);
        }
    }
    pub fn record_first_sight(&mut self, tree: &Tree) {
        let chats: BTreeMap<_, _> = tree
            .chats()
            .map(|c| (c.key.clone(), c.last_activity))
            .collect();
        self.last_seen.retain(|k, _| chats.contains_key(k));
        self.dismissed_children.retain(|k, _| chats.contains_key(k));
        for (key, at) in chats {
            self.last_seen.entry(key).or_insert(at);
        }
    }
    pub fn is_unread(&self, key: &str, activity: i64) -> bool {
        self.last_seen.get(key).is_some_and(|seen| activity > *seen)
    }
    pub fn mark_seen(&mut self, key: &str, now: i64, activity: i64) {
        let seen = self.last_seen.entry(key.into()).or_default();
        *seen = (*seen).max(now).max(activity);
    }
    pub fn record_selection(&mut self, workspace: &str, key: &str) {
        let history = self.tab_history.entry(workspace.into()).or_default();
        history.retain(|k| k != key);
        history.push(key.into());
        if history.len() > 20 {
            history.drain(..history.len() - 20);
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChoiceWrite {
    pub revision: String,
    #[serde(flatten)]
    pub operation: ChoiceOp,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChoiceOp {
    Pin {
        workspace: String,
    },
    Unpin {
        workspace: String,
    },
    ReorderWorkspace {
        workspace: String,
        target: String,
    },
    Collapse {
        id: String,
    },
    Expand {
        id: String,
    },
    ExpandList {
        workspace: String,
    },
    RenameWorkspace {
        workspace: String,
        name: String,
    },
    RenameChat {
        workspace: String,
        chat: String,
        name: String,
    },
    RenameProject {
        project: String,
        name: String,
    },
    SetupScript {
        project: String,
        setup_script: String,
    },
    OpenTab {
        workspace: String,
        chat: String,
        /// A chat started in a tab: it stays out of the sidebar.
        #[serde(default)]
        tab_only: bool,
    },
    ImportChat {
        workspace: String,
        chat: String,
    },
    CloseTab {
        workspace: String,
        chat: String,
    },
    MoveTab {
        workspace: String,
        chat: String,
        target: String,
    },
    StepTab {
        workspace: String,
        chat: String,
        target: String,
    },
    TabGroupCreate {
        workspace: String,
        id: String,
        name: String,
        color: String,
        chat: String,
    },
    TabGroupRename {
        workspace: String,
        group: String,
        name: String,
    },
    TabGroupColor {
        workspace: String,
        group: String,
        color: String,
    },
    TabGroupDelete {
        workspace: String,
        group: String,
    },
    TabGroupFold {
        workspace: String,
        group: String,
        folded: bool,
    },
    MoveTabToGroup {
        workspace: String,
        chat: String,
        group: Option<String>,
    },
    SelectedChat {
        workspace: String,
        chat: Option<String>,
        #[serde(default = "yes")]
        recording_history: bool,
    },
    Seen {
        workspace: String,
        chat: String,
    },
    RowFields {
        list: String,
        fields: Vec<String>,
    },
    /// One owner preference under `prefs`, in the Swift `Prefs` keys: `theme`, `sendKey`, `usageHUDAsked`, or a
    /// notice switch `notices.post|sound|done|badge` that `swarm notices --due` reads (ADR 0077).
    Pref {
        key: String,
        value: Value,
    },
    MuteProject {
        project: String,
        muted: bool,
    },
}
fn yes() -> bool {
    true
}

pub fn apply(
    owner: &mut OwnerChoices,
    state: &mut ViewState,
    tree: &Tree,
    op: ChoiceOp,
    now: i64,
) -> Result<(), String> {
    let op = if let ChoiceOp::SelectedChat {
        workspace,
        chat: None,
        recording_history,
    } = op
    {
        let chat =
            super::navigation::selected_chat(tree, owner, state, &workspace).map(|c| c.id.clone());
        ChoiceOp::SelectedChat {
            workspace,
            chat,
            recording_history,
        }
    } else {
        op
    };
    match op {
        ChoiceOp::Pin { workspace } => {
            tree.workspace(&workspace).ok_or("unknown workspace")?;
            if owner.archived.contains(&workspace) {
                return Err("archived workspace cannot be pinned".into());
            }
            owner.pinned.insert(workspace);
        }
        ChoiceOp::Unpin { workspace } => {
            tree.workspace(&workspace).ok_or("unknown workspace")?;
            owner.pinned.remove(&workspace);
        }
        ChoiceOp::ReorderWorkspace { workspace, target } => {
            let (project, _) = tree.workspace(&workspace).ok_or("unknown workspace")?;
            let (other, _) = tree.workspace(&target).ok_or("unknown target workspace")?;
            if project.id != other.id
                || workspace == target
                || owner.archived.contains(&workspace)
                || owner.archived.contains(&target)
            {
                return Err("workspace move must stay in one active project".into());
            }
            let mut paths: Vec<_> = project.workspaces.iter().map(|w| w.path.clone()).collect();
            let start = paths.iter().position(|p| p == &workspace).unwrap();
            let end = paths.iter().position(|p| p == &target).unwrap();
            paths.remove(start);
            paths.insert(end, workspace);
            owner.workspace_order.insert(project.path.clone(), paths);
        }
        ChoiceOp::Collapse { id } => {
            validate_fold(tree, &id)?;
            state.fold(&id, true);
        }
        ChoiceOp::Expand { id } => {
            validate_fold(tree, &id)?;
            state.fold(&id, false);
        }
        ChoiceOp::ExpandList { workspace } => {
            tree.workspace(&workspace).ok_or("unknown workspace")?;
            state.expanded_lists.insert(workspace);
        }
        ChoiceOp::RenameWorkspace { workspace, name } => {
            tree.workspace(&workspace).ok_or("unknown workspace")?;
            rename(&mut owner.names, workspace, name);
        }
        ChoiceOp::RenameProject { project, name } => {
            if !tree.projects.iter().any(|p| p.path == project) {
                return Err("unknown project".into());
            }
            rename(&mut owner.project_names, project, name);
        }
        ChoiceOp::SetupScript {
            project,
            setup_script,
        } => {
            if !tree.projects.iter().any(|p| p.path == project) {
                return Err("unknown project".into());
            }
            if setup_script.contains('\0') || setup_script.len() > 16_384 {
                return Err("setup script is invalid or exceeds 16 KiB".into());
            }
            let defaults = owner.project_defaults.entry(project).or_insert_with(object);
            if !defaults.is_object() {
                return Err("project defaults must be an object".into());
            }
            if setup_script.trim().is_empty() {
                defaults.as_object_mut().unwrap().remove("setup_script");
            } else {
                defaults["setup_script"] = setup_script.into();
            }
        }
        ChoiceOp::RenameChat {
            workspace,
            chat,
            name,
        } => {
            let chat = tree
                .chat_in(&workspace, &chat)
                .ok_or("unknown chat in workspace")?;
            rename(&mut owner.chat_names, chat.key.clone(), name);
        }
        ChoiceOp::ImportChat { workspace, chat } => {
            let (_, chat) = tree
                .importable
                .iter()
                .find(|(w, c)| *w == workspace && c.contains(&chat))
                .ok_or("no chat to import in workspace")?;
            owner.imported.insert(chat.key.clone());
        }
        ChoiceOp::Seen { workspace, chat } => {
            let chat = tree
                .chat_in(&workspace, &chat)
                .ok_or("unknown chat in workspace")?;
            state.mark_seen(&chat.key, now, chat.last_activity);
        }
        ChoiceOp::SelectedChat {
            workspace,
            chat,
            recording_history,
        } => {
            let (_, entry) = tree.workspace(&workspace).ok_or("unknown workspace")?;
            let previous = state.selected_workspace.as_deref().and_then(|w| {
                state
                    .selected_chats
                    .get(w)
                    .map(|id| (w.to_owned(), id.clone()))
            });
            if let Some((previous_workspace, id)) = previous
                && let Some(old) = tree.chat_in(&previous_workspace, &id)
                && old.is_running == Some(false)
                && chat
                    .as_deref()
                    .and_then(|id| tree.chat_in(&workspace, id))
                    .map(|c| &c.key)
                    != Some(&old.key)
                && let Some(strip) = owner.tabs.get_mut(&previous_workspace)
            {
                *strip = strip.closing(&old.key);
                if let Some(history) = state.tab_history.get_mut(&previous_workspace) {
                    history.retain(|k| k != &old.key);
                }
            }
            state.selected_workspace = Some(workspace.clone());
            if let Some(id) = chat {
                let chat = entry.chat(&id).ok_or("unknown chat in workspace")?;
                state
                    .selected_chats
                    .insert(workspace.clone(), chat.id.clone());
                state.mark_seen(&chat.key, now, chat.last_activity);
                if recording_history {
                    state.record_selection(&workspace, &chat.key);
                }
            } else {
                state.selected_chats.remove(&workspace);
            }
            state.selected_child = None;
        }
        ChoiceOp::RowFields { list, fields } => {
            if fields.iter().any(|f| !super::fields::known_field(f)) {
                return Err("unknown row field".into());
            }
            match list.as_str() {
                "project" => owner.fields.project = fields,
                "workspace" => owner.fields.workspace = fields,
                "chat" => owner.fields.chat = fields,
                "tab" => owner.fields.tab = fields,
                _ => return Err("unknown row field list".into()),
            }
        }
        ChoiceOp::Pref { key, value } => {
            let valid = match key.as_str() {
                "theme" => matches!(value.as_str(), Some("system" | "light" | "dark")),
                "sendKey" => matches!(value.as_str(), Some("return" | "commandReturn")),
                "usageHUDAsked" | "notices.post" | "notices.sound" | "notices.done"
                | "notices.badge" => value.is_boolean(),
                _ => return Err("unknown preference".into()),
            };
            if !valid {
                return Err(format!("invalid value for {key}"));
            }
            let (section, name) = key
                .split_once('.')
                .map_or((None, key.as_str()), |(section, name)| {
                    (Some(section), name)
                });
            prefs_object(&mut owner.prefs, section)?.insert(name.into(), value);
        }
        ChoiceOp::MuteProject { project, muted } => {
            if !tree.projects.iter().any(|p| p.path == project) {
                return Err("unknown project".into());
            }
            let paths = prefs_object(&mut owner.prefs, Some("notices"))?
                .entry("mutedProjects")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or("prefs.notices.mutedProjects must be a list")?;
            paths.retain(|path| path != project.as_str());
            if muted {
                paths.push(project.into());
            }
        }
        op => apply_tab(owner, state, tree, op)?,
    }
    Ok(())
}

/// `prefs`, or its `section` object made when absent. A value of another type is refused, not
/// replaced, so a Swift field that Rust does not know survives.
fn prefs_object<'a>(
    prefs: &'a mut Value,
    section: Option<&str>,
) -> Result<&'a mut serde_json::Map<String, Value>, String> {
    let prefs = prefs.as_object_mut().ok_or("prefs must be an object")?;
    match section {
        None => Ok(prefs),
        Some(section) => prefs
            .entry(section)
            .or_insert_with(object)
            .as_object_mut()
            .ok_or_else(|| format!("prefs.{section} must be an object")),
    }
}

fn rename(names: &mut BTreeMap<String, String>, key: String, name: String) {
    if name.trim().is_empty() {
        names.remove(&key);
    } else {
        names.insert(key, name.trim().into());
    }
}

fn apply_tab(
    owner: &mut OwnerChoices,
    state: &mut ViewState,
    tree: &Tree,
    op: ChoiceOp,
) -> Result<(), String> {
    let workspace = match &op {
        ChoiceOp::OpenTab { workspace, .. }
        | ChoiceOp::CloseTab { workspace, .. }
        | ChoiceOp::MoveTab { workspace, .. }
        | ChoiceOp::StepTab { workspace, .. }
        | ChoiceOp::TabGroupCreate { workspace, .. }
        | ChoiceOp::TabGroupRename { workspace, .. }
        | ChoiceOp::TabGroupColor { workspace, .. }
        | ChoiceOp::TabGroupDelete { workspace, .. }
        | ChoiceOp::TabGroupFold { workspace, .. }
        | ChoiceOp::MoveTabToGroup { workspace, .. } => workspace.clone(),
        _ => return Err("invalid tab operation".into()),
    };
    let (_, entry) = tree.workspace(&workspace).ok_or("unknown workspace")?;
    let strip = owner
        .tabs
        .entry(workspace.clone())
        .or_insert_with(|| entry.seed_tabs());
    let key = |id: &str| {
        entry
            .chat(id)
            .map(|c| c.key.clone())
            .ok_or_else(|| "unknown chat in workspace".to_owned())
    };
    let open = |id: &str| -> Result<String, String> {
        let k = key(id)?;
        if !strip.open.contains(&k) {
            return Err("tab is not open".into());
        }
        Ok(k)
    };
    *strip = match op {
        ChoiceOp::OpenTab { chat, tab_only, .. } => {
            if tab_only {
                owner.tab_chats.insert(key(&chat)?);
            }
            let current = state
                .selected_chats
                .get(&workspace)
                .and_then(|id| entry.chat(id))
                .map(|c| c.key.as_str());
            strip.opening(&key(&chat)?, current)
        }
        ChoiceOp::CloseTab { chat, .. } => strip.closing(&open(&chat)?),
        ChoiceOp::MoveTab { chat, target, .. } => {
            let key = open(&chat)?;
            let target = open(&target)?;
            strip.moving(&key, strip.open.iter().position(|k| k == &target).unwrap())
        }
        ChoiceOp::StepTab { chat, target, .. } => strip.stepping(&open(&chat)?, &open(&target)?),
        ChoiceOp::TabGroupCreate {
            id,
            name,
            color,
            chat,
            ..
        } => {
            if !super::tabs::valid_color(&color) {
                return Err("unknown group color".into());
            }
            strip.grouping(Grouping::New {
                id,
                name,
                color,
                tab: open(&chat)?,
            })
        }
        ChoiceOp::TabGroupRename { group, name, .. } => {
            strip.grouping(Grouping::Rename { group, name })
        }
        ChoiceOp::TabGroupColor { group, color, .. } => {
            if !super::tabs::valid_color(&color) {
                return Err("unknown group color".into());
            }
            strip.grouping(Grouping::Color { group, color })
        }
        ChoiceOp::TabGroupDelete { group, .. } => strip.grouping(Grouping::Delete(group)),
        ChoiceOp::TabGroupFold { group, folded, .. } => {
            strip.grouping(Grouping::Fold { group, folded })
        }
        ChoiceOp::MoveTabToGroup { chat, group, .. } => {
            let tab = open(&chat)?;
            strip.grouping(group.map_or_else(
                || Grouping::Remove(tab.clone()),
                |group| Grouping::Add {
                    tab: tab.clone(),
                    group,
                },
            ))
        }
        _ => unreachable!(),
    };
    if let Some(history) = state.tab_history.get_mut(&workspace) {
        history.retain(|k| strip.open.contains(k));
    }
    let selected = state
        .selected_chats
        .get(&workspace)
        .and_then(|id| entry.chat(id));
    if selected.is_none_or(|c| !strip.open.contains(&c.key)) {
        let key = strip.selection_after_close(
            state
                .tab_history
                .get(&workspace)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        if let Some(chat) = key.and_then(|k| entry.chat(k)) {
            state.selected_chats.insert(workspace, chat.id.clone());
        } else {
            state.selected_chats.remove(&workspace);
        }
    }
    Ok(())
}

fn validate_fold(tree: &Tree, id: &str) -> Result<(), String> {
    if id == "pinned"
        || tree.workspace(id).is_some()
        || tree
            .projects
            .iter()
            .any(|p| format!("project:{}", p.path) == id)
        || tree.chats().any(|c| c.row_id() == id)
    {
        Ok(())
    } else {
        Err("unknown fold id".into())
    }
}
