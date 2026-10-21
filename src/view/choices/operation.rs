use super::super::tree::Tree;
use super::{OwnerChoices, ViewState, state::object, tabs::apply_tab};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

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
    /// One owner preference under `prefs`, in the Swift `Prefs` keys: `theme`, `sendKey`, `fontSans`, `fontMono`, `usageHUDAsked`, or a
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
        let chat = super::super::navigation::selected_chat(tree, owner, state, &workspace)
            .map(|c| c.id.clone());
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
                // Unknown liveness keeps the tab; is_running alone treats it as ended (ADR 0098).
                && old.agents.iter().all(|agent| agent["alive"] == false)
                && !owner.tab_chats.contains(&old.key)
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
            if fields.iter().any(|f| !super::super::fields::known_field(f)) {
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
                "fontSans" => matches!(value.as_str(), Some("system" | "helvetica" | "avenir")),
                "fontMono" => matches!(value.as_str(), Some("system" | "menlo" | "monaco")),
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

fn validate_fold(tree: &Tree, id: &str) -> Result<(), String> {
    if id == "pinned"
        || tree.workspace(id).is_some()
        || tree
            .projects
            .iter()
            .any(|p| format!("project:{}", p.path) == id)
        || tree.chats().any(|c| c.row_id() == id)
        || id
            .strip_prefix("archived:")
            .and_then(|path| tree.workspace(path))
            .is_some_and(|(_, w)| !w.archived.is_empty())
        || id.strip_prefix("archived:project:").is_some_and(|path| {
            tree.projects
                .iter()
                .any(|p| p.path == path && p.workspaces.iter().any(|w| !w.archived.is_empty()))
        })
    {
        Ok(())
    } else {
        Err("unknown fold id".into())
    }
}
