//! Pi facts own one native line and need no reader history.

use super::claude_input::{self, Input};
use super::claude_json::{self, Object};
use super::pi_text;
use super::{NormalizedEvent, SnapshotError, Work};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Value, json, value::RawValue};

#[derive(Clone, Debug)]
pub(super) struct Meta {
    uuid: String,
    timestamp: String,
    pub(super) parent_uuid: Option<String>,
}

#[derive(Debug)]
struct Template {
    kind: &'static str,
    meta: Option<Meta>,
    fields: Value,
    input: Option<Input>,
}

#[derive(Debug)]
pub(crate) struct Fact {
    meta: Option<Meta>,
    events: Vec<Template>,
    token_overflow: bool,
}

impl Fact {
    pub(super) fn push(&mut self, kind: &'static str, fields: Value) {
        self.push_with_meta(kind, fields, self.meta.clone());
    }

    pub(super) fn push_with_meta(&mut self, kind: &'static str, fields: Value, meta: Option<Meta>) {
        self.events.push(Template {
            kind,
            meta,
            fields,
            input: None,
        });
    }

    pub(super) fn child_meta(&self) -> Option<Meta> {
        self.meta.clone().map(|mut meta| {
            meta.parent_uuid = Some(meta.uuid.clone());
            meta
        })
    }

    fn unknown(&mut self, line: &[u8]) {
        self.push("unknown", json!({"raw": String::from_utf8_lossy(line)}));
    }

    fn ignored(&mut self, kind: &str, line: &[u8]) {
        self.push(
            "ignored",
            json!({"kind": kind, "raw": String::from_utf8_lossy(line)}),
        );
    }

    pub(crate) fn project(
        &self,
        session_id: &str,
        work: &mut dyn Work,
    ) -> Result<Vec<NormalizedEvent>, SnapshotError> {
        work.check()?;
        // A mid-line reader can discard this fact without projecting its overflow.
        if self.token_overflow {
            return Err(SnapshotError::NativeTokenOverflow);
        }
        let mut events = Vec::with_capacity(self.events.len());
        for template in &self.events {
            work.check()?;
            events.push(NormalizedEvent::new(&Event {
                template,
                session_id,
            }));
        }
        Ok(events)
    }
}

struct Event<'a> {
    template: &'a Template,
    session_id: &'a str,
}

impl Serialize for Event<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct EventMeta<'a> {
            session_id: &'a str,
            uuid: &'a str,
            timestamp: &'a str,
            parent_uuid: &'a Option<String>,
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
        let meta = self.template.meta.as_ref().map(|meta| EventMeta {
            session_id: self.session_id,
            uuid: &meta.uuid,
            timestamp: &meta.timestamp,
            parent_uuid: &meta.parent_uuid,
            source_tool_use_id: None,
        });
        map.serialize_entry("meta", &meta)?;
        map.end()
    }
}

pub(crate) fn session_id_from_path(path: &str) -> String {
    let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    name.strip_suffix(".jsonl")
        .and_then(|stem| stem.rsplit_once('_'))
        .map(|(_, id)| id.to_owned())
        .unwrap_or_default()
}

pub(crate) fn parse_fact(line: &[u8], work: &mut dyn Work) -> Result<Fact, SnapshotError> {
    work.check()?;
    let mut fact = Fact {
        meta: None,
        events: Vec::new(),
        token_overflow: false,
    };
    let raw = std::str::from_utf8(line)
        .ok()
        .and_then(|text| serde_json::from_str::<&RawValue>(text).ok());
    let Some(raw) = raw else {
        fact.unknown(line);
        return Ok(fact);
    };
    if !claude_json::valid(raw, work)? {
        fact.unknown(line);
        return Ok(fact);
    }
    let Some(record) = Object::parse(raw) else {
        fact.unknown(line);
        return Ok(fact);
    };
    fact.meta = Some(Meta {
        uuid: record.text("id"),
        timestamp: record.text("timestamp"),
        parent_uuid: record.optional_text("parentId"),
    });
    let kind = record.text("type");
    match kind.as_str() {
        "session" | "model_change" | "session_info" => {
            let (kind, key) = match kind.as_str() {
                "session" => ("cwd", "cwd"),
                "model_change" => ("model", "modelId"),
                _ => ("title", "name"),
            };
            fact.push(
                "session_info",
                json!({"kind": kind, "value": record.text(key)}),
            );
        }
        "compaction" | "branch_summary" => fact.push(
            "system_message",
            json!({"kind": kind, "text": record.text("summary")}),
        ),
        "custom_message" => {
            let text = pi_text::join_text(record.get("content"), work)?;
            fact.push("system_message", json!({"kind": if record.boolean("display") == Some(true) { "custom_message" } else { "injected" }, "text": text}));
        }
        "thinking_level_change"
        | "custom"
        | "label"
        | "context_edit"
        | "usage"
        | "session_state"
        | "agent_status"
        | "git_state"
        | "child_usage_attribution"
        | "service_tier_change" => {
            fact.ignored(&kind, line);
        }
        "message" => {
            if let Some(message) = record.object("message") {
                parse_message(&mut fact, &message, line, work)?;
            } else {
                fact.unknown(line);
            }
        }
        _ => fact.unknown(line),
    }
    if fact.events.is_empty() {
        fact.unknown(line);
    }
    Ok(fact)
}

