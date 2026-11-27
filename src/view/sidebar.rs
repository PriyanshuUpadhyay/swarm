use super::{
    choices::{OwnerChoices, ViewState},
    fields::{self, FieldValue, WorkspaceFields},
    navigation::{archived_fold, is_folder, workspace_heading},
    tree::*,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

mod lists;

pub(super) const NEEDS_YOU: &str = "needs-you";
pub(super) const PINNED: &str = "pinned";
pub(super) const MORE_PROJECTS: &str = "more-projects";

const CHAT_LIMIT: usize = 5;
const RECENT_SECONDS: i64 = 7 * 24 * 60 * 60;

type Item<'a> = (&'a Workspace, &'a Chat);

struct Projection<'a> {
    owner: &'a OwnerChoices,
    state: &'a ViewState,
    now: i64,
    cache: &'a BTreeMap<String, WorkspaceFields>,
}

pub fn sections(
    tree: &Tree,
    owner: &OwnerChoices,
    state: &ViewState,
    archive: bool,
    now: i64,
    cache: &BTreeMap<String, WorkspaceFields>,
) -> Vec<Section> {
    let view = Projection {
        owner,
        state,
        now,
        cache,
    };
    let mut sections = Vec::new();
    let promoted = if archive {
        BTreeSet::new()
    } else {
        lists::append(&view, tree, &mut sections)
    };

    let mut names = BTreeMap::new();
    for project in &tree.projects {
        *names
            .entry(super::navigation::project_title(project, owner))
            .or_insert(0) += 1;
    }
    let mut inactive = Vec::new();
    for project in &tree.projects {
        let section = view.project(project, &promoted, archive, &names);
        if archive {
            if !section.rows.is_empty() {
                sections.push(section);
            }
        } else if project
            .workspaces
            .iter()
            .any(|w| !owner.archived.contains(&w.path) && w.chats.iter().any(|c| view.active(w, c)))
        {
            sections.push(section);
        } else if project
            .workspaces
            .iter()
            .any(|w| !owner.archived.contains(&w.path))
        {
            inactive.push(section);
        }
    }
    if !inactive.is_empty() {
        let count = inactive.len();
        let expanded = !state.is_collapsed(MORE_PROJECTS);
        let title = format!("{count} more project{}", if count == 1 { "" } else { "s" });
        sections.push(Section {
            count: Some(count),
            ..view.group(MORE_PROJECTS, title, Vec::new(), expanded)
        });
        if expanded {
            for mut section in inactive {
                section.parent_id = Some(MORE_PROJECTS.into());
                sections.push(section);
            }
        }
    }
    sections
}

pub fn chat_catalog(
    tree: &Tree,
    owner: &OwnerChoices,
    state: &ViewState,
    now: i64,
    cache: &BTreeMap<String, WorkspaceFields>,
) -> Vec<SidebarRow> {
    let view = Projection {
        owner,
        state,
        now,
        cache,
    };
    view.live_workspaces(tree)
        .flat_map(|w| {
            w.chats
                .iter()
                .map(|chat| view.chat_row(w, chat, None, false))
        })
        .collect()
}

