//! Owned AGY line facts keep native event order and bounded numeric inputs.

use super::claude_input::{self, Input};
use super::claude_json::{self, Object};
use super::claude_meta::ring;
use super::{NormalizedEvent, SnapshotError, Work};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Value, json, value::RawValue};

#[derive(Debug)]
pub(crate) struct Fact {
    events: Vec<Template>,
}

#[derive(Clone, Debug)]
struct Meta {
    uuid: String,
    timestamp: String,
}

#[derive(Debug)]
struct Template {
    kind: &'static str,
    fields: Value,
    meta: Option<Meta>,
    input: Option<Input>,
}

impl Fact {
    pub(crate) fn project(
        &self,
        session_id: &str,
        work: &mut dyn Work,
    ) -> Result<Vec<NormalizedEvent>, SnapshotError> {
        work.check()?;
        let mut events = Vec::with_capacity(self.events.len());
        for template in &self.events {
            work.check()?;
            events.push(NormalizedEvent::new(&Projected {
                template,
                session_id,
            }));
        }
        Ok(events)
    }

    fn push(&mut self, kind: &'static str, meta: &Option<Meta>, fields: Value) {
        self.events.push(Template {
            kind,
            fields,
            meta: meta.clone(),
            input: None,
        });
    }

    fn unknown(&mut self, meta: &Option<Meta>, line: &[u8]) {
        self.push(
            "unknown",
            meta,
            json!({"raw": String::from_utf8_lossy(line)}),
        );
    }

    fn ignored(&mut self, meta: &Option<Meta>, kind: &str, line: &[u8]) {
        self.push(
            "ignored",
            meta,
            json!({"kind": kind, "raw": String::from_utf8_lossy(line)}),
        );
    }
}

struct Projected<'a> {
    template: &'a Template,
    session_id: &'a str,
}

impl Serialize for Projected<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Metadata<'a> {
            session_id: &'a str,
            uuid: &'a str,
            timestamp: &'a str,
            parent_uuid: Option<&'a str>,
            source_tool_use_id: Option<&'a str>,
        }

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
        let meta = self.template.meta.as_ref().map(|meta| Metadata {
            session_id: self.session_id,
            uuid: &meta.uuid,
            timestamp: &meta.timestamp,
            parent_uuid: None,
            source_tool_use_id: None,
        });
        map.serialize_entry("meta", &meta)?;
        map.end()
    }
}

pub(crate) fn session_id_from_path(path: &str) -> String {
    let mut parts = path.split('/');
    while let Some(part) = parts.next() {
        if part == "brain" {
            let Some(session_id) = parts.next() else {
                return String::new();
            };
            if !session_id.is_empty() && parts.next().is_some() {
                return session_id.to_owned();
            }
        }
    }
    String::new()
}

pub(crate) fn parse_fact(line: &[u8], work: &mut dyn Work) -> Result<Fact, SnapshotError> {
    work.check()?;
    let mut fact = Fact { events: Vec::new() };
    let raw = std::str::from_utf8(line)
        .ok()
        .and_then(|text| serde_json::from_str::<&RawValue>(text).ok());
    let Some(raw) = raw else {
        fact.unknown(&None, line);
        return Ok(fact);
    };
    if !claude_json::valid(raw, work)? {
        fact.unknown(&None, line);
        return Ok(fact);
    }
    let Some(record) = Object::parse(raw) else {
        fact.unknown(&None, line);
        return Ok(fact);
    };
    let (Some(step_index), Some(timestamp)) = (
        record.token("step_index"),
        record.optional_text("created_at"),
    ) else {
        fact.unknown(&None, line);
        return Ok(fact);
    };
    let meta = Some(Meta {
        uuid: step_index.to_string(),
        timestamp,
    });
    let (Some(kind), Some(status), Some(_source)) = (
        record.optional_text("type"),
        record.optional_text("status"),
        record.optional_text("source"),
    ) else {
        fact.unknown(&meta, line);
        return Ok(fact);
    };

    match kind.as_str() {
        "USER_INPUT" => match record.optional_text("content") {
            Some(content) => user_input(&mut fact, &meta, &content),
            None => fact.unknown(&meta, line),
        },
        "PLANNER_RESPONSE" => {
            planner(&mut fact, &meta, &record, step_index, &status, line, work)?;
        }
        "ERROR_MESSAGE" => fact.push("error", &meta, json!({"message": record.text("error")})),
        "SYSTEM_MESSAGE" | "CHECKPOINT" => fact.push(
            "system_message",
            &meta,
            json!({
                "kind": if kind == "CHECKPOINT" { "compaction" } else { "system" },
                "text": record.text("content"),
            }),
        ),
        "EPHEMERAL_MESSAGE" | "CONVERSATION_HISTORY" | "DIRECTORY_RULES" => {
            fact.ignored(&meta, &kind, line);
        }
        "GENERIC" | "VIEW_FILE" | "RUN_COMMAND" | "GREP_SEARCH" | "CODE_ACTION"
        | "LIST_DIRECTORY" | "READ_URL_CONTENT" | "SEARCH_WEB" | "MCP_TOOL" | "INVOKE_SUBAGENT" => {
            let Some(content) = record.optional_text("content") else {
                fact.unknown(&meta, line);
                return Ok(fact);
            };
            let state = match status.as_str() {
                "DONE" => "completed",
                "RUNNING" => "pending",
                "ERROR" | "INVALID" => "failed",
                _ => {
                    fact.unknown(&meta, line);
                    return Ok(fact);
                }
            };
            fact.push(
                "tool_call_update",
                &meta,
                json!({
                    "tool_call_id": step_index.to_string(),
                    "status": state, "content": content,
                }),
            );
        }
        _ => fact.unknown(&meta, line),
    }
    Ok(fact)
}

