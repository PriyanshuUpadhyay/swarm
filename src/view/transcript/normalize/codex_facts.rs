//! Parsed facts have no reader history and are never a wire type.

use super::super::{CODEX_MIRROR, CONTEXT_COMPACTED};
use super::claude_input::Input;
use super::claude_json::{self, Object};
use super::codex_questions::{self, QuestionOutput};
use super::{NormalizedEvent, SnapshotError, Work};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Value, json, value::RawValue};

#[derive(Debug)]
pub(crate) struct Fact<'a> {
    raw: &'a [u8],
    pub(super) meta: Value,
    pub(super) source_kind: String,
    pub(super) actions: Vec<Action>,
}

#[derive(Debug)]
pub(super) enum Action {
    Event(Template),
    Open {
        call_id: String,
        ids: Vec<String>,
        event: Template,
    },
    Output {
        call_id: String,
        question: QuestionOutput,
        event: Template,
        tail: Vec<Template>,
    },
}

#[derive(Debug)]
pub(super) struct Template {
    kind: &'static str,
    fields: Value,
    pub(super) input: Option<Input>,
}

impl Template {
    pub(super) fn is_mirror(&self) -> bool {
        self.fields[CODEX_MIRROR] == true
    }
    pub(super) fn starts_turn(&self) -> bool {
        self.kind == "turn_started"
    }
    pub(super) fn new(kind: &'static str, fields: Value) -> Self {
        Self {
            kind,
            fields,
            input: None,
        }
    }

    pub(super) fn spawn_call(&self) -> Option<&str> {
        (self.kind == "tool_call" && self.fields["name"] == "spawn_agent")
            .then(|| self.fields["tool_call_id"].as_str())
            .flatten()
    }

    pub(super) fn project_spawn_output(
        &self,
        meta: &Value,
        work: &mut dyn Work,
    ) -> Result<NormalizedEvent, SnapshotError> {
        work.check()?;
        let raw = self.fields["content"]
            .as_str()
            .and_then(|text| serde_json::from_str::<&RawValue>(text).ok());
        work.check()?;

        let agent_id = if let Some(raw) = raw {
            if claude_json::valid(raw, work)? {
                Object::parse(raw).and_then(|output| output.optional_text("agent_id"))
            } else {
                None
            }
        } else {
            None
        };
        work.check()?;

        Ok(self.project_with_agent(meta, agent_id.as_deref()))
    }

    pub(super) fn project(&self, meta: &Value) -> NormalizedEvent {
        self.project_with_agent(meta, None)
    }

    fn project_with_agent(&self, meta: &Value, agent_id: Option<&str>) -> NormalizedEvent {
        struct Event<'a> {
            template: &'a Template,
            meta: &'a Value,
            agent_id: Option<&'a str>,
        }
        impl Serialize for Event<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", self.template.kind)?;
                if let Value::Object(fields) = &self.template.fields {
                    for (key, value) in fields {
                        map.serialize_entry(key, value)?;
                    }
                }
                if let Some(input) = &self.template.input {
                    map.serialize_entry("input", input)?;
                }
                if let Some(id) = self.agent_id {
                    map.serialize_entry("agent_thread_id", id)?;
                }
                map.serialize_entry("meta", self.meta)?;
                map.end()
            }
        }
        NormalizedEvent::new(&Event {
            template: self,
            meta,
            agent_id,
        })
    }
}

