use super::{choices::OwnerChoices, tabs::TabStrip};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Ended,
    Done,
    Working,
    Failed,
    Waiting,
}
impl AgentStatus {
    pub fn new(alive: Option<bool>, state: Option<&str>) -> Self {
        if alive == Some(false) {
            return Self::Ended;
        }
        match state {
            Some("working") => Self::Working,
            Some("waiting") => Self::Waiting,
            Some("failed") => Self::Failed,
            _ => Self::Done,
        }
    }
    pub fn agent(agent: &Value) -> Self {
        if agent["pane"].is_null() {
            Self::Ended
        } else {
            Self::new(agent["alive"].as_bool(), agent["state"].as_str())
        }
    }
    pub fn text(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::Done => "done",
            Self::Working => "working",
            Self::Failed => "failed",
            Self::Waiting => "waiting",
        }
    }
    pub fn aggregate(statuses: impl IntoIterator<Item = Self>) -> Option<Self> {
        statuses.into_iter().max()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceFact {
    pub path: String,
    pub branch: Option<String>,
    pub missing: bool,
    pub locked: bool,
    pub detached: bool,
    pub bare: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectFact {
    pub id: String,
    pub path: String,
    pub common: Option<String>,
    pub launch_directory: String,
    pub workspaces: Vec<WorkspaceFact>,
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub id: String,
    pub key: String,
    pub sessions: Vec<Value>,
    pub agents: Vec<Value>,
    pub all_chairs: Vec<Value>,
    pub last_activity: i64,
    pub title: String,
    pub provider: Option<String>,
    pub is_running: Option<bool>,
    pub status: Option<AgentStatus>,
    pub counts: Vec<StatusCount>,
}
impl Chat {
    pub fn contains(&self, id: &str) -> bool {
        self.key == id || self.sessions.iter().any(|s| s["id"] == id)
    }
    pub fn chair<'a>(&self, agents: &'a [Value]) -> Option<&'a Value> {
        let chair = self.sessions[0]["chair_id"].as_str();
        agents
            .iter()
            .find(|a| a["id"] == "orchestrator" || (chair.is_some() && a["id"].as_str() == chair))
    }
    /// The chat's child agents whose pane is alive. An ended child shows nowhere (ADR 0083).
    pub fn children(&self) -> Vec<&Value> {
        let chair = self.sessions[0]["chair_id"].as_str();
        let mut children: Vec<_> = self
            .agents
            .iter()
            .filter(|a| a["id"] != "orchestrator" && (chair.is_none() || a["id"].as_str() != chair))
            .filter(|a| a["alive"] == true && AgentStatus::agent(a) != AgentStatus::Ended)
            .collect();
        children.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        children
    }
    pub fn row_id(&self) -> String {
        format!("chat:{}", self.key)
    }
    pub fn child_id(&self, agent: &Value) -> String {
        format!(
            "child:{}/{}",
            self.id,
            agent["id"].as_str().unwrap_or_default()
        )
    }
    pub fn state_order(&self) -> u8 {
        if self.is_running == Some(false) {
            2
        } else if self.provider.is_none() {
            1
        } else {
            0
        }
    }
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
    pub missing: bool,
    pub mark: Option<String>,
    pub removed: bool,
    pub chats: Vec<Chat>,
    /// Chats the app cannot open, so no view lists them (ADR 0083). A workspace delete still
    /// closes their live agents.
    pub unlisted: Vec<Chat>,
}
impl Workspace {
    pub fn chat(&self, id: &str) -> Option<&Chat> {
        self.chats
            .iter()
            .find(|c| c.id == id)
            .or_else(|| self.chats.iter().find(|c| c.contains(id)))
    }
    pub fn seed_tabs(&self) -> TabStrip {
        TabStrip {
            open: self
                .chats
                .iter()
                .filter(|c| c.state_order() == 0)
                .map(|c| c.key.clone())
                .collect(),
            groups: Vec::new(),
        }
    }
    pub fn status(&self) -> Option<AgentStatus> {
        AgentStatus::aggregate(
            self.chats.iter().filter_map(|c| c.status).chain(
                self.chats
                    .iter()
                    .flat_map(|c| c.agents.iter().map(AgentStatus::agent)),
            ),
        )
    }
}
#[derive(Clone, Debug)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub name: String,
    pub main_path: Option<String>,
    pub launch_directory: String,
    pub workspaces: Vec<Workspace>,
}
#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub projects: Vec<Project>,
}
impl Tree {
    pub fn workspace(&self, path: &str) -> Option<(&Project, &Workspace)> {
        self.projects
            .iter()
            .find_map(|p| p.workspaces.iter().find(|w| w.path == path).map(|w| (p, w)))
    }
    pub fn chat_in(&self, workspace: &str, id: &str) -> Option<&Chat> {
        self.workspace(workspace).and_then(|(_, w)| w.chat(id))
    }
    pub fn chats(&self) -> impl Iterator<Item = &Chat> {
        self.projects
            .iter()
            .flat_map(|p| p.workspaces.iter().flat_map(|w| &w.chats))
    }
    pub fn project_containing(&self, path: &str) -> Option<&Project> {
        self.projects
            .iter()
            .flat_map(|p| p.workspaces.iter().map(move |w| (p, w)))
            .filter(|(_, w)| contains(path, &w.path))
            .max_by_key(|(_, w)| w.path.len())
            .map(|(p, _)| p)
            .or_else(|| self.projects.iter().find(|p| p.path == path))
    }
}

