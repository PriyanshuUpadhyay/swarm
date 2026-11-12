//! Materialized Codex items also occur beside their response-item copies in legacy logs.

use super::claude_input::{self, Input};
use super::claude_json::Object;
use super::codex_facts::{Fact, status};
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
    fact.source_kind = format!("item_completed/{}", item.text("type"));

    let start = fact.actions.len();
    let id = item.text("id");
    fact.meta["uuid"] = json!(id);
    match item.text("type").as_str() {
        "Plan" => {
            let text = item.text("text");
            if text.is_empty() {
                fact.push_ignored("item_completed/Plan");
            } else {
                fact.push("proposed_plan", json!({"text":text}));
            }
        }

        "UserMessage" | "AgentMessage" => {
            let user = item.text("type") == "UserMessage";
            if let Some(parts) = item.array("content") {
                for part in parts {
                    work.check()?;
                    if let Some(part) = Object::parse(part) {
                        match part.text("type").as_str() {
                            "text" | "Text" => {
                                if user { super::codex_facts::user_text(fact,&part.text("text"),work)?; }
                                else { fact.push("agent_message_chunk",json!({"text":part.text("text")})); }
                            }
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
            let text = item.strings("summary_text").unwrap_or_default();
            let text = if text.is_empty() {
                item.strings("raw_content").unwrap_or_default()
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
            let Some(command) = item.command("command") else {
                fact.push_unknown();
                return Ok(());
            };
            fact.call(
                &id,
                "exec_command",
                "pending",
                Input::Scalar(json!({"cmd":command, "cwd":item.text("cwd")})),
            );
            output(fact, &item, &id, item.text("aggregated_output"));
        }

        "FunctionCallOutput" => {
            super::codex_tools::output(fact, &item, true, "id", work)?;
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
                fact.call(&id, &name, "pending", input);
            } else {
                fact.push_unknown();
            }

            let content = item
                .get("result")
                .filter(|raw| raw.get() != "null")
                .or_else(|| item.get("content_items").filter(|raw| raw.get() != "null"))
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
            fact.call(
                &id,
                &name,
                "pending",
                Input::Scalar(json!({"message":item.text("prompt")})),
            );

            let agent_id = item
                .strings("receiver_thread_ids")
                .unwrap_or_default()
                .into_iter()
                .next();
            let content = item
                .get("agents_states")
                .map_or(String::new(), |raw| raw.get().into());
            fact.push("tool_call_update", json!({"tool_call_id":id,"status":status(&item),"content":content,"agent_thread_id":agent_id}));
        }

        "WebSearch" => web_search(fact, &item, &id, work)?,
        "Extension" => match item.text("kind").as_str() {
            "clock.sleep" => {
                let duration = item.token("durationMs").or_else(|| {
                    payload
                        .token("completed_at_ms")?
                        .checked_sub(payload.token("started_at_ms")?)
                        .filter(|duration| *duration >= 0)
                });
                if let Some(duration) = duration {
                    let duration = crate::view::duration_label(duration as f64 / 1000.0);
                    fact.push(
                        "system_message",
                        json!({"kind":"sleep","text":format!("Waited {duration}")}),
                    );
                } else {
                    fact.push_unknown();
                }
            }

            "web.search" => web_search(fact, &item, &id, work)?,
            "image_gen.generation" => super::codex_facts::image_generation(fact, &item, "id"),
            _ => fact.push_unknown(),
        },
        "EnteredReviewMode" | "ExitedReviewMode" => {
            fact.review_mode(
                item.text("type") == "EnteredReviewMode",
                &item.text("user_facing_hint"),
            );
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

        "ImageGeneration" => super::codex_facts::image_generation(fact, &item, "id"),
        "ImageView" => {
            let text = item
                .optional_text("saved_path")
                .or_else(|| item.optional_text("path"))
                .unwrap_or_default();
            fact.push(
                "image",
                json!({"role":"agent","media_type":"image","text":text}),
            );
        }

        "ContextCompaction" => fact.compaction(false),
        _ => fact.push_unknown(),
    }
    if fact.actions.len() == start {
        fact.push_unknown();
    }
    fact.mark_mirrors(start);

    Ok(())
}

fn web_search(
    fact: &mut Fact<'_>,
    item: &Object<'_>,
    id: &str,
    work: &mut dyn Work,
) -> Result<(), SnapshotError> {
    let input = match item.get("action").filter(|raw| raw.get() != "null") {
        Some(raw) => claude_input::parse(raw, 255, work)?,
        None => Some(Input::Scalar(json!({"query":item.text("query")}))),
    };
    if let Some(input) = input {
        fact.call(id, "web_search", "pending", input);
    } else {
        fact.push_unknown();
    }
    output(
        fact,
        item,
        id,
        item.get("results")
            .map_or(String::new(), |raw| raw.get().into()),
    );

    Ok(())
}

fn output(fact: &mut Fact<'_>, item: &Object<'_>, id: &str, content: String) {
    let mut fields = json!({"tool_call_id":id,"status":status(item),"content":content});
    if let Some(code) = item.integer("exit_code") {
        fields["exit_code"] = json!(code);
    }
    fact.push("tool_call_update", fields);
}
