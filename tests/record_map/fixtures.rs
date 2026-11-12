use super::mapping::{End, RECORD_MAP};
use serde_json::{Value, json};

use crate::common::{completed as item, event, response};

fn codex_call(name: &str) -> Value {
    response(
        json!({"type":"function_call","call_id":"call","name":name,"arguments":"{\"cmd\":\"pwd\",\"message\":\"Check it\"}"}),
    )
}

fn codex_message(role: &str, part: &str) -> Value {
    response(
        json!({"type":"message","id":"message","role":role,"content":[{"type":part,"text":"Message","media_type":"image/png"}]}),
    )
}

fn spawn() -> Vec<Value> {
    vec![
        codex_call("spawn_agent"),
        response(
            json!({"type":"function_call_output","call_id":"call","output":"{\"agent_id\":\"child\"}"}),
        ),
    ]
}

fn patch(change: &str) -> Value {
    let value = match change {
        "add" => json!({"type":"add","content":"new\n"}),
        "delete" => json!({"type":"delete","content":"old\n"}),
        _ => json!({"type":"update","unified_diff":"@@ -1 +1 @@\n-old\n+new\n","move_path":null}),
    };
    json!({"file.txt":value})
}

fn agy(kind: &str, step: u64, status: &str) -> Value {
    json!({"type":kind,"step_index":step,"created_at":"2026-11-11T12:00:00Z","status":status,"source":"agent","content":"Message","error":"Error"})
}

fn claude_tool() -> Value {
    json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"call","name":"Read","input":{"file_path":"file.txt"}}]}})
}

fn pi_tool() -> Value {
    json!({"type":"message","id":"call-row","message":{"role":"assistant","content":[{"type":"toolCall","id":"call","name":"read","arguments":{"path":"file.txt"}}]}})
}

