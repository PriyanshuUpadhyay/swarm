use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabStrip {
    pub open: Vec<String>,
    pub groups: Vec<TabGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "color")]
    pub color: String,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub folded: bool,
}

fn color<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<String, D::Error> {
    let value = String::deserialize(decoder)?;
    Ok(if valid_color(&value) {
        value
    } else {
        "grey".into()
    })
}

pub fn valid_color(color: &str) -> bool {
    matches!(
        color,
        "grey" | "blue" | "green" | "yellow" | "orange" | "red" | "purple" | "pink"
    )
}

#[derive(Clone, Debug)]
pub enum Grouping {
    New {
        id: String,
        name: String,
        color: String,
        tab: String,
    },
    Add {
        tab: String,
        group: String,
    },
    Remove(String),
    Rename {
        group: String,
        name: String,
    },
    Color {
        group: String,
        color: String,
    },
    Fold {
        group: String,
        folded: bool,
    },
    Delete(String),
}

impl TabStrip {
    pub fn opening(&self, key: &str, current: Option<&str>) -> Self {
        if self.open.iter().any(|k| k == key) {
            return self.clone();
        }
        let mut strip = self.clone();
        let index = current
            .and_then(|k| self.open.iter().position(|v| v == k))
            .map_or(self.open.len(), |i| i + 1);
        strip.open.insert(index, key.into());
        strip.pruned(&strip.open.iter().cloned().collect())
    }

    pub fn closing(&self, key: &str) -> Self {
        self.pruned(
            &self
                .open
                .iter()
                .filter(|k| k.as_str() != key)
                .cloned()
                .collect(),
        )
    }

    pub fn moving(&self, key: &str, index: usize) -> Self {
        let Some(source) = self.open.iter().position(|k| k == key) else {
            return self.clone();
        };
        if index >= self.open.len() || source == index {
            return self.clone();
        }
        let target_group = self
            .groups
            .iter()
            .find(|g| g.members.contains(&self.open[index]))
            .map(|g| g.id.clone());
        let mut strip = self.clone();
        for group in &mut strip.groups {
            group.members.retain(|k| k != key);
            if Some(&group.id) == target_group.as_ref() {
                group.members.push(key.into());
            }
        }
        strip.open.remove(source);
        strip.open.insert(index, key.into());
        strip.pruned(&self.open.iter().cloned().collect())
    }

    pub fn step_targets(
        &self,
        pending: &BTreeSet<String>,
    ) -> BTreeMap<String, (Option<String>, Option<String>)> {
        let keys: Vec<_> = self.open.iter().filter(|k| !pending.contains(*k)).collect();
        let mut groups = BTreeMap::new();
        for group in &self.groups {
            for key in &group.members {
                groups.entry(key).or_insert(&group.id);
            }
        }
        let mut bounds: BTreeMap<&String, (usize, usize)> = BTreeMap::new();
        for (i, key) in keys.iter().enumerate() {
            if let Some(group) = groups.get(key) {
                bounds
                    .entry(group)
                    .and_modify(|b| b.1 = i)
                    .or_insert((i, i));
            }
        }
        keys.iter()
            .enumerate()
            .map(|(i, key)| {
                let (left, right) = if let Some(range) = groups.get(key).and_then(|g| bounds.get(g))
                {
                    (
                        (i > range.0).then(|| (*keys[i - 1]).clone()),
                        (i < range.1).then(|| (*keys[i + 1]).clone()),
                    )
                } else {
                    let left = (i > 0).then(|| {
                        let start = groups
                            .get(&keys[i - 1])
                            .and_then(|g| bounds.get(g))
                            .map_or(i - 1, |b| b.0);
                        (*keys[start]).clone()
                    });
                    let right = (i + 1 < keys.len()).then(|| {
                        let end = groups
                            .get(&keys[i + 1])
                            .and_then(|g| bounds.get(g))
                            .map_or(i + 1, |b| b.1);
                        (*keys[end]).clone()
                    });
                    (left, right)
                };
                ((*key).clone(), (left, right))
            })
            .collect()
    }

    pub fn stepping(&self, key: &str, target: &str) -> Self {
        let targets = self.step_targets(&BTreeSet::new());
        let Some((left, right)) = targets.get(key) else {
            return self.clone();
        };
        if left.as_deref() != Some(target) && right.as_deref() != Some(target) {
            return self.clone();
        }
        let mut strip = self.clone();
        let source = strip.open.iter().position(|k| k == key).unwrap();
        let destination = strip.open.iter().position(|k| k == target).unwrap();
        strip.open.remove(source);
        strip.open.insert(destination, key.into());
        strip.pruned(&self.open.iter().cloned().collect())
    }