impl Fact<'_> {
    pub(super) fn mark_mirrors(&mut self, start: usize) {
        for action in &mut self.actions[start..] {
            match action {
                Action::Event(event) | Action::Open { event, .. } => {
                    event.fields[CODEX_MIRROR] = json!(true)
                }
                Action::Output { event, tail, .. } => {
                    event.fields[CODEX_MIRROR] = json!(true);
                    for event in tail {
                        event.fields[CODEX_MIRROR] = json!(true);
                    }
                }
            }
        }
    }
    pub(super) fn call(&mut self, id: &str, name: &str, status: &str, input: Input) {
        let mut event = Template::new(
            "tool_call",
            json!({"tool_call_id":id,"name":name,"status":status}),
        );
        event.input = Some(input);
        self.actions.push(Action::Event(event));
    }

    pub(super) fn compaction(&mut self) {
        self.push(
            "system_message",
            json!({"kind":"compaction","text":CONTEXT_COMPACTED}),
        );
    }

    pub(super) fn review_mode(&mut self, entered: bool, hint: &str) {
        let text = if !entered {
            "Exited review mode".into()
        } else if hint.is_empty() {
            "Entered review mode".into()
        } else {
            format!("Entered review mode: {hint}")
        };

        self.push("system_message", json!({"kind":"review_mode","text":text}));
    }

    pub(super) fn push(&mut self, kind: &'static str, fields: Value) {
        self.actions
            .push(Action::Event(Template::new(kind, fields)));
    }
    pub(super) fn unknown(&self) -> Template {
        Template::new("unknown", json!({"raw": String::from_utf8_lossy(self.raw)}))
    }
    pub(super) fn ignored(&self, kind: &str) -> Template {
        Template::new(
            "ignored",
            json!({"kind": kind, "raw": String::from_utf8_lossy(self.raw)}),
        )
    }
    pub(super) fn push_unknown(&mut self) {
        self.actions.push(Action::Event(self.unknown()));
    }
    pub(super) fn push_ignored(&mut self, kind: &str) {
        self.actions.push(Action::Event(self.ignored(kind)));
    }
}

pub(super) fn parse<'a>(line: &'a [u8], work: &mut dyn Work) -> Result<Fact<'a>, SnapshotError> {
    work.check()?;
    let mut fact = Fact {
        raw: line,
        meta: Value::Null,
        source_kind: String::new(),
        actions: Vec::new(),
    };

    let raw = std::str::from_utf8(line)
        .ok()
        .and_then(|text| serde_json::from_str::<&RawValue>(text).ok());
    let Some(raw) = raw else {
        fact.push_unknown();
        return Ok(fact);
    };

    if !claude_json::valid(raw, work)? {
        fact.push_unknown();
        return Ok(fact);
    }
    let Some(record) = Object::parse(raw) else {
        fact.push_unknown();
        return Ok(fact);
    };

    let record_type = record.text("type");
    let payload = record.object("payload");
    let id = payload
        .as_ref()
        .map(|obj| obj.text("id"))
        .unwrap_or_default();
    fact.meta = json!({
        "session_id": if record_type == "session_meta" { id.clone() } else { String::new() },
        "uuid": id, "timestamp": record.text("timestamp"),
        "parent_uuid": null, "source_tool_use_id": null,
    });

    if matches!(
        record_type.as_str(),
        "token_usage_record"
            | "world_state"
            | "inter_agent_communication_metadata"
            | "inter_agent_communication"
            | "security_risk_score"
            | "dynamic_tools_updated"
            | "realtime_item"
    ) {
        fact.push_ignored(&record_type);
        return Ok(fact);
    }

    let Some(payload) = payload else {
        fact.push_unknown();
        return Ok(fact);
    };
    let kind = payload.text("type");
    fact.source_kind = format!("{record_type}/{kind}");
    match record_type.as_str() {
        "session_meta" | "turn_context" => {
            for key in ["model", "cwd"] {
                if let Some(value) = payload.optional_text(key) {
                    fact.push("session_info", json!({"kind": key, "value": value}));
                }
            }
            if fact.actions.is_empty() {
                fact.push_ignored(&record_type);
            }
        }

        "event_msg" => event_message(&mut fact, &payload, &kind, work)?,
        "compacted" => fact.push(
            "system_message",
            json!({"kind": "compaction", "text": payload.text("message")}),
        ),
        "retained_context" => fact.push_ignored(&format!("retained_context/{kind}")),
        "response_item" => match kind.as_str() {
            "ghost_snapshot" | "additional_tools" | "configuration_update" => {
                fact.push_ignored(&kind)
            }
            "message" => message(&mut fact, &payload, work)?,
            "reasoning" => reasoning(&mut fact, &payload, work)?,
            "image_generation_call" => image_generation(&mut fact, &payload, "id"),
            "compaction" | "compaction_summary" | "context_compaction" => {
                let start = fact.actions.len();
                fact.compaction();
                fact.mark_mirrors(start);
            }

            "agent_message" => {
                for raw in payload.array("content").unwrap_or_default() {
                    work.check()?;
                    if let Some(part) = Object::parse(raw)
                        && part.text("type") == "input_text"
                    {
                        fact.push("agent_message_chunk", json!({"text": part.text("text")}));
                    }
                }
                if fact.actions.is_empty() {
                    fact.push_ignored("response_item/agent_message");
                }
            }
            _ => {
                if !super::codex_tools::append(&mut fact, &payload, &kind, work)? {
                    fact.push_unknown();
                }
            }
        },
        _ => fact.push_unknown(),
    }

    Ok(fact)
}