pub fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
pub fn contains(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|tail| tail.starts_with('/'))
}
pub fn nonblank(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
pub fn age(timestamp: i64, now: i64) -> String {
    let seconds = now.saturating_sub(timestamp).max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

pub fn build(snapshot: &Value, owner: &OwnerChoices) -> Tree {
    let sessions: Vec<Value> = snapshot["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["archivedAt"].is_null() && s["archived_at_s"].is_null())
        .cloned()
        .collect();
    let mut facts: Vec<ProjectFact> =
        serde_json::from_value(snapshot["projects"].clone()).unwrap_or_default();
    // Fixtures and older snapshots can provide just the project/workspace facts on sessions.
    if facts.is_empty() {
        for session in &sessions {
            let cwd = session["cwd"].as_str().unwrap_or_default();
            let path = session["project"].as_str().unwrap_or(cwd);
            let common = session["common"].as_str().map(str::to_owned);
            let id = common
                .as_ref()
                .map_or_else(|| format!("folder:{path}"), |c| format!("repository:{c}"));
            let index = facts.iter().position(|p| p.id == id).unwrap_or_else(|| {
                facts.push(ProjectFact {
                    id,
                    path: path.into(),
                    launch_directory: path.into(),
                    common,
                    workspaces: Vec::new(),
                });
                facts.len() - 1
            });
            let path = session["workspace"].as_str().unwrap_or(cwd);
            if !facts[index].workspaces.iter().any(|w| w.path == path) {
                facts[index].workspaces.push(WorkspaceFact {
                    path: path.into(),
                    branch: session["branch"].as_str().map(str::to_owned),
                    ..Default::default()
                });
            }
        }
        for path in &owner.project_paths {
            if !facts
                .iter()
                .any(|p| p.path == *path || p.workspaces.iter().any(|w| w.path == *path))
            {
                facts.push(ProjectFact {
                    id: format!("folder:{path}"),
                    path: path.clone(),
                    launch_directory: path.clone(),
                    common: None,
                    workspaces: vec![WorkspaceFact {
                        path: path.clone(),
                        ..Default::default()
                    }],
                });
            }
        }
    }
    let mut projects = Vec::new();
    for fact in facts {
        if owner.removed_projects.contains(&fact.path) {
            continue;
        }
        let project_sessions: Vec<_> = sessions
            .iter()
            .filter(|s| {
                s["project_id"] == fact.id
                    || (s["project_id"].is_null()
                        && s["project"]
                            .as_str()
                            .unwrap_or(s["cwd"].as_str().unwrap_or_default())
                            == fact.path)
            })
            .cloned()
            .collect();
        let opened = owner
            .project_paths
            .iter()
            .any(|p| *p == fact.path || fact.workspaces.iter().any(|w| w.path == *p));
        let mut workspaces = Vec::new();
        let mut matched = BTreeSet::new();
        for ws in fact.workspaces.iter().filter(|w| !w.bare) {
            let matching: Vec<_> = project_sessions
                .iter()
                .filter(|s| {
                    if s["workspace"].is_string() {
                        s["workspace"] == ws.path
                    } else {
                        fact.workspaces
                            .iter()
                            .filter(|w| {
                                !w.bare && contains(s["cwd"].as_str().unwrap_or_default(), &w.path)
                            })
                            .max_by_key(|w| w.path.len())
                            .is_some_and(|w| w.path == ws.path)
                    }
                })
                .cloned()
                .collect();
            matched.extend(
                matching
                    .iter()
                    .filter_map(|s| s["id"].as_str())
                    .map(str::to_owned),
            );
            if matching.is_empty() && !ws.missing && !opened {
                continue;
            }
            let (listed, unlisted) = chats(matching, snapshot, owner);
            workspaces.push(Workspace {
                path: ws.path.clone(),
                name: ws.branch.clone().unwrap_or_else(|| basename(&ws.path)),
                branch: ws.branch.clone(),
                missing: ws.missing,
                mark: if ws.locked {
                    Some("locked".into())
                } else if ws.detached {
                    Some("detached".into())
                } else {
                    None
                },
                removed: false,
                chats: listed,
                unlisted,
            });
        }
        let unmatched: Vec<_> = project_sessions
            .into_iter()
            .filter(|s| !matched.contains(s["id"].as_str().unwrap_or_default()))
            .collect();
        let (removed, hub): (Vec<_>, Vec<_>) = unmatched.into_iter().partition(|s| {
            s["folder_missing"] == true && s["cwd"] != fact.path && s["cwd"] != json!(fact.common)
        });
        for (removed, sessions) in [(false, hub), (true, removed)] {
            if !sessions.is_empty() {
                let (listed, unlisted) = chats(sessions, snapshot, owner);
                workspaces.push(Workspace {
                    path: if removed {
                        format!("{}#removed", fact.path)
                    } else {
                        fact.path.clone()
                    },
                    name: if removed {
                        "Removed worktrees".into()
                    } else {
                        basename(&fact.path)
                    },
                    branch: None,
                    missing: false,
                    mark: None,
                    removed,
                    chats: listed,
                    unlisted,
                });
            }
        }
        if workspaces.is_empty() && !opened {
            continue;
        }
        if !opened && workspaces.iter().all(|w| w.chats.is_empty()) {
            continue;
        }
        let main_path = if fact
            .common
            .as_ref()
            .is_some_and(|c| *c != format!("{}/.git", fact.path))
        {
            workspaces
                .iter()
                .find(|w| w.branch.as_deref() == Some("main"))
                .or_else(|| {
                    workspaces
                        .iter()
                        .find(|w| w.branch.as_deref() == Some("master"))
                })
                .map(|w| w.path.clone())
        } else {
            Some(fact.path.clone())
        };
        order_workspaces(
            &mut workspaces,
            owner
                .workspace_order
                .get(&fact.path)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            main_path.as_deref(),
            &fact.path,
        );
        projects.push(Project {
            id: fact.id,
            name: basename(&fact.path),
            path: fact.path,
            main_path,
            launch_directory: fact.launch_directory,
            workspaces,
        });
    }
    projects.sort_by(|a, b| natural_cmp(&a.path, &b.path));
    Tree { projects }
}

/// The chats the app can open, then the ones it cannot.
fn chats(sessions: Vec<Value>, snapshot: &Value, owner: &OwnerChoices) -> (Vec<Chat>, Vec<Chat>) {
    let by_id: BTreeMap<_, _> = sessions
        .iter()
        .filter_map(|s| s["id"].as_str().map(|id| (id, s)))
        .collect();
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for session in &sessions {
        let mut root = session;
        let mut seen = BTreeSet::new();
        while let Some(parent) = root["continuation_of"]
            .as_str()
            .and_then(|id| by_id.get(id))
        {
            if !seen.insert(root["id"].as_str()) {
                break;
            }
            root = parent;
        }
        let group = if let (Some(provider), Some(id)) =
            (root["chair_provider"].as_str(), root["chair_id"].as_str())
        {
            format!("chair:{provider}:{id}")
        } else if let Some(log) = root["chair_log"].as_str() {
            format!("log:{log}")
        } else {
            format!("session:{}", root["id"].as_str().unwrap_or_default())
        };
        groups.entry(group).or_default().push(session.clone());
    }
    let mut result = Vec::new();
    let mut unlisted = Vec::new();
    for mut sessions in groups.into_values() {
        sessions.sort_by(|a, b| {
            b["created_at"]
                .as_i64()
                .cmp(&a["created_at"].as_i64())
                .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
        });
        let current = &sessions[0];
        let id = current["id"].as_str().unwrap_or_default().to_owned();
        let oldest = sessions
            .iter()
            .min_by(|a, b| {
                a["created_at"]
                    .as_i64()
                    .cmp(&b["created_at"].as_i64())
                    .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
            })
            .unwrap();
        let key = oldest["id"].as_str().unwrap_or_default().to_owned();
        let known = sessions
            .iter()
            .all(|s| snapshot["agents"][s["id"].as_str().unwrap_or_default()].is_array());
        let all: Vec<Value> = sessions
            .iter()
            .flat_map(|s| {
                snapshot["agents"][s["id"].as_str().unwrap_or_default()]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect();
        if (known && all.is_empty())
            || (!known
                && sessions
                    .iter()
                    .map(|s| s["agents"].as_u64().unwrap_or_default())
                    .sum::<u64>()
                    == 0)
        {
            continue;
        }
        let agents = snapshot["agents"][&id]
            .as_array()
            .cloned()
            .unwrap_or_default();
        // The rows route reads a chair log only in these formats, and a closed chat with no log
        // has nothing to show (ADR 0083).
        let readable = matches!(
            current["chair_provider"].as_str(),
            Some("claude" | "codex" | "agy")
        );
        let closed_without_log = known
            && nonblank(current["chair_log"].as_str()).is_none()
            && agents
                .iter()
                .all(|a| AgentStatus::agent(a) == AgentStatus::Ended);
        let provider = nonblank(current["chair_provider"].as_str());
        let title = nonblank(owner.chat_names.get(&key).map(String::as_str))
            .or_else(|| {
                sessions
                    .iter()
                    .find_map(|s| nonblank(s["cli_name"].as_str()))
            })
            .or_else(|| {
                sessions
                    .iter()
                    .rev()
                    .find_map(|s| nonblank(s["title"].as_str()))
            })
            .unwrap_or_else(|| {
                format!(
                    "{} {}",
                    provider.as_deref().unwrap_or("Chat"),
                    id.chars().take(8).collect::<String>()
                )
            });
        let last_activity = sessions
            .iter()
            .map(|s| {
                s["last_message_at"]
                    .as_i64()
                    .unwrap_or(s["created_at"].as_i64().unwrap_or_default())
                    .max(s["chair_log_activity"].as_i64().unwrap_or_default())
            })
            .max()
            .unwrap_or_default();
        let is_running = known.then(|| agents.iter().any(|a| a["alive"] == true));
        let status = snapshot["agents"][&id]
            .is_array()
            .then(|| AgentStatus::aggregate(agents.iter().map(AgentStatus::agent)))
            .flatten();
        let counts = status_counts(agents.iter().map(AgentStatus::agent));
        let all_chairs = sessions
            .iter()
            .filter_map(|s| {
                snapshot["agents"][s["id"].as_str().unwrap_or_default()]
                    .as_array()
                    .and_then(|agents| {
                        agents.iter().find(|a| {
                            a["id"] == "orchestrator"
                                || (s["chair_id"].is_string() && a["id"] == s["chair_id"])
                        })
                    })
            })
            .cloned()
            .collect();
        let chat = Chat {
            id,
            key,
            sessions,
            agents,
            all_chairs,
            title,
            last_activity,
            provider,
            is_running,
            status,
            counts,
        };
        if readable && !closed_without_log {
            result.push(chat);
        } else {
            unlisted.push(chat);
        }
    }
    result.sort_by(|a, b| {
        a.state_order()
            .cmp(&b.state_order())
            .then_with(|| b.last_activity.cmp(&a.last_activity))
            .then_with(|| a.key.cmp(&b.key))
    });
    (result, unlisted)
}

pub fn order_workspaces(
    workspaces: &mut [Workspace],
    order: &[String],
    main: Option<&str>,
    hub: &str,
) {
    let group = |w: &Workspace| {
        if w.removed {
            2
        } else if w.path == hub && Some(w.path.as_str()) != main {
            1
        } else {
            0
        }
    };
    workspaces.sort_by(|a, b| {
        order
            .iter()
            .position(|p| p == &a.path)
            .unwrap_or(usize::MAX)
            .cmp(
                &order
                    .iter()
                    .position(|p| p == &b.path)
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| group(a).cmp(&group(b)))
            .then_with(|| (Some(b.path.as_str()) == main).cmp(&(Some(a.path.as_str()) == main)))
            .then_with(|| natural_cmp(&a.name, &b.name))
            .then_with(|| a.path.cmp(&b.path))
    });
}

pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut left = String::new();
                let mut right = String::new();
                while a.peek().is_some_and(char::is_ascii_digit) {
                    left.push(a.next().unwrap());
                }
                while b.peek().is_some_and(char::is_ascii_digit) {
                    right.push(b.next().unwrap());
                }
                let left = left.trim_start_matches('0');
                let right = right.trim_start_matches('0');
                let order = left.len().cmp(&right.len()).then_with(|| left.cmp(right));
                if !order.is_eq() {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                a.next();
                b.next();
                let order = x.cmp(&y);
                if !order.is_eq() {
                    return order;
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatusCount {
    pub status: AgentStatus,
    pub count: usize,
}
/// Counts only live agents, so a closed chat shows no badge (ADR 0083).
pub fn status_counts(statuses: impl IntoIterator<Item = AgentStatus>) -> Vec<StatusCount> {
    let mut counts = BTreeMap::new();
    for status in statuses.into_iter().filter(|s| *s != AgentStatus::Ended) {
        *counts.entry(status).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .rev()
        .map(|(status, count)| StatusCount { status, count })
        .collect()
}

pub use super::sidebar::{Section, Selection, SidebarRow, sections};
