//! Normalizes all Claude record branches, without expanding unused source trees.

use super::claude_input::Input;
use super::claude_json::{self, Object};
use super::claude_meta::{event, ignored, meta, ring, text_event, text_kind, unknown, usage};
use super::{NormalizedEvent, SnapshotError, Work};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Value, json, value::RawValue};

pub(super) fn line(
    line: &[u8],
    work: &mut dyn Work,
) -> Result<Vec<NormalizedEvent>, SnapshotError> {
    work.check()?;
    let Ok(text) = std::str::from_utf8(line) else {
        return Ok(vec![unknown(line, &Value::Null)]);
    };
    let Ok(raw) = serde_json::from_str::<&RawValue>(text) else {
        return Ok(vec![unknown(line, &Value::Null)]);
    };
    if !claude_json::valid(raw, work)? {
        return Ok(vec![unknown(line, &Value::Null)]);
    }
    let Some(rec) = Object::parse(raw) else {
        return Ok(vec![unknown(line, &Value::Null)]);
    };
    let meta = meta(&rec);
    let kind = rec.text("type");
    let mut events = Vec::new();
    match kind.as_str() {
        "attachment" => events.push(attachment(&rec, &meta, line)),
        "ai-title" | "custom-title" => events.push(event("session_info", &meta, json!({
            "kind": "title", "value": rec.text(if kind == "ai-title" {"aiTitle"} else {"customTitle"}),
        }))),
        "agent-name" => events.push(event("session_info", &meta, json!({"kind":"agent_name", "value":rec.text("agentName")}))),
        "system" => events.push(system(&rec, &meta, line)),
        "cost-state" => events.push(cost(&rec, &meta)),
        "queue-operation" => events.push(event("queue_operation", &meta, json!({
            "operation": rec.text("operation"), "content": rec.optional_text("content"), "reason":rec.optional_text("reason"),
        }))),
        "last-prompt" | "atis-latch" | "mode" | "permission-mode" | "file-history-snapshot"
        | "file-history-delta" | "pr-link" | "bridge-session" | "frame-link" | "history-suppression"
        | "artifact-autoreact-ledger" | "artifact-comment-monitor" | "continued-in"
        | "worktree-state" | "relocated" | "api-request" | "api-request-blob" | "api-request-shape"
        | "tools-used" | "session-owner" => events.push(ignored(&kind, line, &meta)),
        "" if ["sid", "from", "to", "ts"].iter().all(|key| rec.get(key).is_some()) => events.push(ignored("session-link", line, &meta)),
        "user" | "assistant" => messages(&rec, &meta, kind == "user", line, &mut events, work)?,
        _ => events.push(unknown(line, &meta)),
    }
    if events.is_empty() {
        events.push(unknown(line, &meta));
    }
    Ok(events)
}

fn cost(rec: &Object<'_>, meta: &Value) -> NormalizedEvent {
    let mut value = usage(meta, "cost");
    value["cost_completeness"] = json!(match rec.boolean("hasUnknownModelCost") {
        Some(true) => "partial",
        Some(false) => "complete",
        None => "unknown",
    });
    let cost = Input::Scalar(json!(rec.nonnegative_number("totalCostUSD")));
    NormalizedEvent::new(&CostUsage {
        fields: value.as_object().expect("usage event is an object"),
        cost,
    })
}

struct CostUsage<'a> {
    fields: &'a serde_json::Map<String, Value>,
    cost: Input,
}