fn event_message(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    kind: &str,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    match kind {
        "agent_reasoning" => {
            fact.push("agent_thought_chunk", json!({"text": payload.text("text")}))
        }
        "agent_reasoning_raw_content" => {
            fact.push("agent_thought_part", json!({"text":payload.text("text")}))
        }
        "task_started" => fact.push("turn_started", json!({})),

        "task_complete" | "turn_aborted" => fact.push(
            "turn_ended",
            json!({
                "duration_ms": payload.integer("duration_ms"),
                "reason": if kind == "task_complete" { "completed" } else { "aborted" },
            }),
        ),

        "context_compacted" => fact.push(
            "system_message",
            json!({"kind": "compaction", "text": payload.text("message")}),
        ),
        "token_count" => {
            if let Some(info) = payload.object("info")
                && let Some(last) = info.object("last_token_usage")
            {
                let total = info.object("total_token_usage");
                fact.push("usage", json!({
                        "source": "codex", "kind": "context",
                        "context_tokens": last.token("total_tokens"),
                        "context_capacity_tokens": info.token("model_context_window"),
                        "input_tokens": last.token("input_tokens"), "output_tokens": last.token("output_tokens"),
                        "cache_read_tokens": last.token("cached_input_tokens"),
                        "cache_write_tokens": last.token("cache_write_input_tokens"),
                        "session_input_tokens": total.as_ref().and_then(|obj| obj.token("input_tokens")),
                        "session_output_tokens": total.as_ref().and_then(|obj| obj.token("output_tokens")),
                        "cost_usd": null, "cost_completeness": "unknown",
                    }));
                return Ok(());
            }
            fact.push_ignored("event_msg/token_count");
        }

        "error" => fact.push("error", json!({"message": payload.text("message")})),
        "entered_review_mode" | "exited_review_mode" => {
            fact.review_mode(
                kind == "entered_review_mode",
                &payload.text("user_facing_hint"),
            );
        }

        "item_completed" => super::codex_items::append(fact, payload, work)?,
        "patch_apply_end" => super::codex_changes::append(fact, payload, "call_id", work)?,
        "sub_agent_activity" => subagent_activity(fact, payload),
        "image_generation_end" => {
            let start = fact.actions.len();
            image_generation(fact, payload, "call_id");
            fact.mark_mirrors(start);
        }

        "thread_rolled_back" => {
            if let Some(turns) = payload.token("num_turns") {
                fact.push("thread_rolled_back", json!({"num_turns":turns}));
            } else {
                fact.push_unknown();
            }
        }
        "user_message"
        | "agent_message"
        | "exec_command_end"
        | "mcp_tool_call_end"
        | "web_search_end"
        | "thread_settings_applied"
        | "thread_goal_updated" => fact.push_ignored(&format!("event_msg/{kind}")),
        _ => fact.push_unknown(),
    }

    Ok(())
}

pub(super) fn subagent_activity(fact: &mut Fact<'_>, payload: &Object<'_>) {
    fact.push("subagent_activity", json!({
        "agent_thread_id":payload.text("agent_thread_id"), "agent_path":payload.text("agent_path"),
        "kind":payload.text("kind"),
    }));
}

pub(super) fn image_generation(fact: &mut Fact<'_>, payload: &Object<'_>, id_field: &str) {
    fact.meta["uuid"] = json!(payload.text(id_field));
    fact.push(
        "image",
        json!({"role":"agent", "media_type":"image", "text":payload.optional_text("saved_path").or_else(|| payload.optional_text("savedPath")).unwrap_or_default()}),
    );
}