fn user_input(fact: &mut Fact, meta: &Option<Meta>, content: &str) {
    let mut text = content;
    let mut context = "";
    let leading = text.trim_start_matches([' ', '\t', '\r', '\n']);
    if let Some(request) = leading.strip_prefix("<USER_REQUEST>") {
        if let Some(end) = request.rfind("</USER_REQUEST>") {
            text = request[..end].trim_matches([' ', '\t', '\r', '\n']);
            context =
                request[end + "</USER_REQUEST>".len()..].trim_matches([' ', '\t', '\r', '\n']);
        } else {
            text = request.trim_matches([' ', '\t', '\r', '\n']);
        }
    }
    if ring(text) {
        fact.push(
            "system_message",
            meta,
            json!({"kind": "swarm_ring", "text": text}),
        );
    } else {
        fact.push("user_message_chunk", meta, json!({"text": text}));
    }
    if !context.is_empty() {
        fact.push(
            "system_message",
            meta,
            json!({"kind": "context", "text": context}),
        );
    }
}

fn planner(
    fact: &mut Fact,
    meta: &Option<Meta>,
    record: &Object<'_>,
    step_index: i64,
    status: &str,
    line: &[u8],
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let mut has_unknown = false;
    let mut replied = false;
    let mut has_tool_call = false;
    for (field, event_kind) in [
        ("thinking", "agent_thought_chunk"),
        ("content", "agent_message_chunk"),
    ] {
        work.check()?;
        if record.non_null(field).is_some() {
            if let Some(text) = record.optional_text(field) {
                if !text.is_empty() {
                    fact.push(event_kind, meta, json!({"text": text}));
                    replied |= field == "content";
                }
            } else {
                has_unknown = true;
            }
        }
    }
    if let Some(raw) = record.non_null("tool_calls") {
        if let Some(calls) = claude_json::array(raw) {
            for (index, call) in calls.iter().enumerate() {
                work.check()?;
                let Some(call) = Object::parse(call) else {
                    has_unknown = true;
                    continue;
                };
                let (Some(name), Some(input)) = (call.optional_text("name"), call.get("args"))
                else {
                    has_unknown = true;
                    continue;
                };
                if !input.get().starts_with('{') {
                    has_unknown = true;
                    continue;
                }
                let Some(input) = claude_input::parse(input, 255, work)? else {
                    has_unknown = true;
                    continue;
                };
                let call_id = index
                    .checked_add(1)
                    .and_then(|number| i64::try_from(number).ok())
                    .and_then(|offset| step_index.checked_add(offset));
                let Some(call_id) = call_id else {
                    has_unknown = true;
                    continue;
                };
                fact.events.push(Template {
                    kind: "tool_call",
                    fields: json!({
                        "tool_call_id": call_id.to_string(), "name": name,
                        "status": "pending",
                    }),
                    meta: meta.clone(),
                    input: Some(input),
                });
                has_tool_call = true;
            }
        } else {
            has_unknown = true;
        }
    }
    if has_unknown {
        fact.unknown(meta, line);
    } else if fact.events.is_empty() {
        fact.ignored(meta, "PLANNER_RESPONSE", line);
    } else if replied && !has_tool_call && status == "DONE" {
        fact.push(
            "turn_ended",
            meta,
            json!({"duration_ms": null, "reason": "completed"}),
        );
    }
    Ok(())
}