    pub fn pruned(&self, listed: &BTreeSet<String>) -> Self {
        let mut strip = self.clone();
        let mut seen = BTreeSet::new();
        strip
            .open
            .retain(|k| listed.contains(k) && seen.insert(k.clone()));
        let mut grouped = BTreeSet::new();
        strip.groups = self
            .groups
            .iter()
            .filter_map(|group| {
                let mut group = group.clone();
                group.members = strip
                    .open
                    .iter()
                    .filter(|k| group.members.contains(k) && grouped.insert((*k).clone()))
                    .cloned()
                    .collect();
                (!group.members.is_empty()).then_some(group)
            })
            .collect();
        let mut emitted = BTreeSet::new();
        strip.open = strip
            .open
            .iter()
            .flat_map(
                |key| match strip.groups.iter().find(|g| g.members.contains(key)) {
                    Some(group) if emitted.insert(group.id.clone()) => group.members.clone(),
                    Some(_) => Vec::new(),
                    None => vec![key.clone()],
                },
            )
            .collect();
        strip
    }

    pub fn grouping(&self, action: Grouping) -> Self {
        let mut strip = self.clone();
        match action {
            Grouping::New {
                id,
                name,
                color,
                tab,
            } => {
                let name = name.trim();
                if name.is_empty()
                    || !self.open.contains(&tab)
                    || self.groups.iter().any(|g| g.id == id)
                {
                    return self.clone();
                }
                strip = strip.grouping(Grouping::Remove(tab.clone()));
                strip.groups.push(TabGroup {
                    id,
                    name: name.into(),
                    color,
                    members: vec![tab],
                    folded: false,
                });
            }
            Grouping::Add { tab, group } => {
                let Some(original) = self.groups.iter().find(|g| g.id == group) else {
                    return self.clone();
                };
                if !self.open.contains(&tab) || original.members.contains(&tab) {
                    return self.clone();
                }
                strip = strip.grouping(Grouping::Remove(tab.clone()));
                let Some(last) = original.members.last() else {
                    return self.clone();
                };
                strip.open.retain(|k| k != &tab);
                let index = strip
                    .open
                    .iter()
                    .position(|k| k == last)
                    .map_or(strip.open.len(), |i| i + 1);
                strip.open.insert(index, tab.clone());
                strip
                    .groups
                    .iter_mut()
                    .find(|g| g.id == group)
                    .unwrap()
                    .members
                    .push(tab);
            }
            Grouping::Remove(key) => {
                for group in &mut strip.groups {
                    group.members.retain(|k| k != &key);
                }
            }
            Grouping::Rename { group, name } => {
                if !name.trim().is_empty()
                    && let Some(g) = strip.groups.iter_mut().find(|g| g.id == group)
                {
                    g.name = name.trim().into();
                }
            }
            Grouping::Color { group, color } => {
                if valid_color(&color)
                    && let Some(g) = strip.groups.iter_mut().find(|g| g.id == group)
                {
                    g.color = color;
                }
            }
            Grouping::Fold { group, folded } => {
                if let Some(g) = strip.groups.iter_mut().find(|g| g.id == group) {
                    g.folded = folded;
                }
            }
            Grouping::Delete(id) => strip.groups.retain(|g| g.id != id),
        }
        strip.pruned(&self.open.iter().cloned().collect())
    }

    pub fn selection_after_close<'a>(&'a self, history: &'a [String]) -> Option<&'a str> {
        history
            .iter()
            .rev()
            .find(|k| self.open.contains(k))
            .or_else(|| self.open.first())
            .map(String::as_str)
    }
}

