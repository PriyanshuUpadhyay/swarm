//! Parsed facts have no reader history and are never a wire type.

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
    pub(super) fn new(kind: &'static str, fields: Value) -> Self {
        Self {
            kind,
            fields,
            input: None,
        }
    }

    pub(super) fn project(&self, meta: &Value) -> NormalizedEvent {
        struct Event<'a> {
            template: &'a Template,
            meta: &'a Value,
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
                map.serialize_entry("meta", self.meta)?;
                map.end()
            }
        }
        NormalizedEvent::new(&Event {
            template: self,
            meta,
        })
    }
}

impl Fact<'_> {
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
    fn push_ignored(&mut self, kind: &str) {
        self.actions.push(Action::Event(self.ignored(kind)));
    }
}

pub(super) fn parse<'a>(line: &'a [u8], work: &mut dyn Work) -> Result<Fact<'a>, SnapshotError> {
    work.check()?;
    let mut fact = Fact {
        raw: line,
        meta: Value::Null,
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
    let Some(payload) = payload else {
        fact.push_unknown();
        return Ok(fact);
    };
    let kind = payload.text("type");
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
        "token_usage_record" | "world_state" | "inter_agent_communication_metadata" => {
            fact.push_ignored(&record_type)
        }
        "response_item" => match kind.as_str() {
            "ghost_snapshot" | "additional_tools" => fact.push_ignored(&kind),
            "message" => message(&mut fact, &payload, work)?,
            "reasoning" => reasoning(&mut fact, &payload, work)?,
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
            let hint = payload.text("user_facing_hint");
            let text = if kind == "entered_review_mode" {
                if hint.is_empty() {
                    "Entered review mode".into()
                } else {
                    format!("Entered review mode: {hint}")
                }
            } else {
                "Exited review mode".into()
            };
            fact.push("system_message", json!({"kind":"review_mode", "text":text}));
        }
        "item_completed" if plan_text(payload).is_some() => {
            fact.push("proposed_plan", json!({"text":plan_text(payload).unwrap()}));
        }
        "patch_apply_end" => super::codex_changes::append(fact, payload, "call_id", work)?,
        "item_completed"
            if payload
                .object("item")
                .is_some_and(|item| item.text("type") == "FileChange") =>
        {
            super::codex_changes::append(fact, &payload.object("item").unwrap(), "id", work)?;
        }
        "user_message"
        | "agent_message"
        | "exec_command_end"
        | "mcp_tool_call_end"
        | "web_search_end"
        | "item_completed"
        | "thread_settings_applied"
        | "sub_agent_activity" => fact.push_ignored(&format!("event_msg/{kind}")),
        _ => fact.push_unknown(),
    }
    Ok(())
}

fn message(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let role = payload.text("role");
    let Some(content) = payload
        .array("content")
        .filter(|_| matches!(role.as_str(), "user" | "assistant" | "developer" | "system"))
    else {
        fact.push_unknown();
        return Ok(());
    };
    let is_context = matches!(role.as_str(), "developer" | "system");
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
            let trimmed = text.trim_start_matches([' ', '\t', '\r', '\n']);
            if let Some((call_id, answers)) = codex_questions::reply(trimmed, work)? {
                fact.push(
                    "elicitation_result",
                    json!({"tool_call_id": call_id, "answers": answers}),
                );
            } else if [
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
            .any(|prefix| trimmed.starts_with(prefix))
            {
                fact.push("system_message", json!({"kind": "context", "text": text}));
            } else if super::claude_meta::ring(&text) {
                fact.push(
                    "system_message",
                    json!({"kind": "swarm_ring", "text": text}),
                );
            } else {
                fact.push("user_message_chunk", json!({"text": text}));
            }
        } else {
            fact.push("agent_message_chunk", json!({"text": text}));
        }
    }
    if fact.actions.is_empty() {
        fact.push_unknown();
    }
    Ok(())
}

fn reasoning(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let summary = payload.get("summary");
    if summary.is_none_or(|raw| raw.get() == "null") {
        fact.push("agent_thought_chunk", json!({"text":""}));
        return Ok(());
    }
    let Some(parts) = summary.and_then(claude_json::array) else {
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

/// The text of a completed Codex plan-mode item.
fn plan_text(payload: &Object<'_>) -> Option<String> {
    let item = payload.object("item")?;
    (item.text("type") == "Plan")
        .then(|| item.optional_text("text"))
        .flatten()
}
