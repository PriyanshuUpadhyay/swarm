use super::fields::{FieldValue, LineTotals, PrFact, WorkspaceFields};
use serde::Serialize;

use super::{
    choices::{OwnerChoices, ViewState},
    tree::{
        AgentStatus, ArchivedChat, Project, StatusCount, Tree, Workspace, age, basename, nonblank,
    },
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarRow {
    pub id: String,
    pub kind: &'static str,
    pub depth: u8,
    pub parent_id: Option<String>,
    pub expanded: bool,
    pub has_children: bool,
    pub children_summary: Option<String>,
    pub title: String,
    pub detail: String,
    pub status: Option<AgentStatus>,
    pub counts: Vec<StatusCount>,
    pub age: Option<String>,
    pub help: String,
    pub pinned: bool,
    pub archived: bool,
    pub missing: bool,
    pub new_chat_enabled: bool,
    /// A new chat on this row gets its own worktree (ADR 0146).
    pub starts_chat_worktree: bool,
    pub dimmed: bool,
    pub fields: Vec<FieldValue>,
    pub selection: Option<Selection>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub workspace: String,
    pub chat: Option<String>,
    pub agent_session: Option<String>,
    pub agent: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    pub kind: &'static str,
    pub path: Option<String>,
    /// The workspace row also serves as the header when it stands for a plain folder.
    pub folder: bool,
    pub collapse_id: String,
    pub expanded: bool,
    pub title: String,
    pub status: Option<AgentStatus>,
    pub fields: Vec<FieldValue>,
    pub rows: Vec<SidebarRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

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
    parts.push(super::fields::count_text(workspace.chats.len(), "chat"));
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
#[derive(Serialize)]
pub struct ProjectNav {
    pub path: String,
    pub title: String,
    pub folder: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceNav {
    pub project_path: String,
    pub title: String,
    pub missing: bool,
    pub archived: bool,
    pub new_chat_enabled: bool,
    pub starts_chat_worktree: bool,
    pub branch: Option<String>,
    pub diff: Option<LineTotals>,
    pub pr: Option<PrFact>,
}

pub fn navigation_catalog(
    tree: &Tree,
    owner: &OwnerChoices,
    cache: &BTreeMap<String, WorkspaceFields>,
) -> (Vec<ProjectNav>, BTreeMap<String, WorkspaceNav>) {
    let mut projects = Vec::new();
    let mut workspaces = BTreeMap::new();
    for p in &tree.projects {
        projects.push(ProjectNav {
            path: p.path.clone(),
            title: project_title(p, owner),
            folder: is_folder(p),
        });
        for w in &p.workspaces {
            let facts = cache.get(&w.path);
            let archived = owner.archived.contains(&w.path);
            workspaces.insert(
                w.path.clone(),
                WorkspaceNav {
                    project_path: p.path.clone(),
                    title: workspace_heading(p, w, owner, true),
                    missing: w.missing,
                    archived,
                    new_chat_enabled: !w.missing && !w.removed && !archived,
                    starts_chat_worktree: w.hub_main,
                    branch: w.branch.clone(),
                    diff: facts.and_then(|f| f.diff).filter(|_| w.branch.is_some()),
                    pr: facts
                        .and_then(|f| f.pr_fact.clone())
                        .filter(|_| w.branch.is_some()),
                },
            );
        }
    }
    (projects, workspaces)
}

pub(super) fn is_folder(project: &Project) -> bool {
    super::lifecycle::common(project).is_none() && project.workspaces.len() == 1
}

pub(super) fn workspace_heading(
    p: &Project,
    w: &Workspace,
    owner: &OwnerChoices,
    in_project: bool,
) -> String {
    if w.removed {
        w.name.clone()
    } else {
        workspace_title(p, w, owner, in_project && !is_folder(p))
    }
}

pub(super) fn archived_fold(
    id: String,
    chats: &[(&ArchivedChat, Option<String>)],
    state: &ViewState,
    now: i64,
) -> Vec<SidebarRow> {
    if chats.is_empty() {
        return Vec::new();
    }
    let expanded = !state.is_collapsed(&id);
    let title = format!("Archived · {}", chats.len());
    let field = |text| FieldValue {
        field: "title".into(),
        text,
        status: None,
    };
    let mut rows = vec![SidebarRow {
        id: id.clone(),
        kind: "archived",
        expanded,
        has_children: true,
        title: title.clone(),
        archived: true,
        dimmed: true,
        help: "Archived chats here, from the 50 newest of all workspaces".into(),
        fields: vec![field(title)],
        ..Default::default()
    }];
    if expanded {
        rows.extend(chats.iter().map(|(chat, workspace)| SidebarRow {
            id: format!("archived-chat:{}", chat.id),
            kind: "archived-chat",
            depth: 1,
            parent_id: Some(id.clone()),
            title: chat.title.clone(),
            detail: workspace.clone().unwrap_or_default(),
            age: Some(age(chat.archived_at, now)),
            help: format!("{}\nArchived. Unarchive to open it.", chat.title),
            archived: true,
            dimmed: true,
            fields: vec![field(chat.title.clone())],
            ..Default::default()
        }));
    }
    rows
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
