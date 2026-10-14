use super::{
    choices::ViewState,
    tree::{AgentStatus, Chat, Workspace, age},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const FIELDS: &[&str] = &[
    "status", "title", "provider", "branch", "age", "children", "question", "unread", "dirty",
    "model", "effort", "steps", "pr", "ci", "tokens", "cost",
];
pub fn known_field(field: &str) -> bool {
    FIELDS.contains(&field)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RowFieldLists {
    pub project: Vec<String>,
    pub workspace: Vec<String>,
    pub chat: Vec<String>,
    pub tab: Vec<String>,
}
impl Default for RowFieldLists {
    fn default() -> Self {
        let list = |fields: &[&str]| fields.iter().map(|s| (*s).into()).collect();
        Self {
            project: list(&["title", "status"]),
            workspace: list(&["status", "title", "branch", "children", "age", "steps"]),
            chat: list(&[
                "status", "title", "age", "steps", "children", "unread", "tokens",
            ]),
            tab: list(&["status", "title", "provider", "unread", "tokens"]),
        }
    }
}
impl<'de> Deserialize<'de> for RowFieldLists {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Saved {
            project: Option<Vec<String>>,
            workspace: Option<Vec<String>>,
            chat: Option<Vec<String>>,
            tab: Option<Vec<String>>,
        }
        let saved = Saved::deserialize(decoder)?;
        let bundled = Self::default();
        let list = |fields: Option<Vec<String>>, default| {
            fields
                .map(|f| f.into_iter().filter(|s| known_field(s)).collect())
                .unwrap_or(default)
        };
        Ok(Self {
            project: list(saved.project, bundled.project),
            workspace: list(saved.workspace, bundled.workspace),
            chat: list(saved.chat, bundled.chat),
            tab: list(saved.tab, bundled.tab),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFields {
    pub dirty_count: Option<usize>,
    pub pr: Option<String>,
    pub ci: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FieldValue {
    pub field: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<AgentStatus>,
}
#[derive(Default)]
pub struct Context {
    pub text: BTreeMap<String, String>,
    pub status: Option<AgentStatus>,
    pub unread: bool,
}
impl Context {
    pub fn with_status(mut self, status: Option<AgentStatus>) -> Self {
        self.status = status;
        self
    }
    pub fn values(&self, fields: &[String]) -> Vec<FieldValue> {
        fields
            .iter()
            .filter_map(|field| {
                let text = match field.as_str() {
                    "status" => self.status.map(|s| s.text().into()),
                    "unread" => self.unread.then(|| "Unread".into()),
                    _ => self.text.get(field).filter(|s| !s.is_empty()).cloned(),
                }?;
                Some(FieldValue {
                    field: field.clone(),
                    text,
                    status: if field == "status" { self.status } else { None },
                })
            })
            .collect()
    }
    pub fn add_workspace(&mut self, fields: Option<&WorkspaceFields>) {
        if let Some(fields) = fields {
            if let Some(dirty) = fields.dirty_count {
                self.text.insert("dirty".into(), format!("{dirty} dirty"));
            }
            if let Some(pr) = &fields.pr {
                self.text.insert("pr".into(), pr.clone());
            }
            if let Some(ci) = &fields.ci {
                self.text.insert("ci".into(), ci.clone());
            }
        }
    }
}
pub fn count_text(count: usize, unit: &str) -> String {
    format!("{count} {unit}{}", if count == 1 { "" } else { "s" })
}
fn add_usage(context: &mut Context, agents: impl IntoIterator<Item = Value>) {
    let agents: Vec<_> = agents.into_iter().collect();
    let tokens: Vec<_> = agents.iter().filter_map(|a| a["tokens"].as_i64()).collect();
    if !tokens.is_empty() {
        context.text.insert(
            "tokens".into(),
            format!(
                "{} tokens",
                tokens.into_iter().fold(0i64, i64::saturating_add)
            ),
        );
    }
    let costs: Vec<_> = agents
        .iter()
        .filter_map(|a| a["costUsd"].as_f64().or_else(|| a["cost_usd"].as_f64()))
        .collect();
    if !costs.is_empty() {
        context.text.insert(
            "cost".into(),
            format!("${:.2}", costs.into_iter().sum::<f64>()),
        );
    }
}
fn question(agent: &Value) -> Option<&str> {
    agent["prompt"]["question"].as_str().or_else(|| {
        if AgentStatus::agent(agent) == AgentStatus::Waiting {
            agent["state_detail"].as_str()
        } else {
            None
        }
    })
}
pub fn agent_context(agent: &Value, title: &str) -> Context {
    let mut context = Context {
        status: Some(AgentStatus::agent(agent)),
        ..Default::default()
    };
    context.text.insert("title".into(), title.into());
    for field in ["provider", "model", "effort"] {
        if let Some(text) = agent[field].as_str() {
            context.text.insert(field.into(), text.into());
        }
    }
    if let Some(question) = question(agent) {
        context.text.insert("question".into(), question.into());
    }
    add_usage(&mut context, [agent.clone()]);
    context
}
pub fn chat_context(
    chat: &Chat,
    state: &ViewState,
    now: i64,
    branch: Option<&str>,
    cache: Option<&WorkspaceFields>,
) -> Context {
    let mut context = Context {
        status: chat.status,
        unread: state.is_unread(&chat.key, chat.last_activity),
        ..Default::default()
    };
    context.text.insert("title".into(), chat.title.clone());
    context
        .text
        .insert("age".into(), age(chat.last_activity, now));
    if let Some(provider) = &chat.provider {
        context.text.insert("provider".into(), provider.clone());
    }
    if let Some(branch) = branch {
        context.text.insert("branch".into(), branch.into());
    }
    let children = chat.children();
    if !children.is_empty() {
        context
            .text
            .insert("children".into(), count_text(children.len(), "agent"));
    }
    // Prompt cards in the current session take precedence over waiting detail.
    if let Some(text) = chat
        .agents
        .iter()
        .find_map(|a| a["prompt"]["question"].as_str())
        .or_else(|| {
            chat.agents
                .iter()
                .find(|a| AgentStatus::agent(a) == AgentStatus::Waiting)
                .and_then(|a| a["state_detail"].as_str())
        })
    {
        context.text.insert("question".into(), text.into());
    }
    if let Some(chair) = chat.all_chairs.first() {
        for field in ["model", "effort"] {
            if let Some(text) = chair[field].as_str() {
                context.text.insert(field.into(), text.into());
            }
        }
    }
    add_usage(&mut context, chat.all_chairs.clone());
    context.add_workspace(cache);
    context
}
pub fn workspace_context(
    workspaces: &[Workspace],
    title: &str,
    owner: &super::choices::OwnerChoices,
    state: &ViewState,
    now: i64,
    cache: &BTreeMap<String, WorkspaceFields>,
) -> Context {
    let chats: Vec<_> = workspaces.iter().flat_map(|w| &w.chats).collect();
    let contexts: Vec<_> = chats
        .iter()
        .map(|c| chat_context(c, state, now, None, None))
        .collect();
    let mut context = Context {
        status: AgentStatus::aggregate(workspaces.iter().filter_map(Workspace::status)),
        unread: chats
            .iter()
            .any(|c| state.is_unread(&c.key, c.last_activity)),
        ..Default::default()
    };
    context.text.insert("title".into(), title.into());
    for field in ["provider", "model", "effort", "question"] {
        context.text.insert(
            field.into(),
            unique(contexts.iter().filter_map(|c| c.text.get(field).cloned())),
        );
    }
    context.text.insert(
        "branch".into(),
        workspaces
            .iter()
            .filter_map(|w| w.branch.as_deref())
            .collect::<Vec<_>>()
            .join(" · "),
    );
    context.text.insert(
        "children".into(),
        count_text(
            workspaces
                .iter()
                .flat_map(|w| w.listed_chats(owner))
                .count(),
            "chat",
        ),
    );
    if let Some(latest) = chats.iter().map(|c| c.last_activity).max() {
        context.text.insert("age".into(), age(latest, now));
    }
    add_usage(
        &mut context,
        chats.iter().flat_map(|c| c.all_chairs.clone()),
    );
    let dirty: Vec<_> = workspaces
        .iter()
        .filter_map(|w| cache.get(&w.path).and_then(|f| f.dirty_count))
        .collect();
    if !dirty.is_empty() {
        context.text.insert(
            "dirty".into(),
            format!("{} dirty", dirty.iter().sum::<usize>()),
        );
    }
    for field in ["pr", "ci"] {
        context.text.insert(
            field.into(),
            unique(
                workspaces
                    .iter()
                    .filter_map(|w| cache.get(&w.path))
                    .filter_map(|f| {
                        if field == "pr" {
                            f.pr.clone()
                        } else {
                            f.ci.clone()
                        }
                    }),
            ),
        );
    }
    context
}
fn unique(values: impl IntoIterator<Item = String>) -> String {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect::<Vec<_>>()
        .join(" · ")
}
pub fn requested_paths<'a>(
    field: &str,
    tree: &'a super::tree::Tree,
    lists: &RowFieldLists,
) -> Vec<&'a str> {
    let all = lists
        .project
        .iter()
        .chain(&lists.workspace)
        .any(|f| f == field);
    let chats = lists.chat.iter().chain(&lists.tab).any(|f| f == field);
    tree.projects
        .iter()
        .flat_map(|p| &p.workspaces)
        .filter(|w| !w.missing && !w.removed && (all || (chats && !w.chats.is_empty())))
        .map(|w| w.path.as_str())
        .collect()
}
pub fn github_fields(request: &Value) -> WorkspaceFields {
    let Some(number) = request["number"].as_u64() else {
        return WorkspaceFields::default();
    };
    let states: Vec<_> = request["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            c["conclusion"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or(c["state"].as_str())
                .or(c["status"].as_str())
                .unwrap_or_default()
                .to_uppercase()
        })
        .collect();
    let ci = if states.is_empty() {
        None
    } else if states.iter().any(|s| {
        [
            "FAILURE",
            "ERROR",
            "CANCELLED",
            "TIMED_OUT",
            "ACTION_REQUIRED",
            "STARTUP_FAILURE",
            "STALE",
        ]
        .contains(&s.as_str())
    }) {
        Some("CI failed".into())
    } else if states.iter().any(|s| {
        [
            "PENDING",
            "EXPECTED",
            "QUEUED",
            "IN_PROGRESS",
            "WAITING",
            "REQUESTED",
        ]
        .contains(&s.as_str())
    }) {
        Some("CI pending".into())
    } else if states
        .iter()
        .all(|s| ["SUCCESS", "NEUTRAL", "SKIPPED"].contains(&s.as_str()))
    {
        Some("CI passed".into())
    } else {
        Some("CI unknown".into())
    };
    WorkspaceFields {
        dirty_count: None,
        pr: Some(format!("PR #{number}")),
        ci,
    }
}
