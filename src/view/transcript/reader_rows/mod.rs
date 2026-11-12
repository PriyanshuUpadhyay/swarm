//! Retained transcript rows shared by source readers and server adapters.

use super::{self as view, Record, Row};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

mod delta;
mod pages;
mod projection;
mod render;
pub use delta::RowDelta;
pub use pages::RowPage;

pub type Position = (u64, usize);
pub const ROW_RECORD_LIMIT: usize = 2000;

#[derive(Clone, Debug)]
pub struct ReaderChunk {
    pub records: Vec<Position>,
    pub rows: Vec<Row>,
    context: (bool, bool),
}
#[derive(Clone)]
pub struct ReaderRows {
    pub records: BTreeMap<Position, Record>,
    pub start: u64,
    pub end: u64,
    pub is_codex: bool,
    pub shown: HashSet<String>,
    pub last_rows: Vec<Row>,
    pub last_usage: Option<view::ChatUsage>,
    pub usage: view::UsageState,
    pub chunks: BTreeMap<usize, ReaderChunk>,
    record_chunks: HashMap<Position, usize>,
    pub dirty: HashSet<usize>,
    pub rebuild: bool,
    pub initialized: bool,
    context: (bool, bool),
    contexts: HashMap<Position, (bool, bool)>,
    scope_start: Position,
    saw_tool: bool,
    anchors: HashMap<String, Vec<Position>>,
    unmatched: HashSet<String>,
    shell_outputs: HashSet<Position>,
    pub(super) next_chunk: usize,
    pub(super) tail_steps: usize,
    pub(super) last_prose: Option<(view::RowKind, String)>,
    clear_row: Option<Row>,
    pending_queue: Vec<Record>,
    queue_cursor: Option<Position>,
    last_queued: Vec<Row>,
    queue_dirty: bool,
    pub(super) unavailable: BTreeSet<Position>,
}
impl std::fmt::Debug for ReaderRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReaderRows")
            .field("start", &self.start)
            .field("end", &self.end)
            .field("is_codex", &self.is_codex)
            .field("records", &self.records.len())
            .finish_non_exhaustive()
    }
}
impl ReaderRows {
    pub fn empty(from: u64, is_codex: bool) -> Self {
        Self {
            records: BTreeMap::new(),
            start: from,
            end: from,
            is_codex,
            shown: HashSet::new(),
            last_rows: Vec::new(),
            last_usage: None,
            usage: view::UsageState::default(),
            chunks: BTreeMap::new(),
            record_chunks: HashMap::new(),
            dirty: HashSet::new(),
            rebuild: false,
            initialized: false,
            context: (from == 0, from > 0),
            contexts: HashMap::new(),
            scope_start: (from, 0),
            saw_tool: false,
            anchors: HashMap::new(),
            unmatched: HashSet::new(),
            shell_outputs: HashSet::new(),
            next_chunk: 0,
            tail_steps: 0,
            last_prose: None,
            clear_row: None,
            pending_queue: Vec::new(),
            queue_cursor: None,
            last_queued: Vec::new(),
            queue_dirty: false,
            unavailable: BTreeSet::new(),
        }
    }
    pub fn ingest(&mut self, start: u64, end: u64, events: &[Value]) {
        self.start = self.start.min(start);
        self.end = self.end.max(end);
        for (index, event) in events.iter().enumerate() {
            let position = (start, index);
            if self.records.contains_key(&position) {
                continue;
            }
            let record = Record::new(event.clone(), format!("raw-{start}-{index}"), self.is_codex);
            self.insert_record(position, record);
        }
    }
    fn insert_record(&mut self, position: Position, record: Record) {
        self.usage.ingest(&record.event);
        self.contexts.insert(position, self.context);
        self.records.insert(position, record);
        self.update_queue(position);
        self.append(position);
    }
    fn update_queue(&mut self, key: Position) {
        let record = &self.records[&key];
        let event = &record.event;
        if event["type"] != "queue_operation"
            || self.queue_cursor.is_some_and(|cursor| key <= cursor)
        {
            return;
        }
        self.queue_cursor = Some(key);
        self.queue_dirty = true;
        match view::string(event, "operation") {
            "enqueue" if event["content"].is_string() => self.pending_queue.push(record.clone()),
            "dequeue" if !self.pending_queue.is_empty() => {
                self.pending_queue.remove(0);
            }
            "remove" | "popAll" => {
                if let Some(index) = self
                    .pending_queue
                    .iter()
                    .position(|record| record.event["content"] == event["content"])
                {
                    self.pending_queue.remove(index);
                }
            }
            _ => (),
        }
    }
    fn append(&mut self, position: Position) {
        let event = &self.records[&position].event;
        let event_type = view::string(event, "type");
        // Rebuild only a batch with a join participant, including a later canonical copy.
        self.rebuild |=
            self.is_codex && crate::view::builder::codex::needs_join(&self.records[&position]);

        if crate::view::builder::starts_scope(event_type, self.saw_tool) {
            self.scope_start = position;
            self.saw_tool = false;
            self.anchors.clear();
            self.unmatched.clear();
            self.shell_outputs.clear();
        }
        if matches!(event_type, "turn_started" | "turn_ended") {
            self.context.0 = true;
        }
        let mut row = view::row(&self.records[&position]);
        if let Some(row) = &mut row {
            if self.is_codex
                && (row.kind == view::RowKind::User
                    || row.system_kind.as_deref() == Some("swarm_ring"))
            {
                if row.kind == view::RowKind::User && !self.context.0 {
                    row.arrives_mid_turn = true;
                }
                self.context.0 = false;
            }
            if row.system_kind.as_deref() == Some("swarm_ring") && !self.context.1 {
                row.starts_turn = true;
            }
            if row.opens_turn() {
                self.context.1 = true;
            }
            if row.ends_turn {
                self.context.1 = false;
            }
            row.collapsed = view::is_collapsed(row);
        }
        let reference = match event_type {
            "tool_call_update" | "tool_diff" => {
                Some(format!("tool:{}", view::string(event, "tool_call_id")))
            }
            "system_message" => match view::string(event, "kind") {
                "shell_output" => Some(format!(
                    "shell:{}",
                    event["meta"]["parent_uuid"].as_str().unwrap_or("")
                )),
                "command_output" | "skill_body" | "injected" => {
                    let kind = view::string(event, "kind");
                    let text = view::string(event, "text").trim_start();
                    if kind == "injected"
                        && ["<system-reminder>", "<local-command-caveat>"]
                            .iter()
                            .any(|prefix| text.starts_with(prefix))
                    {
                        None
                    } else {
                        let command = format!(
                            "command:{}",
                            event["meta"]["parent_uuid"].as_str().unwrap_or("")
                        );
                        if self.anchors.contains_key(&command) {
                            Some(command)
                        } else if kind != "command_output" {
                            let tool = format!(
                                "tool:{}",
                                event["meta"]["source_tool_use_id"].as_str().unwrap_or("")
                            );
                            if kind != "injected"
                                || self.anchors.get(&tool).is_some_and(|anchors| {
                                    anchors.iter().any(|key| {
                                        view::string(&self.records[key].event, "name") == "Skill"
                                    })
                                })
                            {
                                Some(tool)
                            } else {
                                None
                            }
                        } else {
                            Some(command)
                        }
                    }
                }
                _ => None,
            },
            _ => None,
        };
        let reference =
            reference.filter(|id| id.split_once(':').is_some_and(|(_, id)| !id.is_empty()));
        let target = reference
            .as_ref()
            .and_then(|id| self.anchors.get(id))
            .and_then(|anchors| {
                let session = event["meta"]["session_id"].as_str().unwrap_or("");
                let mut matches = anchors.iter().filter(|key| {
                    let anchor = self.records[key].event["meta"]["session_id"]
                        .as_str()
                        .unwrap_or("");
                    (session.is_empty() || anchor.is_empty() || session == anchor)
                        && (view::string(event, "kind") != "injected"
                            || !reference.as_ref().is_some_and(|id| id.starts_with("tool:"))
                            || view::string(&self.records[key].event, "name") == "Skill")
                });
                let first = matches.next()?;
                matches.next().is_none().then_some(*first)
            });
        let target = target.filter(|key| {
            view::string(event, "kind") != "shell_output" || !self.shell_outputs.contains(key)
        });
        if view::string(event, "kind") == "shell_output"
            && let Some(key) = target
        {
            self.shell_outputs.insert(key);
        }
        if target.is_none()
            && let Some(reference) = &reference
        {
            self.unmatched.insert(reference.clone());
        }
        let anchor = match event_type {
            "tool_call" => Some(format!("tool:{}", view::string(event, "tool_call_id"))),
            "system_message" if view::string(event, "kind") == "shell_input" => Some(format!(
                "shell:{}",
                event["meta"]["uuid"].as_str().unwrap_or("")
            )),
            "system_message" if row.as_ref().is_some_and(|row| row.command.is_some()) => Some(
                format!("command:{}", event["meta"]["uuid"].as_str().unwrap_or("")),
            ),
            _ => None,
        };
        if let Some(anchor) = anchor {
            if self.unmatched.contains(&anchor) {
                self.rebuild = true;
            }
            let anchors = self.anchors.entry(anchor).or_default();
            if !anchors.is_empty() {
                self.rebuild = true;
            }
            anchors.push(position);
        }
        if crate::view::builder::is_tool_event(event_type) {
            self.saw_tool = true;
        }
        if event_type == "turn_ended" {
            self.rebuild = true;
            self.scope_start = position;
            self.saw_tool = false;
            self.anchors.clear();
            self.unmatched.clear();
            self.shell_outputs.clear();
        }
        if matches!(view::string(event, "kind"), "skill_body" | "injected")
            && target.is_some_and(|key| {
                view::string(&self.records[&key].event, "type") == "system_message"
            })
        {
            self.context.1 = true;
        }
        // A checklist hides the older ones and replays task changes over every record, and a
        // task's id arrives with its call's result, so each of them needs a full build to make the
        // live rows equal to the stored rows.
        let is_todo_list = |row: &Row| row.kind == view::RowKind::TodoList;
        if row.as_ref().is_some_and(is_todo_list)
            || target
                .and_then(|key| view::row(&self.records[&key]))
                .as_ref()
                .is_some_and(is_todo_list)
        {
            self.rebuild = true;
        }

        if !self.initialized {
            return;
        }
        if let Some(target) = target.and_then(|key| self.record_chunks.get(&key).copied()) {
            self.chunks.get_mut(&target).unwrap().records.push(position);
            self.record_chunks.insert(position, target);
            self.dirty.insert(target);
            self.last_prose = None;
            return;
        }
        let Some(row) = row else {
            if matches!(event_type, "turn_started" | "page")
                && let Some((&id, _)) = self.chunks.last_key_value()
            {
                self.chunks.get_mut(&id).unwrap().records.push(position);
                self.record_chunks.insert(position, id);
                self.dirty.insert(id);
            }
            return;
        };
        let step =
            crate::view::is_step(&row) || (event_type == "tool_call" && !row.kind.is_checklist());
        let prose = matches!(
            row.kind,
            view::RowKind::User | view::RowKind::Assistant | view::RowKind::Thought
        )
        .then(|| (row.kind, row.id.clone()));
        let merges = prose.is_some() && self.last_prose == prose;
        let new_chunk = self.chunks.is_empty()
            || (!merges
                && (!step
                    || self.tail_steps == 0
                    || self.tail_steps == crate::view::FOLD_STEP_LIMIT));
        if new_chunk {
            let id = self.next_chunk;
            self.next_chunk += 1;
            self.chunks.insert(
                id,
                ReaderChunk {
                    records: Vec::new(),
                    rows: Vec::new(),
                    context: self.contexts[&position],
                },
            );
            self.tail_steps = 0;
        }
        let id = *self.chunks.last_key_value().unwrap().0;
        self.chunks.get_mut(&id).unwrap().records.push(position);
        self.record_chunks.insert(position, id);
        self.dirty.insert(id);
        if step {
            if !merges {
                self.tail_steps += 1;
            }
        } else {
            self.tail_steps = 0;
        }
        self.last_prose = prose;
    }
}
