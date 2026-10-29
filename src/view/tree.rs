mod ordering;
mod projects;
pub use ordering::{StatusCount, natural_cmp, order_workspaces, status_counts};
pub use projects::build;

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
    /// An unlisted chat that the owner can import: it has a log or a live chair to show.
    pub importable: bool,
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
    /// The archived chats whose folder is this workspace, newest first.
    pub archived: Vec<ArchivedChat>,
}

/// An archived session, from the snapshot's `archived` list.
#[derive(Clone, Debug)]
pub struct ArchivedChat {
    pub id: String,
    pub title: String,
    pub archived_at: i64,
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
    /// Each chat the owner can import, with its workspace. A project whose chats are all
    /// unlisted is not in `projects`, so these are kept apart.
    pub importable: Vec<(String, Chat)>,
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
    /// The deepest workspace whose folder contains `path`, with its project.
    pub fn workspace_containing(&self, path: &str) -> Option<(&Project, &Workspace)> {
        self.projects
            .iter()
            .flat_map(|p| p.workspaces.iter().map(move |w| (p, w)))
            .filter(|(_, w)| contains(path, &w.path))
            .max_by_key(|(_, w)| w.path.len())
    }
    pub fn project_containing(&self, path: &str) -> Option<&Project> {
        self.workspace_containing(path)
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
/// A name on one line: a line break, U+2028 too, breaks a sidebar row or a tab in WebKit.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn nonblank(text: Option<&str>) -> Option<String> {
    text.map(one_line).filter(|s| !s.is_empty())
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

/// The chat id that the page header shows (`shortId` in `web/src/core/route.ts`): the shortest
/// start of `id`, at least 8 characters, that no other live or archived session shares. A UUID v7
/// starts with the time, so chats made in the same minute share their first 8 characters.
pub fn short_id(id: &str, snapshot: &Value) -> String {
    let others: Vec<&str> = ["sessions", "archived"]
        .iter()
        .filter_map(|list| snapshot[*list].as_array())
        .flatten()
        .filter_map(|s| s["id"].as_str())
        .filter(|other| *other != id)
        .collect();
    let mut length = 8.min(id.len());
    while length < id.len() && others.iter().any(|o| o.starts_with(&id[..length])) {
        length += 1;
    }
    id[..length].to_owned()
}

/// The snapshot's archived sessions in `workspace`, newest first. A chat with no name or title
/// shows its provider and short id, as a live one does.
pub fn archived_chats(
    snapshot: &Value,
    workspace: &str,
    owner: &OwnerChoices,
) -> Vec<ArchivedChat> {
    let all: Vec<&Value> = ["sessions", "archived"]
        .iter()
        .flat_map(|k| snapshot[*k].as_array().into_iter().flatten())
        .collect();
    // A rename is kept under the chat's first session, as for a live chat (`key`).
    let key = |id: &str| {
        let mut key = id.to_owned();
        let mut seen = BTreeSet::new();
        while seen.insert(key.clone()) {
            match all
                .iter()
                .find(|s| s["id"] == key.as_str())
                .and_then(|s| s["continuation_of"].as_str())
            {
                Some(parent) if all.iter().any(|s| s["id"] == parent) => key = parent.to_owned(),
                _ => break,
            }
        }
        key
    };
    let mut chats: Vec<_> = snapshot["archived"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["workspace"] == workspace)
        .filter_map(|s| {
            let id = s["id"].as_str()?.to_owned();
            let title = nonblank(owner.chat_names.get(&key(&id)).map(String::as_str))
                .or_else(|| nonblank(s["title"].as_str()))
                .unwrap_or_else(|| {
                    format!(
                        "{} {}",
                        s["chair_provider"].as_str().unwrap_or("Chat"),
                        short_id(&id, snapshot)
                    )
                });
            Some(ArchivedChat {
                title,
                archived_at: s["archivedAt"].as_i64().unwrap_or_default(),
                id,
            })
        })
        .collect();
    chats.sort_by(|a, b| {
        b.archived_at
            .cmp(&a.archived_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    chats
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
        // The app lists only the chats it started, which run on its own adapter (ADR 0084).
        let started_by_app = current["adapter"] == "tmux-swarm";
        // The rows route reads a chair log only in these formats, and a closed chat with no log
        // has nothing to show (ADR 0083).
        let readable = current["chair_provider"]
            .as_str()
            .is_some_and(|format| super::transcript::LOG_FORMATS.contains(&format));
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
                    short_id(&id, snapshot)
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
        let openable = readable && !closed_without_log;
        let listed = openable && (started_by_app || owner.imported.contains(&key));
        let chat = Chat {
            importable: openable && !listed,
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
        if listed {
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

pub use super::sidebar::{Section, Selection, SidebarRow, sections};

#[cfg(test)]
mod tests {
    #[test]
    fn a_name_is_one_line() {
        assert_eq!(
            super::nonblank(Some(" Fix\u{2028}the\n\tsidebar ")).as_deref(),
            Some("Fix the sidebar")
        );
        assert_eq!(super::nonblank(Some("\u{2028}\n")), None);
    }
}