pub(super) fn fixture(cli: &str, kind: &str) -> Vec<Value> {
    let (group, name) = kind.split_once('/').unwrap_or(("", kind));
    match cli {
        "claude" => match group {
            "attachment" => vec![
                json!({"type":"attachment","attachment":{"type":name,"hookName":"SessionStart","decision":"allow","prompt":"Message","origin":{"kind":"human"},"identity":{"modelId":"model"}}}),
            ],

            "system" => vec![
                json!({"type":"system","subtype":name,"content":"Message","durationMs":20,"error":{"message":"Error"}}),
            ],
            "block" => {
                let mut lines = if name == "tool_result" {
                    vec![claude_tool()]
                } else {
                    vec![]
                };
                lines.push(json!({"type":if name=="tool_result" {"user"} else {"assistant"},"message":{"content":[{
                    "type":name,"text":"Message","thinking":"Thought","source":{"media_type":"image/png"},
                    "id":"call","name":"Read","input":{"file_path":"file.txt"},"tool_use_id":"call","content":"Result",
                    "from":{"model":"old"},"to":{"model":"new"}
                }]}}));
                lines
            }
            _ => vec![match name {
                "user" => json!({"type":name,"message":{"content":"Message"}}),
                "assistant" => {
                    json!({"type":name,"message":{"content":[{"type":"text","text":"Message"}]}})
                }
                "attachment" => {
                    json!({"type":name,"attachment":{"type":"hook_cancelled","hookName":"SessionStart"}})
                }
                "system" => json!({"type":name,"subtype":"informational","content":"Message"}),
                "ai-title" => json!({"type":name,"aiTitle":"Title"}),
                "custom-title" => json!({"type":name,"customTitle":"Title"}),
                "queue-operation" => {
                    json!({"type":name,"operation":"enqueue","content":"Queued message"})
                }
                "session-link" => json!({"sid":"s","from":"a","to":"b","ts":"time"}),
                _ => json!({"type":name}),
            }],
        },

        "codex" => match group {
            "response_item" => match name {
                "message" => vec![codex_message("user", "input_text")],
                "reasoning" => vec![response(
                    json!({"type":name,"id":"reason","summary":[{"type":"summary_text","text":"Thought"}]}),
                )],
                "agent_message" => vec![response(
                    json!({"type":name,"content":[{"type":"input_text","text":"Message"}]}),
                )],
                "function_call" => vec![codex_call("exec_command")],
                "custom_tool_call" => vec![response(
                    json!({"type":name,"call_id":"call","name":"apply_patch","input":"patch"}),
                )],
                "function_call_output" | "custom_tool_call_output" => vec![
                    codex_call("exec_command"),
                    response(json!({"type":name,"call_id":"call","output":"Result"})),
                ],
                "tool_search_output" => vec![
                    response(json!({"type":"tool_search_call","call_id":"call","arguments":{}})),
                    response(json!({"type":name,"call_id":"call","tools":[{"name":"read"}]})),
                ],
                "compaction" | "compaction_summary" | "context_compaction" => vec![
                    json!({"type":"compacted","payload":{"message":"Summary"}}),
                    response(
                        json!({"type":name,"id":"compaction","encrypted_content":"encrypted"}),
                    ),
                ],
                "local_shell_call" => vec![response(
                    json!({"type":name,"call_id":"call","status":"completed","action":{"type":"exec","command":["pwd"]}}),
                )],
                _ => vec![response(
                    json!({"type":name,"id":"image","call_id":"call","status":"completed","action":{"query":"rust"},"arguments":{},"result":"image"}),
                )],
            },

            "message" => vec![
                if matches!(name, "user" | "assistant" | "developer" | "system") {
                    codex_message(
                        name,
                        if name == "assistant" {
                            "output_text"
                        } else {
                            "input_text"
                        },
                    )
                } else {
                    codex_message("user", name)
                },
            ],

            "local_shell" => fixture("codex", "response_item/local_shell_call"),
            "file_change" => vec![
                codex_call("apply_patch"),
                event(
                    json!({"type":"patch_apply_end","call_id":"call","status":"completed","changes":patch(name)}),
                ),
            ],

            "item_content" => vec![item(
                json!({"type":if name=="Text" {"AgentMessage"} else {"UserMessage"},"id":"message","content":[{"type":name,"text":"Message","path":"/tmp/image.png"}]}),
            )],
            "item_completed" => {
                if let Some(kind) = name.strip_prefix("Extension/") {
                    return vec![item(json!({
                        "type":"Extension","kind":kind,"id":"extension","durationMs":1000,
                        "query":"rust","action":null,"results":[{"title":"Rust","url":"https://www.rust-lang.org/"}],
                        "status":"completed","result":"cG5n","savedPath":"/tmp/image.png"
                    }))];
                }
                let mut lines = match name {
                    "UserMessage" => vec![codex_message("user", "input_text")],
                    "AgentMessage" => vec![codex_message("assistant", "output_text")],
                    "Reasoning" => fixture("codex", "response_item/reasoning"),
                    "SubAgentActivity" => spawn(),
                    "CommandExecution" | "FunctionCallOutput" => vec![codex_call("exec_command")],
                    "FileChange" => vec![codex_call("apply_patch")],
                    "McpToolCall" | "DynamicToolCall" => vec![codex_call("read")],
                    "CollabAgentToolCall" => vec![codex_call("spawn_agent")],
                    "WebSearch" => vec![codex_call("web_search")],
                    _ => vec![],
                };
                let id = match name {
                    "UserMessage" | "AgentMessage" => "message",
                    "Reasoning" => "reason",
                    _ => "call",
                };
                lines.push(item(json!({"type":name,"id":id,"text":"Plan","content":[{"type":if name=="UserMessage" {"text"} else {"Text"},"text":"Message"}],
                    "summary_text":["Thought"],"raw_content":[],"command":["pwd"],"cwd":"/work","status":"completed","aggregated_output":"Result","exit_code":0,
                    "tool":if name=="CollabAgentToolCall" {"spawn_agent"} else if name=="WebSearch" {"web_search"} else {"read"},"arguments":{},"output":"Result","result":{"content":[{"type":"text","text":"Result"}]},
                    "receiver_thread_ids":["child"],"agent_thread_id":"child","agent_path":"/worker","kind":"completed","changes":patch("update"),
                    "user_facing_hint":"Review the code","fragments":[{"text":"Hook message","hookRunId":"hook"}],"path":"/tmp/image.png","saved_path":"/tmp/image.png","action":{"query":"rust"}
                })));
                lines
            }

            "event_msg" => match name {
                "agent_reasoning_raw_content" => vec![
                    fixture("codex", "response_item/reasoning").remove(0),
                    event(json!({"type":name,"text":"Raw thought"})),
                ],
                "sub_agent_activity" => {
                    let mut lines = spawn();
                    lines.push(event(json!({"type":name,"agent_thread_id":"child","agent_path":"/worker","kind":"completed"})));
                    lines
                }
                "patch_apply_end" => vec![
                    codex_call("apply_patch"),
                    event(
                        json!({"type":name,"call_id":"call","status":"completed","changes":patch("update")}),
                    ),
                ],
                "thread_rolled_back" => vec![
                    event(json!({"type":"task_started"})),
                    codex_message("user", "input_text"),
                    event(json!({"type":name,"num_turns":1})),
                ],
                "user_message" | "agent_message" => vec![
                    codex_message(
                        if name == "user_message" {
                            "user"
                        } else {
                            "assistant"
                        },
                        "input_text",
                    ),
                    event(json!({"type":name,"message":"Message"})),
                ],
                "exec_command_end" | "mcp_tool_call_end" | "web_search_end" => vec![
                    codex_call("exec_command"),
                    response(
                        json!({"type":"function_call_output","call_id":"call","output":"Result"}),
                    ),
                    event(json!({"type":name,"call_id":"call"})),
                ],
                "item_completed" => vec![item(json!({"type":"Plan","id":"plan","text":"Plan"}))],
                _ => vec![event(
                    json!({"type":name,"text":"Thought","message":"Message","call_id":"image","status":"completed","info":{"last_token_usage":{"total_tokens":20},"model_context_window":100}}),
                )],
            },
            _ => match name {
                "event_msg" => fixture("codex", "event_msg/agent_reasoning"),
                "response_item" => fixture("codex", "response_item/message"),
                "compacted" => vec![json!({"type":name,"payload":{"message":"Summary"}})],
                _ => vec![json!({"type":name,"payload":{"model":"model","cwd":"/work"}})],
            },
        },

        "pi" => match group {
            "block" => vec![
                json!({"type":"message","message":{"role":if name=="image" {"user"} else {"assistant"},"content":[{"type":name,"text":"Message","thinking":"Thought","mimeType":"image/png","id":"call","name":"read","arguments":{}}]}}),
            ],
            "message" => {
                let mut lines = if name == "toolResult" {
                    vec![pi_tool()]
                } else {
                    vec![]
                };
                lines.push(json!({"type":"message","id":"message","message":{"role":name,"content":[{"type":"text","text":"Message"}],"toolCallId":"call","command":"pwd","output":"/work","exitCode":0}}));
                lines
            }
            _ => vec![if name == "message" {
                json!({"type":name,"id":"message","message":{"role":"user","content":"Message"}})
            } else {
                json!({"type":name,"id":"entry","cwd":"/work","modelId":"model","name":"Title","summary":"Summary","content":"Message","display":true})
            }],
        },

        "agy" => {
            let name = if group == "status" { "GENERIC" } else { name };
            let mut lines = if RECORD_MAP.iter().any(|&(cli, kind, end)| {
                cli == "agy" && kind == name && matches!(end, End::Part(_))
            }) {
                let mut planner = agy("PLANNER_RESPONSE", 0, "RUNNING");
                planner["content"] = json!("");
                planner["tool_calls"] = json!([{"name":"read","args":{"path":"file.txt"}}]);
                vec![planner]
            } else {
                vec![]
            };
            lines.push(agy(
                name,
                if lines.is_empty() { 0 } else { 1 },
                if group == "status" {
                    kind.split('/').nth(1).unwrap()
                } else {
                    "DONE"
                },
            ));
            lines
        }
        _ => panic!("unknown CLI"),
    }
}