fn parse_message(
    fact: &mut Fact,
    message: &Object<'_>,
    line: &[u8],
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let role = message.text("role");
    let content = message.get("content");
    match role.as_str() {
        "user" => {
            let text = pi_text::join_text(content, work)?;
            if !text.is_empty() && !pi_text::append_skill(fact, &text, work)? {
                if super::claude_meta::ring(&text) {
                    fact.push(
                        "system_message",
                        json!({"kind": "swarm_ring", "text": text}),
                    );
                } else {
                    fact.push("user_message_chunk", json!({"text": text}));
                }
            }
            pi_text::append_images(fact, content, "user", work)?;
        }
        "assistant" => parse_assistant(fact, message, line, work)?,
        "toolResult" => {
            let text = pi_text::join_text(content, work)?;
            fact.push(
                "tool_call_update",
                json!({
                    "tool_call_id": message.text("toolCallId"),
                    "status": if message.boolean("isError") == Some(true) { "failed" } else { "completed" },
                    "content": text,
                }),
            );
            pi_text::append_images(fact, content, "tool", work)?;
        }
        "bashExecution" => {
            fact.push(
                "system_message",
                json!({"kind": "shell_input", "text": format!("<bash-input>{}</bash-input>", message.text("command"))}),
            );
            let output = message.text("output");
            let text = match message.integer("exitCode") {
                Some(code) => format!(
                    "<bash-stdout>{output}</bash-stdout><bash-exit-code>{code}</bash-exit-code>"
                ),
                None => format!("<bash-stdout>{output}</bash-stdout>"),
            };
            fact.push_with_meta(
                "system_message",
                json!({"kind": "shell_output", "text": text}),
                fact.child_meta(),
            );
        }
        "system" | "custom" | "branchSummary" | "compactionSummary" => {
            fact.ignored(&format!("message/{role}"), line);
        }
        _ => fact.unknown(line),
    }
    Ok(())
}

fn parse_assistant(
    fact: &mut Fact,
    message: &Object<'_>,
    line: &[u8],
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let model = message.text("model");
    if !model.is_empty() {
        fact.push("session_info", json!({"kind": "model", "value": model}));
    }
    if let Some(usage) = message.object("usage") {
        let input = usage.token("input");
        let read = usage.token("cacheRead");
        let write = usage.token("cacheWrite");
        let context = match (input, read, write) {
            (Some(input), Some(read), Some(write)) => {
                let sum = input
                    .checked_add(read)
                    .and_then(|sum| sum.checked_add(write));
                fact.token_overflow = sum.is_none();
                sum
            }
            _ => None,
        };
        fact.push(
            "usage",
            json!({
                "source": "pi", "kind": "context", "context_tokens": context,
                "context_capacity_tokens": null, "input_tokens": input,
                "output_tokens": usage.token("output"), "cache_read_tokens": read,
                "cache_write_tokens": write, "session_input_tokens": null,
                "session_output_tokens": null, "cost_usd": null,
                "cost_completeness": "unknown",
            }),
        );
    }
    let mut called_tool = false;
    let mut has_unknown = false;
    let mut wrote_text = false;
    if let Some(content) = message.array("content") {
        for raw in content {
            work.check()?;
            let Some(block) = Object::parse(raw) else {
                has_unknown = true;
                continue;
            };
            match block.text("type").as_str() {
                "text" => {
                    let text = block.text("text");
                    if !text.is_empty() {
                        let text = if wrote_text {
                            format!("\n\n{text}")
                        } else {
                            text
                        };
                        fact.push("agent_message_chunk", json!({"text": text}));
                        wrote_text = true;
                    }
                }
                "thinking" => {
                    let text = block.text("thinking");
                    fact.push("agent_thought_chunk", json!({"text": text}));
                }
                "toolCall" => {
                    let input = match block.get("arguments") {
                        Some(raw) => claude_input::parse(raw, 255, work)?,
                        None => Some(Input::Scalar(Value::Null)),
                    };
                    let Some(input) = input else {
                        has_unknown = true;
                        continue;
                    };
                    fact.events.push(Template {
                        kind: "tool_call",
                        meta: fact.meta.clone(),
                        fields: json!({"tool_call_id": block.text("id"), "name": block.text("name"), "status": "pending"}),
                        input: Some(input),
                    });
                    called_tool = true;
                }
                _ => has_unknown = true,
            }
        }
    } else if message
        .get("content")
        .is_some_and(|raw| raw.get() != "null")
    {
        has_unknown = true;
    }
    if has_unknown {
        fact.unknown(line);
    }
    let stop = message.text("stopReason");
    let error = message.text("errorMessage");
    let user_abort = stop == "error"
        && matches!(
            error.as_str(),
            "This operation was aborted" | "Operation aborted"
        );
    if stop == "error" && !user_abort {
        fact.push("error", json!({"message": error}));
        fact.push(
            "turn_ended",
            json!({"duration_ms": null, "reason": "aborted"}),
        );
    } else if user_abort || stop == "aborted" {
        fact.push(
            "turn_ended",
            json!({"duration_ms": null, "reason": "aborted"}),
        );
    } else if !called_tool && matches!(stop.as_str(), "stop" | "length") {
        fact.push(
            "turn_ended",
            json!({"duration_ms": null, "reason": "completed"}),
        );
    }
    Ok(())
}