pub fn flip(
    history: &[String],
    open: &[String],
    current: Option<&str>,
    step: i64,
) -> Option<String> {
    let mut seen = BTreeSet::new();
    let mut recent: Vec<_> = history
        .iter()
        .rev()
        .filter(|k| open.contains(k) && seen.insert(*k))
        .collect();
    recent.reverse();
    if recent.is_empty() {
        return None;
    }
    let index = current.and_then(|k| recent.iter().position(|v| v.as_str() == k));
    let position = index.map_or_else(
        || if step < 0 { recent.len() - 1 } else { 0 },
        |i| {
            (i as i64 + step.rem_euclid(recent.len() as i64)).rem_euclid(recent.len() as i64)
                as usize
        },
    );
    Some(recent[position].clone())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTab {
    pub id: String,
    pub session: Option<String>,
    pub title: String,
    pub status: Option<super::tree::AgentStatus>,
    pub badge: Option<String>,
    pub can_close: bool,
    pub can_hide: bool,
    pub pending: Option<String>,
    pub fields: Vec<super::fields::FieldValue>,
    pub group: Option<TabGroup>,
    pub children: Option<ChildCount>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildCount {
    pub count: usize,
    pub waiting: usize,
    pub first_waiting: Option<String>,
    pub text: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct TabRun {
    pub id: String,
    pub group: Option<TabGroup>,
    pub tabs: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabsView {
    pub workspace: String,
    pub tabs: Vec<ChatTab>,
    pub strip: TabStrip,
    pub runs: Vec<TabRun>,
    pub history: Vec<String>,
    pub selected_chat: Option<String>,
    pub previous_recent: Option<String>,
    pub next_recent: Option<String>,
    pub step_targets: BTreeMap<String, (Option<String>, Option<String>)>,
}
pub fn badge(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "codex" => "X".into(),
        "agy" => "A".into(),
        _ => provider.chars().take(1).collect::<String>().to_uppercase(),
    }
}
pub fn view(
    tree: &super::tree::Tree,
    owner: &super::choices::OwnerChoices,
    state: &super::choices::ViewState,
    workspace: &str,
    now: i64,
    cache: &BTreeMap<String, super::fields::WorkspaceFields>,
) -> Option<TabsView> {
    let (_, entry) = tree.workspace(workspace)?;
    let listed: BTreeSet<_> = entry.chats.iter().map(|c| c.key.clone()).collect();
    let strip = owner
        .tabs
        .get(workspace)
        .cloned()
        .unwrap_or_else(|| entry.seed_tabs())
        .pruned(&listed);
    let pending: Vec<_> = state
        .pending
        .iter()
        .filter(|p| p.workspace == workspace)
        .collect();
    let starting: BTreeSet<_> = pending
        .iter()
        .filter_map(|p| p.session.as_deref())
        .collect();
    let mut tabs: Vec<_> = pending
        .iter()
        .rev()
        .map(|p| ChatTab {
            id: p.id.clone(),
            session: p.session.clone(),
            title: "New chat".into(),
            status: None,
            badge: None,
            can_close: false,
            can_hide: false,
            pending: Some(if p.state == "launched" {
                "starting".into()
            } else {
                p.state.clone()
            }),
            fields: Vec::new(),
            group: None,
            children: None,
        })
        .collect();
    for key in &strip.open {
        let Some(chat) = entry
            .chats
            .iter()
            .find(|c| c.key == *key && !starting.contains(c.id.as_str()))
        else {
            continue;
        };
        let children: Vec<_> = chat
            .children()
            .into_iter()
            .filter(|a| {
                a["alive"] == true
                    && super::tree::AgentStatus::agent(a) != super::tree::AgentStatus::Ended
            })
            .collect();
        let waiting: Vec<_> = children
            .iter()
            .filter(|a| super::tree::AgentStatus::agent(a) == super::tree::AgentStatus::Waiting)
            .collect();
        tabs.push(ChatTab {
            id: key.clone(),
            session: Some(chat.id.clone()),
            title: chat.title.clone(),
            status: chat.status,
            badge: chat.provider.as_deref().map(badge),
            can_close: chat.state_order() == 0 && !state.closing.contains(&chat.id),
            can_hide: true,
            pending: None,
            fields: super::fields::chat_context(
                chat,
                state,
                now,
                entry.branch.as_deref(),
                cache.get(workspace),
            )
            .values(&owner.fields.tab),
            group: strip
                .groups
                .iter()
                .find(|g| g.members.contains(key))
                .cloned(),
            children: (!children.is_empty()).then(|| ChildCount {
                count: children.len(),
                waiting: waiting.len(),
                first_waiting: waiting
                    .first()
                    .and_then(|a| a["id"].as_str())
                    .map(str::to_owned),
                text: format!(
                    "{}{}",
                    children.len(),
                    if waiting.is_empty() {
                        String::new()
                    } else {
                        format!(" · {} waiting", waiting.len())
                    }
                ),
            }),
        });
    }
    let mut runs: Vec<TabRun> = Vec::new();
    for tab in &tabs {
        if let Some(group) = &tab.group
            && let Some(last) = runs.last_mut()
            && last.group.as_ref().is_some_and(|g| g.id == group.id)
        {
            last.tabs.push(tab.id.clone());
            continue;
        }
        runs.push(TabRun {
            id: tab
                .group
                .as_ref()
                .map_or_else(|| format!("tab:{}", tab.id), |g| format!("group:{}", g.id)),
            group: tab.group.clone(),
            tabs: vec![tab.id.clone()],
        });
    }
    let selected = super::navigation::selected_chat(tree, owner, state, workspace);
    let current = selected.map(|c| c.key.as_str());
    let history = state
        .tab_history
        .get(workspace)
        .cloned()
        .unwrap_or_default();
    let pending: BTreeSet<_> = state.pending.iter().map(|p| p.id.clone()).collect();
    Some(TabsView {
        workspace: workspace.into(),
        previous_recent: flip(&history, &strip.open, current, -1),
        next_recent: flip(&history, &strip.open, current, 1),
        step_targets: strip.step_targets(&pending),
        selected_chat: selected.map(|c| c.id.clone()),
        tabs,
        runs,
        strip,
        history,
    })
}
pub fn overflow(tabs: &[ChatTab], edges: &BTreeMap<String, f64>, width: f64) -> Vec<String> {
    if width <= 0.0 {
        return Vec::new();
    }
    tabs.iter()
        .filter(|t| edges.get(&t.id).copied().unwrap_or_default() > width)
        .map(|t| t.id.clone())
        .collect()
}
