use super::super::{fields::RowFieldLists, tabs::TabStrip, tree::Tree};
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
pub(super) fn object() -> Value {
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
    /// A chat's children and a workspace's archived chats start folded, so their stored key
    /// means expanded.
    fn folded_first(id: &str) -> bool {
        id.starts_with("chat:") || id.starts_with("archived:")
    }
    fn collapse_key(id: &str) -> String {
        if Self::folded_first(id) {
            format!("expanded:{id}")
        } else if id.starts_with('/') {
            format!("workspace:{id}")
        } else {
            id.into()
        }
    }
    pub fn is_collapsed(&self, id: &str) -> bool {
        let stored = self.collapsed.contains(&Self::collapse_key(id));
        if Self::folded_first(id) {
            !stored
        } else {
            stored
        }
    }
    pub fn fold(&mut self, id: &str, collapsed: bool) {
        let key = Self::collapse_key(id);
        if collapsed != Self::folded_first(id) {
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
