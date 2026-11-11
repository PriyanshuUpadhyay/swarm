//! Materialized Codex items also occur beside their response-item copies in legacy logs.

use super::claude_input::{self, Input};
use super::claude_json::Object;
use super::codex_facts::{Action, Fact, Template};
use super::{SnapshotError, Work};
use serde_json::{Value, json};

pub(super) fn append(
    fact: &mut Fact<'_>,
    payload: &Object<'_>,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let Some(item) = payload.object("item") else {
        fact.push_unknown();
        return Ok(());
    };
    let start = fact.actions.len();
    let id = item.text("id");
    fact.meta["uuid"] = json!(id);
    match item.text("type").as_str() {
        "Plan" => fact.push("proposed_plan", json!({"text":item.text("text")})),
        "UserMessage" | "AgentMessage" => {
            let user = item.text("type") == "UserMessage";
            if let Some(parts) = item.array("content") {
                for part in parts {
                    work.check()?;
                    if let Some(part) = Object::parse(part) {
                        match part.text("type").as_str() {
                            "text" | "Text" => fact.push(if user {"user_message_chunk"} else {"agent_message_chunk"}, json!({"text":part.text("text")})),
                            "image" | "local_image" => fact.push("image", json!({"role":"user", "media_type":"image", "text":part.text("path")})),
                            _ => fact.push_unknown(),
                        }
                    } else {
                        fact.push_unknown();
                    }
                }
            }
        }
        "Reasoning" => {
            let text = strings(&item, "summary_text");
            let text = if text.is_empty() {
                strings(&item, "raw_content")
            } else {
                text
            };
            if text.is_empty() {
                fact.push("agent_thought_chunk", json!({"text":""}));
            }
            for text in text {
                fact.push("agent_thought_chunk", json!({"text":text}));
            }
        }
        "CommandExecution" => {
            let command = strings(&item, "command").join(" ");
            call(
                fact,
                &id,
                "exec_command",
                Input::Scalar(json!({"cmd":command, "cwd":item.text("cwd")})),
            );
            output(fact, &item, &id, item.text("aggregated_output"));
        }
        "FunctionCallOutput" => {
            let content = item.optional_text("output").unwrap_or_else(|| {
                item.get("output")
                    .map_or(String::new(), |raw| raw.get().into())
            });
            output(fact, &item, &id, content);
        }
        "FileChange" => super::codex_changes::append(fact, &item, "id", work)?,
        "SubAgentActivity" => super::codex_facts::subagent_activity(fact, &item),
        "McpToolCall" | "DynamicToolCall" => {
            let name = item.text("tool");
            let input = match item.get("arguments") {
                Some(raw) => claude_input::parse(raw, 255, work)?,
                None => Some(Input::Scalar(Value::Null)),
            };
            if let Some(input) = input {
                call(fact, &id, &name, input);
            } else {
                fact.push_unknown();
            }
            let content = item
                .get("result")
                .or_else(|| item.get("content_items"))
                .map_or_else(
                    || {
                        item.object("error")
                            .map_or_else(|| item.text("error"), |error| error.text("message"))
                    },
                    |raw| raw.get().into(),
                );
            output(fact, &item, &id, content);
        }
        "CollabAgentToolCall" => {
            let name = item.text("tool");
            call(
                fact,
                &id,
                &name,
                Input::Scalar(json!({"message":item.text("prompt")})),
            );
            let agent_id = strings(&item, "receiver_thread_ids").into_iter().next();
            let content = item
                .get("agents_states")
                .map_or(String::new(), |raw| raw.get().into());
            fact.push("tool_call_update", json!({"tool_call_id":id,"status":status(&item),"content":content,"agent_thread_id":agent_id}));
        }
        "WebSearch" => {
            let input = match item.get("action") {
                Some(raw) => claude_input::parse(raw, 255, work)?,
                None => Some(Input::Scalar(json!({"query":item.text("query")}))),
            };
            if let Some(input) = input {
                call(fact, &id, "web_search", input);
            } else {
                fact.push_unknown();
            }
            output(
                fact,
                &item,
                &id,
                item.get("results")
                    .map_or(String::new(), |raw| raw.get().into()),
            );
        }
        "EnteredReviewMode" | "ExitedReviewMode" => {
            let text = if item.text("type") == "ExitedReviewMode" {
                "Exited review mode".into()
            } else if item.text("user_facing_hint").is_empty() {
                "Entered review mode".into()
            } else {
                format!("Entered review mode: {}", item.text("user_facing_hint"))
            };
            fact.push("system_message", json!({"kind":"review_mode","text":text}));
        }
        "HookPrompt" => {
            let mut text = Vec::new();
            for raw in item.array("fragments").unwrap_or_default() {
                work.check()?;
                if let Some(part) = Object::parse(raw) {
                    text.push(part.text("text"));
                }
            }
            fact.push(
                "system_message",
                json!({"kind":"hook_prompt","text":text.join("\n")}),
            );
        }
        "ImageGeneration" | "ImageView" => {
            let text = item
                .optional_text("saved_path")
                .or_else(|| item.optional_text("path"))
                .unwrap_or_default();
            fact.push(
                "image",
                json!({"role":"agent","media_type":"image","text":text}),
            );
        }
        "ContextCompaction" => fact.push(
            "system_message",
            json!({"kind":"compaction","text":"Context compacted"}),
        ),
        // Extension schemas belong to their writers; unknown variants must stay counted.
        _ => fact.push_unknown(),
    }
    if fact.actions.len() == start {
        fact.push_unknown();
    }
    fact.mark_mirrors(start);
    Ok(())
}

fn strings(item: &Object<'_>, key: &str) -> Vec<String> {
    item.get(key)
        .and_then(|raw| serde_json::from_str(raw.get()).ok())
        .unwrap_or_default()
}

fn call(fact: &mut Fact<'_>, id: &str, name: &str, input: Input) {
    let mut event = Template::new(
        "tool_call",
        json!({"tool_call_id":id,"name":name,"status":"pending"}),
    );
    event.input = Some(input);
    fact.actions.push(Action::Event(event));
}

fn status(item: &Object<'_>) -> &'static str {
    if matches!(
        item.text("status").as_str(),
        "failed" | "declined" | "interrupted"
    ) {
        "failed"
    } else {
        "completed"
    }
}

fn output(fact: &mut Fact<'_>, item: &Object<'_>, id: &str, content: String) {
    let mut fields = json!({"tool_call_id":id,"status":status(item),"content":content});
    if let Some(code) = item.integer("exit_code") {
        fields["exit_code"] = json!(code);
    }
    fact.push("tool_call_update", fields);
}
