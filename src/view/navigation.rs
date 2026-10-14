use super::{
    choices::{OwnerChoices, ViewState},
    tree::{Project, SidebarRow, Tree, Workspace, basename, nonblank},
};
use std::collections::BTreeMap;

pub fn project_title(project: &Project, owner: &OwnerChoices) -> String {
    nonblank(owner.project_names.get(&project.path).map(String::as_str))
        .unwrap_or_else(|| project.name.clone())
}
pub fn workspace_title(
    project: &Project,
    workspace: &Workspace,
    owner: &OwnerChoices,
    in_project: bool,
) -> String {
    if let Some(name) = nonblank(owner.names.get(&workspace.path).map(String::as_str)) {
        return name;
    }
    let folder = basename(&workspace.path);
    if folder.is_empty() {
        return workspace.path.clone();
    }
    if in_project {
        return if folder == project.name {
            workspace.branch.clone().unwrap_or(folder)
        } else {
            folder
        };
    }
    let project_name = project_title(project, owner);
    if folder == project.name {
        project_name
    } else {
        format!("{project_name} / {folder}")
    }
}
pub fn title_key(title: &str) -> String {
    title.to_lowercase().replace('ß', "ss").replace('ς', "σ")
}
pub fn path_qualifier(path: &str, others: &[String]) -> String {
    let components: Vec<_> = path.split('/').filter(|s| !s.is_empty()).collect();
    for length in 1..=components.len() {
        let suffix = &components[components.len() - length..];
        if others.iter().all(|p| {
            let parts: Vec<_> = p.split('/').filter(|s| !s.is_empty()).collect();
            parts.len() < length || parts[parts.len() - length..] != *suffix
        }) {
            return if length == components.len() {
                path.into()
            } else {
                suffix.join("/")
            };
        }
    }
    path.into()
}
pub fn workspace_detail(
    project: &Project,
    workspace: &Workspace,
    owner: &OwnerChoices,
    in_project: bool,
    ids: &BTreeMap<String, Vec<String>>,
) -> String {
    let title = workspace_title(project, workspace, owner, in_project);
    let others: Vec<_> = ids
        .get(&title_key(&title))
        .into_iter()
        .flatten()
        .filter(|p| *p != &workspace.path)
        .cloned()
        .collect();
    let custom = nonblank(owner.names.get(&workspace.path).map(String::as_str)).is_some();
    let mut parts = Vec::new();
    if !others.is_empty() {
        parts.push(path_qualifier(&workspace.path, &others));
    }
    let project_name = project_title(project, owner);
    if !in_project && custom && !parts.contains(&project_name) {
        parts.push(project_name);
    }
    if let Some(branch) = &workspace.branch
        && !parts.contains(branch)
        && branch != &title
        && (custom || *branch != basename(&workspace.path))
    {
        parts.push(branch.clone());
    }
    parts.push(super::fields::count_text(
        workspace.listed_chats(owner).count(),
        "chat",
    ));
    parts.join(" · ")
}
pub fn selected_chat<'a>(
    tree: &'a Tree,
    owner: &OwnerChoices,
    state: &ViewState,
    workspace: &str,
) -> Option<&'a super::tree::Chat> {
    let (_, entry) = tree.workspace(workspace)?;
    let saved = state
        .selected_chats
        .get(workspace)
        .and_then(|id| entry.chat(id));
    if let Some(strip) = owner.tabs.get(workspace) {
        if let Some(chat) = saved
            && strip.open.contains(&chat.key)
        {
            return Some(chat);
        }
        let key = strip.selection_after_close(
            state
                .tab_history
                .get(workspace)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        )?;
        entry.chat(key)
    } else {
        saved.or_else(|| entry.chats.first())
    }
}
pub fn selected_row(
    rows: &[SidebarRow],
    workspace: Option<&str>,
    chat: Option<&super::tree::Chat>,
    child: Option<&str>,
) -> Option<String> {
    if let Some(child) = child
        && let Some(chat) = chat
        && rows.iter().any(|r| {
            r.id == child
                && r.selection.as_ref().and_then(|s| s.chat.as_deref()) == Some(chat.id.as_str())
        })
    {
        return Some(child.into());
    }
    if let Some(chat) = chat
        && rows.iter().any(|r| r.id == chat.row_id())
    {
        return Some(chat.row_id());
    }
    workspace.map(str::to_owned)
}
