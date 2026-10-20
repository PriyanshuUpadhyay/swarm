//! Server live rows.

use super::*;

pub(super) type Position = (u64, usize);
pub(super) const ROW_RECORD_LIMIT: usize = 2000;

#[derive(Clone)]
pub(super) struct LiveChunk {
    pub(super) records: Vec<Position>,
    pub(super) rows: Vec<Row>,
    pub(super) context: (bool, bool),
}
#[derive(Clone)]
pub(super) struct LiveRows {
    pub(super) records: BTreeMap<Position, Record>,
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) is_codex: bool,
    pub(super) shown: HashSet<String>,
    pub(super) last_rows: Vec<Row>,
    pub(super) last_usage: Option<view::ChatUsage>,
    pub(super) usage: view::UsageState,
    pub(super) chunks: BTreeMap<usize, LiveChunk>,
    pub(super) record_chunks: HashMap<Position, usize>,
    pub(super) dirty: HashSet<usize>,
    pub(super) rebuild: bool,
    pub(super) initialized: bool,
    pub(super) context: (bool, bool),
    pub(super) contexts: HashMap<Position, (bool, bool)>,
    pub(super) scope_start: Position,
    pub(super) saw_tool: bool,
    pub(super) anchors: HashMap<String, Vec<Position>>,
    pub(super) unmatched: HashSet<String>,
    pub(super) shell_outputs: HashSet<Position>,
    pub(super) next_chunk: usize,
    pub(super) tail_steps: usize,
    pub(super) last_prose: Option<(view::RowKind, String)>,
    pub(super) clear_row: Option<Row>,
    pub(super) pending_queue: Vec<Record>,
    pub(super) queue_cursor: Option<Position>,
    pub(super) last_queued: Vec<Row>,
    pub(super) queue_dirty: bool,
}
impl LiveRows {
    pub(super) fn empty(from: u64, is_codex: bool) -> Self {
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
        }
    }
    pub(super) fn ingest(&mut self, start: u64, end: u64, events: &[Value]) {
        self.start = self.start.min(start);
        self.end = self.end.max(end);
        for (index, event) in events.iter().enumerate() {
            let position = (start, index);
            if self.records.contains_key(&position) {
                continue;
            }
            let record = Record::new(event.clone(), format!("raw-{start}-{index}"), self.is_codex);
            self.usage.ingest(event);
            self.contexts.insert(position, self.context);
            self.records.insert(position, record);
            self.update_queue(position);
            self.append(position);
        }
    }
    pub(super) fn update_queue(&mut self, key: Position) {
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
    pub(super) fn append(&mut self, position: Position) {
        let event = &self.records[&position].event;
        let event_type = view::string(event, "type");
        if matches!(event_type, "page" | "turn_started")
            || (event_type == "user_message_chunk" && self.saw_tool)
        {
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
        if matches!(event_type, "tool_call" | "tool_call_update" | "tool_diff") {
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
        let step = swarm::view::is_step(&row) || event_type == "tool_call";
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
                    || self.tail_steps == swarm::view::FOLD_STEP_LIMIT));
        if new_chunk {
            let id = self.next_chunk;
            self.next_chunk += 1;
            self.chunks.insert(
                id,
                LiveChunk {
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

#[cfg(test)]
#[path = "live_rows/tests.rs"]
mod tests;