fn sort_chats(chats: &mut [Item<'_>]) {
    chats.sort_by(|(_, a), (_, b)| {
        b.last_activity
            .cmp(&a.last_activity)
            .then(a.key.cmp(&b.key))
    });
}

fn prompt(chat: &Chat) -> Option<&str> {
    chat.agents.iter().find_map(|a| {
        if AgentStatus::agent(a) != AgentStatus::Blocked || a["prompt"]["id"].as_str()?.is_empty() {
            return None;
        }
        a["prompt"]["question"]
            .as_str()
            .filter(|text| !text.trim().is_empty())
    })
}

fn urgent(chat: &Chat) -> bool {
    matches!(
        chat.status,
        Some(AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error)
    )
}

fn more(group: &str, count: usize) -> SidebarRow {
    let title = format!("… {count} more");
    SidebarRow {
        id: format!("more:{group}"),
        kind: "more",
        title: title.clone(),
        help: "Show all chats".into(),
        fields: vec![FieldValue {
            field: "title".into(),
            text: title,
            status: None,
        }],
        ..Default::default()
    }
}

impl<'a> Projection<'a> {
    fn live_workspaces(&self, tree: &'a Tree) -> impl Iterator<Item = &'a Workspace> {
        tree.projects
            .iter()
            .flat_map(|p| &p.workspaces)
            .filter(|w| !self.owner.archived.contains(&w.path))
    }

    fn active(&self, w: &Workspace, chat: &Chat) -> bool {
        urgent(chat)
            || self.state.is_unread(&chat.key, chat.last_activity)
            || self.owner.pinned.contains(&w.path)
            || self.now.saturating_sub(chat.last_activity) <= RECENT_SECONDS
    }

    fn group(
        &self,
        id: &'static str,
        title: String,
        rows: Vec<SidebarRow>,
        expanded: bool,
    ) -> Section {
        Section {
            id: id.into(),
            kind: id,
            path: None,
            folder: false,
            collapse_id: id.into(),
            expanded,
            title,
            status: AgentStatus::aggregate(rows.iter().filter_map(|r| r.status)),
            fields: Vec::new(),
            rows,
            parent_id: None,
            count: None,
        }
    }

    fn project(
        &self,
        p: &'a Project,
        promoted: &BTreeSet<&str>,
        archive: bool,
        names: &BTreeMap<String, usize>,
    ) -> Section {
        let collapse_id = format!("project:{}", p.path);
        let mut rows = Vec::new();
        let mut folder = false;

        if archive {
            let mut archived = Vec::new();
            for w in &p.workspaces {
                if self.owner.archived.contains(&w.path) {
                    let mut chats: Vec<_> = w.chats.iter().map(|c| (w, c)).collect();
                    sort_chats(&mut chats);
                    rows.push(self.workspace_row(p, w, &chats));
                    for (_, chat) in chats {
                        self.chat_rows(w, chat, Some(w.path.as_str()), &mut rows);
                    }
                }
                let name =
                    (p.workspaces.len() > 1).then(|| workspace_heading(p, w, self.owner, true));
                archived.extend(w.archived.iter().map(|chat| (chat, name.clone())));
            }

            archived.sort_by_key(|(chat, _)| Reverse(chat.archived_at));
            rows.extend(archived_fold(
                format!("archived:project:{}", p.path),
                &archived,
                self.state,
                self.now,
            ));
        } else {
            let mut all: Vec<_> = p
                .workspaces
                .iter()
                .filter(|w| {
                    !self.owner.archived.contains(&w.path) && !self.owner.pinned.contains(&w.path)
                })
                .flat_map(|w| {
                    w.chats
                        .iter()
                        .filter(|c| !promoted.contains(c.id.as_str()))
                        .map(move |chat| (w, chat))
                })
                .collect();
            sort_chats(&mut all);
            let active: Vec<_> = all
                .iter()
                .copied()
                .filter(|(w, c)| self.active(w, c))
                .collect();

            let open = self.state.expanded_lists.contains(&collapse_id);
            let shown: Vec<_> = if open {
                all.clone()
            } else {
                active
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(i, entry)| (i < CHAT_LIMIT || urgent(entry.1)).then_some(entry))
                    .collect()
            };

            self.grouped_rows(p, &shown, &active, &mut rows);
            folder = is_folder(p) && rows.first().is_some_and(|r| r.kind == "workspace");
            if all.len() > shown.len() {
                let mut fold = more(&collapse_id, all.len() - shown.len());
                if folder {
                    fold.parent_id = Some(p.workspaces[0].path.clone());
                    fold.depth = 1;
                }
                rows.push(fold);
            }
        }

        let name = super::navigation::project_title(p, self.owner);
        let parent = Path::new(&p.path)
            .parent()
            .map(|p| basename(&p.to_string_lossy()))
            .unwrap_or_default();
        let title = if names[&name] > 1 && !parent.is_empty() {
            format!("{name} — {parent}")
        } else {
            name
        };
        let status = AgentStatus::aggregate(rows.iter().filter_map(|r| r.status));
        let context =
            fields::workspace_context(&p.workspaces, &title, self.state, self.now, self.cache);
        Section {
            id: format!("project:{}", p.id),
            kind: "project",
            path: Some(p.path.clone()),
            folder,
            expanded: !self.state.is_collapsed(&collapse_id),
            collapse_id,
            title,
            status,
            fields: context
                .with_status(status)
                .values(&self.owner.fields.project),
            rows,
            parent_id: None,
            count: None,
        }
    }

    fn grouped_rows(
        &self,
        project: &Project,
        shown: &[Item<'_>],
        active: &[Item<'_>],
        out: &mut Vec<SidebarRow>,
    ) {
        let mut groups: BTreeMap<&str, Vec<_>> = BTreeMap::new();
        for &(w, c) in active {
            groups.entry(&w.path).or_default().push((w, c));
        }

        let mut shown_groups: BTreeMap<&str, Vec<_>> = BTreeMap::new();
        for &(w, chat) in shown {
            if groups
                .get(w.path.as_str())
                .is_some_and(|chats| chats.len() >= 2)
            {
                shown_groups.entry(&w.path).or_default().push(chat);
            }
        }

        for &(w, chat) in shown {
            let Some(group) = groups.get(w.path.as_str()).filter(|group| group.len() >= 2) else {
                self.chat_rows(w, chat, None, out);
                continue;
            };
            if let Some(shown) = shown_groups.remove(w.path.as_str()) {
                let row = self.workspace_row(project, w, group);
                let expanded = row.expanded;
                out.push(row);
                if expanded {
                    for chat in shown {
                        self.chat_rows(w, chat, Some(w.path.as_str()), out);
                    }
                }
            }
        }
    }

    fn workspace_row(&self, p: &Project, w: &Workspace, chats: &[Item<'_>]) -> SidebarRow {
        let owner = self.owner;
        let pinned = owner.pinned.contains(&w.path);
        let archived = owner.archived.contains(&w.path);
        let in_project = !pinned || archived;

        let title = workspace_heading(p, w, owner, in_project);
        let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for item in &p.workspaces {
            ids.entry(super::navigation::title_key(&workspace_heading(
                p, item, owner, in_project,
            )))
            .or_default()
            .push(item.path.clone());
        }
        let mut detail = super::navigation::workspace_detail(p, w, owner, in_project, &ids);
        if w.missing {
            detail.push_str(" · folder missing");
        }
        if let Some(mark) = &w.mark {
            detail.push_str(&format!(" · {mark}"));
        }

        let status = AgentStatus::aggregate(chats.iter().filter_map(|(_, c)| c.status));
        let mut context = fields::workspace_context(
            std::slice::from_ref(w),
            &title,
            self.state,
            self.now,
            self.cache,
        );
        if owner.fields.workspace.iter().any(|f| f == "title") {
            let suffix = fields::count_text(w.chats.len(), "chat");
            let branch = detail
                .split(" · ")
                .filter(|p| *p != suffix)
                .collect::<Vec<_>>()
                .join(" · ");
            context.text.insert("branch".into(), branch);
        }

        SidebarRow {
            id: w.path.clone(),
            kind: "workspace",
            expanded: !self.state.is_collapsed(&w.path),
            has_children: !chats.is_empty(),
            title,
            detail,
            status,
            counts: status_counts(
                chats
                    .iter()
                    .flat_map(|(_, c)| c.agents.iter().map(AgentStatus::agent)),
            ),
            age: chats.first().map(|(_, c)| age(c.last_activity, self.now)),
            help: format!(
                "{} · {}\n{}",
                super::navigation::project_title(p, owner),
                w.name,
                w.path
            ),
            pinned,
            archived,
            missing: w.missing,
            new_chat_enabled: w.can_start_chat(archived),
            starts_chat_worktree: w.hub_main,
            fields: context.with_status(status).values(&owner.fields.workspace),
            selection: Some(Selection {
                workspace: w.path.clone(),
                chat: None,
                agent_session: None,
                agent: None,
            }),
            ..Default::default()
        }
    }

    fn chat_row(
        &self,
        w: &Workspace,
        c: &Chat,
        parent: Option<&str>,
        show_prompt: bool,
    ) -> SidebarRow {
        let id = c.row_id();
        let children = c.children();
        let waiting = children
            .iter()
            .filter(|a| AgentStatus::agent(a) == AgentStatus::Blocked)
            .count();
        let summary = (!children.is_empty()).then(|| {
            let count = children.len();
            if waiting > 0 {
                format!("{count} · {waiting} waiting")
            } else {
                count.to_string()
            }
        });

        let mut context = fields::chat_context(
            c,
            self.state,
            self.now,
            w.branch.as_deref(),
            self.cache.get(&w.path),
        );
        if let Some(text) = &summary {
            context.text.insert("children".into(), text.clone());
        }
        let branch_parent = c
            .sessions
            .iter()
            .find_map(|s| s["branch_of"].as_str())
            .and_then(|parent| w.chats.iter().find(|c| c.contains(parent)));
        let detail = if let Some(question) = prompt(c) {
            context.text.remove("question");
            if show_prompt {
                shorten(&one_line(question), 80)
            } else {
                String::new()
            }
        } else {
            c.agents
                .iter()
                .find_map(|a| a["state_detail"].as_str())
                .map(one_line)
                .unwrap_or_default()
        };
        let mut fields = context
            .with_status(c.status)
            .values(&self.owner.fields.chat);
        if w.chats.len() == 1 {
            for mark in [w.missing.then_some("folder missing"), w.mark.as_deref()]
                .into_iter()
                .flatten()
            {
                fields.push(FieldValue {
                    field: "mark".into(),
                    text: mark.into(),
                    status: None,
                });
            }
        }

        SidebarRow {
            expanded: !self.state.is_collapsed(&id),
            id,
            kind: "chat",
            depth: u8::from(parent.is_some()),
            parent_id: parent.map(str::to_owned),
            has_children: !children.is_empty(),
            children_summary: summary,
            title: c.title.clone(),
            detail,
            status: c.status,
            counts: c.counts.clone(),
            age: Some(age(c.last_activity, self.now)),
            help: branch_parent.map_or_else(
                || format!("{}\n{}", c.title, w.path),
                |parent| format!("{}\nBranched from {}\n{}", c.title, parent.title, w.path),
            ),
            pinned: self.owner.pinned.contains(&w.path),
            archived: self.owner.archived.contains(&w.path),
            missing: w.missing,
            fields,
            selection: Some(Selection {
                workspace: w.path.clone(),
                chat: Some(c.id.clone()),
                agent_session: None,
                agent: None,
            }),
            ..Default::default()
        }
    }

    fn chat_rows(&self, w: &Workspace, c: &Chat, parent: Option<&str>, out: &mut Vec<SidebarRow>) {
        let row = self.chat_row(w, c, parent, true);
        let expanded = row.expanded;
        let depth = row.depth + 1;
        let id = row.id.clone();
        out.push(row);
        if !expanded {
            return;
        }

        for agent in c.children() {
            let status = AgentStatus::agent(agent);
            let name = agent["id"].as_str().unwrap_or_default();
            let role = agent["role"].as_str().unwrap_or_default();
            let mut context = fields::agent_context(agent, &format!("{name} · {role}"));
            context.add_workspace(self.cache.get(&w.path));
            if let Some(branch) = &w.branch {
                context.text.insert("branch".into(), branch.clone());
            }

            out.push(SidebarRow {
                id: c.child_id(agent),
                kind: "child",
                depth,
                parent_id: Some(id.clone()),
                title: name.into(),
                detail: format!("· {role}"),
                status: Some(status),
                help: format!("{name} · {role}"),
                pinned: self.owner.pinned.contains(&w.path),
                archived: self.owner.archived.contains(&w.path),
                dimmed: matches!(status, AgentStatus::Done | AgentStatus::Ended),
                fields: context.values(&self.owner.fields.chat),
                selection: Some(Selection {
                    workspace: w.path.clone(),
                    chat: Some(c.id.clone()),
                    agent_session: Some(c.id.clone()),
                    agent: Some(name.into()),
                }),
                ..Default::default()
            });
        }
    }
}
