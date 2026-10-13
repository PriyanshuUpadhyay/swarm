use super::{
    choices::{OwnerChoices, ViewState},
    fields::{self, FieldValue, WorkspaceFields},
    tree::*,
};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Serialize)]
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
    pub collapse_id: String,
    pub expanded: bool,
    pub title: String,
    pub status: Option<AgentStatus>,
    pub fields: Vec<FieldValue>,
    pub rows: Vec<SidebarRow>,
}

pub fn sections(
    tree: &Tree,
    owner: &OwnerChoices,
    state: &ViewState,
    archive: bool,
    now: i64,
    cache: &BTreeMap<String, WorkspaceFields>,
) -> Vec<Section> {
    let mut sections = Vec::new();
    let all: Vec<_> = tree
        .projects
        .iter()
        .flat_map(|p| p.workspaces.iter().map(move |w| (p, w)))
        .collect();
    let mut ids_by_title: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (p, w) in &all {
        ids_by_title
            .entry(super::navigation::title_key(
                &super::navigation::workspace_title(p, w, owner, false),
            ))
            .or_default()
            .push(w.path.clone());
    }
    let mut pinned_rows = Vec::new();
    if !archive {
        for (p, w) in &all {
            if owner.pinned.contains(&w.path) && !owner.archived.contains(&w.path) {
                pinned_rows.extend(rows(p, w, owner, state, now, false, &ids_by_title, cache));
            }
        }
        if !pinned_rows.is_empty() {
            sections.push(Section {
                id: "pinned".into(),
                kind: "pinned",
                path: None,
                collapse_id: "pinned".into(),
                expanded: !state.is_collapsed("pinned"),
                title: "Pinned".into(),
                status: AgentStatus::aggregate(pinned_rows.iter().filter_map(|r| r.status)),
                fields: Vec::new(),
                rows: pinned_rows,
            });
        }
    }
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for project in &tree.projects {
        *names
            .entry(super::navigation::project_title(project, owner))
            .or_default() += 1;
    }
    for project in &tree.projects {
        let mut ids = BTreeMap::new();
        for w in &project.workspaces {
            ids.entry(super::navigation::title_key(
                &super::navigation::workspace_title(project, w, owner, true),
            ))
            .or_insert_with(Vec::new)
            .push(w.path.clone());
        }
        let mut project_rows = Vec::new();
        for w in &project.workspaces {
            if if archive {
                owner.archived.contains(&w.path)
            } else {
                !owner.pinned.contains(&w.path) && !owner.archived.contains(&w.path)
            } {
                project_rows.extend(rows(project, w, owner, state, now, true, &ids, cache));
            }
        }
        if archive && project_rows.is_empty() {
            continue;
        }
        let name = super::navigation::project_title(project, owner);
        let parent = Path::new(&project.path)
            .parent()
            .map(|p| basename(&p.to_string_lossy()))
            .unwrap_or_default();
        let title = if names[&name] > 1 && !parent.is_empty() {
            format!("{name} — {parent}")
        } else {
            name
        };
        let status = AgentStatus::aggregate(project_rows.iter().filter_map(|r| r.status));
        let context = fields::workspace_context(&project.workspaces, &title, state, now, cache);
        let collapse_id = format!("project:{}", project.path);
        sections.push(Section {
            id: format!("project:{}", project.id),
            kind: "project",
            path: Some(project.path.clone()),
            expanded: !state.is_collapsed(&collapse_id),
            collapse_id,
            title,
            status,
            fields: context.with_status(status).values(&owner.fields.project),
            rows: project_rows,
        });
    }
    sections
}