impl Serialize for CostUsage<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.fields.len()))?;
        for (key, value) in self.fields {
            if key == "cost_usd" {
                map.serialize_entry(key, &self.cost)?;
            } else {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}

fn attachment(rec: &Object<'_>, meta: &Value, line: &[u8]) -> NormalizedEvent {
    let Some(attachment) = rec.object("attachment") else {
        return unknown(line, meta);
    };
    let kind = attachment.text("type");
    match kind.as_str() {
        "hook_success" | "hook_non_blocking_error" | "hook_blocking_error" | "hook_cancelled" => {
            event(
                "hook_result",
                meta,
                json!({
                    "kind":kind, "hook_event":attachment.text("hookEvent"), "hook_name":attachment.text("hookName"),
                    "tool_call_id":attachment.text("toolUseID"), "exit_code":attachment.integer("exitCode"),
                }),
            )
        }
        "hook_permission_decision" => event(
            "permission_decision",
            meta,
            json!({
                "hook_event":attachment.text("hookEvent"), "tool_call_id":attachment.text("toolUseID"), "decision":attachment.text("decision"),
            }),
        ),
        "queued_command" => {
            let human = attachment
                .object("origin")
                .is_some_and(|origin| origin.text("kind") == "human");
            let prompt = attachment.text("prompt");
            event(
                "system_message",
                meta,
                json!({"kind":if ring(&prompt) {"swarm_ring"} else if human {"queued_prompt"} else {&kind}, "text":prompt}),
            )
        }
        "model" => event(
            "session_info",
            meta,
            json!({"kind":"model", "value":attachment.object("identity").map_or(String::new(), |obj| obj.text("modelId"))}),
        ),
        "total_tokens_reminder"
        | "hook_additional_context"
        | "sandbox_instructions"
        | "bash_output_audience_note"
        | "batching_reminder_sent"
        | "silent_turn_reminder"
        | "skill_listing"
        | "deferred_tools_delta"
        | "mcp_instructions_delta"
        | "prompt_snapshot"
        | "edited_text_file"
        | "agent_listing_delta"
        | "auto_mode"
        | "date"
        | "session_context"
        | "instructions"
        | "environment"
        | "deferred_tools_record"
        | "command_permissions"
        | "diagnostics"
        | "file"
        | "remote_session_change"
        | "nested_memory"
        | "task_reminder"
        | "compact_file_reference"
        | "invoked_skills"
        | "date_change"
        | "dynamic_skill"
        | "task_status"
        | "plan_mode_exit"
        | "thinking_stripped"
        | "read_truncation_notice"
        | "plan_mode"
        | "credential_org"
        | "thinking_drop"
        | "skill_mention" => ignored(&format!("attachment/{kind}"), line, meta),
        _ => unknown(line, meta),
    }
}

fn system(rec: &Object<'_>, meta: &Value, line: &[u8]) -> NormalizedEvent {
    let kind = rec.text("subtype");
    match kind.as_str() {
        "turn_duration" => event(
            "turn_ended",
            meta,
            json!({"duration_ms":rec.integer("durationMs"), "reason":"completed"}),
        ),
        "compact_boundary"
        | "away_summary"
        | "informational"
        | "local_command"
        | "scheduled_task_fire"
        | "stop_hook_summary"
        | "model_refusal_fallback" => {
            let content = rec.text("content");
            let command = if kind == "local_command" {
                text_kind(rec, &content)
            } else {
                ""
            };
            let label = if matches!(command, "command" | "command_output") {
                command
            } else if kind == "compact_boundary" {
                "compaction"
            } else if kind == "model_refusal_fallback" {
                "model_fallback"
            } else {
                &kind
            };
            event(
                "system_message",
                meta,
                json!({"kind":label, "text":if content.is_empty() {rec.text("summary")} else {content}}),
            )
        }
        "api_error" => {
            let error = rec.object("error");
            let formatted = error
                .as_ref()
                .map_or(String::new(), |error| error.text("formatted"));
            let mut message = if formatted.is_empty() {
                error
                    .as_ref()
                    .map_or(String::new(), |error| error.text("message"))
            } else {
                formatted
            };
            if let (Some(attempt), Some(max)) =
                (rec.integer("retryAttempt"), rec.integer("maxRetries"))
            {
                message = format!("{message} (retry {attempt} of {max})");
            }
            event("error", meta, json!({"message":message}))
        }
        _ => unknown(line, meta),
    }
}

fn messages(
    rec: &Object<'_>,
    meta: &Value,
    is_user: bool,
    line: &[u8],
    events: &mut Vec<NormalizedEvent>,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let Some(message) = rec.object("message") else {
        return Ok(());
    };
    if !is_user {
        let model = message.text("model");
        if !model.is_empty() && model != "<synthetic>" {
            events.push(event(
                "session_info",
                meta,
                json!({"kind":"model", "value":model}),
            ));
        }
        if model != "<synthetic>"
            && let Some(tokens) = message.object("usage")
        {
            let input = tokens
                .token("input_tokens")
                .zip(tokens.token("cache_read_input_tokens"))
                .zip(tokens.token("cache_creation_input_tokens"))
                .and_then(|((input, read), write)| input.checked_add(read)?.checked_add(write));
            let mut value = usage(meta, "context");
            value["context_tokens"] = json!(input);
            value["input_tokens"] = json!(input);
            value["output_tokens"] = json!(tokens.token("output_tokens"));
            value["cache_read_tokens"] = json!(tokens.token("cache_read_input_tokens"));
            value["cache_write_tokens"] = json!(tokens.token("cache_creation_input_tokens"));
            events.push(NormalizedEvent::new(&value));
        }
    }
    if is_user && let Some(content) = message.optional_text("content") {
        events.push(text_event(meta, rec, true, &content));
    }
    let Some(content) = message.array("content") else {
        return Ok(());
    };
    let mut count = 0;
    for raw in &content {
        work.check()?;
        if Object::parse(raw).is_some_and(|block| block.text("type") == "tool_result") {
            count += 1;
        }
    }
    let mut has_unknown = false;
    for raw in content {
        work.check()?;
        let Some(block) = Object::parse(raw) else {
            if !has_unknown {
                events.push(unknown(line, meta));
                has_unknown = true;
            }
            continue;
        };
        let kind = block.text("type");
        match kind.as_str() {
            "text" => events.push(text_event(meta, rec, is_user, &block.text("text"))),
            "image" => events.push(event("image", meta, json!({"role":if is_user {"user"} else {"agent"}, "media_type":block.object("source").map_or(String::new(), |source| source.text("media_type"))}))),
            "fallback" if !is_user => events.push(event("system_message", meta, json!({"kind":"model_fallback", "text":format!("{} -> {}", block.object("from").map_or(String::new(), |from| from.text("model")), block.object("to").map_or(String::new(), |to| to.text("model")))}))),
            "thinking" => {
                let thought = block.text("thinking");
                events.push(event("agent_thought_chunk", meta, json!({"text":thought})));
            }
            "tool_use" if !is_user => super::claude_tools::call(&block, meta, line, events, &mut has_unknown, work)?,
            "tool_result" if is_user => super::claude_tools::result(&block, rec, meta, line, count, events, &mut has_unknown, work)?,
            _ if !has_unknown => { events.push(unknown(line, meta)); has_unknown = true; }
            _ => (),
        }
    }
    Ok(())
}