fn message(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let role = payload.text("role");
    if role == "system" {
        fact.push_ignored("response_item/message/system");
        return Ok(());
    }

    let Some(content) = payload
        .array("content")
        .filter(|_| matches!(role.as_str(), "user" | "assistant" | "developer"))
    else {
        fact.push_unknown();
        return Ok(());
    };

    let is_context = role == "developer";
    let mut has_unknown = false;
    for raw in content {
        work.check()?;
        let part = Object::parse(raw);
        if let Some(part) = &part
            && part.text("type") == "input_image"
        {
            fact.push("image", json!({
                    "role": if role == "user" { "user" } else if is_context { "tool" } else { "agent" },
                    "media_type": codex_questions::media_type(part),
                }));
            continue;
        }

        let text = part.as_ref().and_then(|part| {
            let kind = part.text("type");
            if matches!(kind.as_str(), "input_text" | "output_text")
                || role == "user" && kind == "text"
            {
                part.optional_text("text")
            } else {
                None
            }
        });

        let Some(text) = text else {
            if !has_unknown {
                fact.push_unknown();
                has_unknown = true;
            }
            continue;
        };

        if is_context {
            fact.push("system_message", json!({"kind": role, "text": text}));
        } else if role == "user" {
            user_text(fact, &text, work)?;
        } else {
            fact.push("agent_message_chunk", json!({"text": text}));
        }
    }

    if fact.actions.is_empty() {
        fact.push_unknown();
    }
    Ok(())
}

pub(super) fn user_text(
    fact: &mut Fact<'_>,
    text: &str,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let trimmed = text.trim_start_matches([' ', '\t', '\r', '\n']);

    if let Some((call_id, answers)) = codex_questions::reply(trimmed, work)? {
        fact.push(
            "elicitation_result",
            json!({"tool_call_id": call_id, "answers": answers}),
        );
        return Ok(());
    }

    if trimmed.starts_with("<hook_prompt ") || trimmed.starts_with("<hook_prompt>") {
        let text = trimmed
            .split_once('>')
            .and_then(|(_, body)| body.strip_suffix("</hook_prompt>"))
            .unwrap_or(text);
        fact.push("system_message", json!({"kind":"hook_prompt", "text":text}));
        return Ok(());
    }

    let context = [
        "<environment_context>",
        "<user_instructions>",
        "<permissions instructions>",
        "<skill>",
        "<user_shell_command>",
        "<turn_aborted>",
        "<recommended_plugins>",
        "<subagent_notification>",
        "<no retained transcript delta entries>",
        "# AGENTS.md",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix));

    if context {
        fact.push("system_message", json!({"kind": "context", "text": text}));
    } else if super::claude_meta::ring(text) {
        fact.push(
            "system_message",
            json!({"kind": "swarm_ring", "text": text}),
        );
    } else {
        fact.push("user_message_chunk", json!({"text": text}));
    }

    Ok(())
}

fn reasoning(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let Some(summary) = payload.non_null("summary") else {
        fact.push("agent_thought_chunk", json!({"text":""}));
        return Ok(());
    };

    let Some(parts) = claude_json::array(summary) else {
        fact.push_unknown();
        return Ok(());
    };
    let mut has_unknown = false;
    for raw in parts {
        work.check()?;

        if let Some(text) = Object::parse(raw).and_then(|part| part.optional_text("text")) {
            if !text.is_empty() {
                fact.push("agent_thought_chunk", json!({"text": text}));
            }
        } else if !has_unknown {
            fact.push_unknown();
            has_unknown = true;
        }
    }

    if fact.actions.is_empty() {
        fact.push("agent_thought_chunk", json!({"text":""}));
    }
    Ok(())
}

pub(super) fn status(item: &Object<'_>) -> &'static str {
    if item.text("status") == "interrupted" {
        "interrupted"
    } else if item.boolean("success") == Some(false)
        || matches!(item.text("status").as_str(), "failed" | "declined")
    {
        "failed"
    } else {
        "completed"
    }
}