#[allow(clippy::too_many_arguments)]
fn rows(
    project: &Project,
    workspace: &Workspace,
    owner: &OwnerChoices,
    state: &ViewState,
    now: i64,
    in_project: bool,
    ids: &BTreeMap<String, Vec<String>>,
    cache: &BTreeMap<String, WorkspaceFields>,
) -> Vec<SidebarRow> {
    let pinned = owner.pinned.contains(&workspace.path);
    let archived = owner.archived.contains(&workspace.path);
    let title = if workspace.removed {
        workspace.name.clone()
    } else {
        super::navigation::workspace_title(project, workspace, owner, in_project)
    };
    let mut detail =
        super::navigation::workspace_detail(project, workspace, owner, in_project, ids);
    if workspace.missing {
        detail.push_str(" · folder missing");
    }
    if let Some(mark) = &workspace.mark {
        detail.push_str(&format!(" · {mark}"));
    }
    let status = workspace.status();
    let mut context =
        fields::workspace_context(std::slice::from_ref(workspace), &title, state, now, cache);
    if owner.fields.workspace.iter().any(|f| f == "title") {
        let suffix = fields::count_text(workspace.chats.len(), "chat");
        context.text.insert(
            "branch".into(),
            detail
                .split(" · ")
                .filter(|p| *p != suffix)
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }
    let mut result = vec![SidebarRow {
        id: workspace.path.clone(),
        kind: "workspace",
        depth: 0,
        parent_id: None,
        expanded: !state.is_collapsed(&workspace.path),
        has_children: !workspace.chats.is_empty(),
        children_summary: None,
        title,
        detail,
        status,
        counts: status_counts(
            workspace
                .chats
                .iter()
                .flat_map(|c| c.agents.iter().map(AgentStatus::agent)),
        ),
        age: workspace
            .chats
            .iter()
            .map(|c| c.last_activity)
            .max()
            .map(|at| age(at, now)),
        help: format!(
            "{} · {}\n{}",
            super::navigation::project_title(project, owner),
            workspace.name,
            workspace.path
        ),
        pinned,
        archived,
        missing: workspace.missing,
        new_chat_enabled: !workspace.missing && !workspace.removed && !archived,
        dimmed: false,
        fields: context.with_status(status).values(&owner.fields.workspace),
        selection: Some(Selection {
            workspace: workspace.path.clone(),
            chat: None,
            agent_session: None,
            agent: None,
        }),
    }];
    if state.is_collapsed(&workspace.path) {
        return result;
    }
    let mut chats: Vec<_> = workspace.chats.iter().collect();
    chats.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.row_id().cmp(&b.row_id()))
    });
    let shown = if state.expanded_lists.contains(&workspace.path) {
        chats.len()
    } else {
        chats.len().min(5)
    };
    for chat in chats.iter().take(shown) {
        let id = chat.row_id();
        let expanded = !state.is_collapsed(&id);
        let children = chat.children();
        let status = if expanded && !children.is_empty() {
            chat.chair(&chat.agents).map(AgentStatus::agent)
        } else {
            AgentStatus::aggregate(chat.agents.iter().map(AgentStatus::agent)).or(chat.status)
        };
        let waiting = children
            .iter()
            .filter(|a| AgentStatus::agent(a) == AgentStatus::Waiting)
            .count();
        let summary = (!children.is_empty()).then(|| {
            fields::count_text(children.len(), "agent")
                + &if waiting > 0 {
                    format!(" · {waiting} waiting")
                } else {
                    String::new()
                }
        });
        let mut context = fields::chat_context(
            chat,
            state,
            now,
            workspace.branch.as_deref(),
            cache.get(&workspace.path),
        );
        if let Some(text) = &summary {
            context.text.insert("children".into(), text.clone());
        }
        result.push(SidebarRow {
            id: id.clone(),
            kind: "chat",
            depth: 1,
            parent_id: Some(workspace.path.clone()),
            expanded,
            has_children: !children.is_empty(),
            children_summary: summary,
            title: chat.title.clone(),
            detail: String::new(),
            status,
            counts: chat.counts.clone(),
            age: Some(age(chat.last_activity, now)),
            help: format!("{}\n{}", chat.title, workspace.path),
            pinned,
            archived,
            missing: false,
            new_chat_enabled: false,
            dimmed: false,
            fields: context.with_status(status).values(&owner.fields.chat),
            selection: Some(Selection {
                workspace: workspace.path.clone(),
                chat: Some(chat.id.clone()),
                agent_session: None,
                agent: None,
            }),
        });
        if expanded {
            for agent in children {
                let status = AgentStatus::agent(agent);
                let name = agent["id"].as_str().unwrap_or_default();
                let role = agent["role"].as_str().unwrap_or_default();
                let mut context = fields::agent_context(agent, &format!("{name} · {role}"));
                context.add_workspace(cache.get(&workspace.path));
                if let Some(branch) = &workspace.branch {
                    context.text.insert("branch".into(), branch.clone());
                }
                result.push(SidebarRow {
                    id: chat.child_id(agent),
                    kind: "child",
                    depth: 2,
                    parent_id: Some(id.clone()),
                    expanded: false,
                    has_children: false,
                    children_summary: None,
                    title: name.into(),
                    detail: format!("· {role}"),
                    status: Some(status),
                    counts: Vec::new(),
                    age: None,
                    help: format!("{name} · {role}"),
                    pinned,
                    archived,
                    missing: false,
                    new_chat_enabled: false,
                    dimmed: matches!(status, AgentStatus::Done | AgentStatus::Ended),
                    fields: context.values(&owner.fields.chat),
                    selection: Some(Selection {
                        workspace: workspace.path.clone(),
                        chat: Some(chat.id.clone()),
                        agent_session: Some(chat.id.clone()),
                        agent: Some(name.into()),
                    }),
                });
            }
        }
    }
    if shown < chats.len() {
        let title = format!("… {} more", chats.len() - shown);
        result.push(SidebarRow {
            id: format!("more:{}", workspace.path),
            kind: "more",
            depth: 1,
            parent_id: Some(workspace.path.clone()),
            expanded: false,
            has_children: false,
            children_summary: None,
            title: title.clone(),
            detail: String::new(),
            status: None,
            counts: Vec::new(),
            age: None,
            help: "Show all chats".into(),
            pinned,
            archived,
            missing: false,
            new_chat_enabled: false,
            dimmed: false,
            fields: vec![FieldValue {
                field: "title".into(),
                text: title,
                status: None,
            }],
            selection: None,
        });
    }
    result
}
