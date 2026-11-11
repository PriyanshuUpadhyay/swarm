//! Main paths copied from OLD Zig expectations; each row names its source.
use super::*;

fn check(format: &str, cases: &[(&str, &str, &str, Value)]) {
    for (name, original, input, expected) in cases {
        let actual = json!(translated(format, Path::new(""), input.as_bytes()));
        for (pointer, value) in expected.as_object().unwrap() {
            assert_eq!(
                actual.pointer(pointer),
                Some(value),
                "{name}: {original}: {pointer}"
            );
        }
    }
}

#[test]
fn claude_message_main() {
    check(
        "claude",
        &[
            (
                "assistant text and thinking blocks become two chunks",
                "packages/transcript/src/claude_message_tests.zig:18",
                r###"{"type":"assistant","sessionId":"s1","uuid":"u2","timestamp":"t","message":{"content":[{"type":"thinking","thinking":"plan"},{"type":"text","text":"done"}]}}"###,
                json!({"/0/type":"agent_thought_chunk","/0/text":"plan","/1/type":"agent_message_chunk","/1/text":"done"}),
            ),
            (
                "user text survives an image block",
                "packages/transcript/src/claude_message_tests.zig:81",
                r###"{"type":"user","sessionId":"s1","message":{"content":[{"type":"text","text":"first"},{"type":"image","source":"x"},{"type":"text","text":"last"}]}}"###,
                json!({"/0/text":"first","/1/role":"user","/2/text":"last"}),
            ),
            (
                "tools-used",
                "packages/transcript/src/claude_message_tests.zig:129",
                r###"{"type":"tools-used","tools":["Bash"],"sessionId":"s1"}"###,
                json!({"/0/kind":"tools-used"}),
            ),
            (
                "session-owner",
                "packages/transcript/src/claude_message_tests.zig:129",
                r###"{"type":"session-owner","accountUuid":"a","organizationUuid":"o","sessionId":"s1"}"###,
                json!({"/0/kind":"session-owner"}),
            ),
            (
                "attachment/skill_mention",
                "packages/transcript/src/claude_message_tests.zig:129",
                r###"{"type":"attachment","uuid":"u1","attachment":{"type":"skill_mention","skillName":"swarm:research"}}"###,
                json!({"/0/kind":"attachment/skill_mention"}),
            ),
            (
                "Claude 2.1.300 request records and dropped thinking are ignored",
                "packages/transcript/src/claude_message_tests.zig:116",
                r###"{"type":"attachment","attachment":{"type":"thinking_drop","thinkingBlocksSent":1}}"###,
                json!({"/0/kind":"attachment/thinking_drop"}),
            ),
            (
                "Claude context includes cache while cost snapshots remain separate",
                "packages/transcript/src/claude_message_tests.zig:154",
                r###"{"type":"assistant","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":80,"cache_creation_input_tokens":20,"output_tokens":7},"content":[]}}"###,
                json!({"/0/context_tokens":110,"/0/output_tokens":7,"/0/context_capacity_tokens":null}),
            ),
            (
                "Claude API error record becomes an error with its retry count",
                "packages/transcript/src/claude_message_tests.zig:199",
                r###"{"type":"system","subtype":"api_error","level":"error","error":{"message":"Connection error.","formatted":"Connection lost while your computer was asleep"},"retryInMs":519,"retryAttempt":1,"maxRetries":10,"uuid":"u1"}"###,
                json!({"/0/message":"Connection lost while your computer was asleep (retry 1 of 10)"}),
            ),
        ],
    );
}

#[test]
fn claude_context_main() {
    check(
        "claude",
        &[
            (
                "Claude background task notification is system text, not a user prompt",
                "packages/transcript/src/claude_context_tests.zig:59",
                r###"{"type":"user","message":{"content":"<task-notification>\n<status>completed</status>\n<summary>Background command done</summary>\n</task-notification>"}}"###,
                json!({"/0/kind":"task_notification"}),
            ),
            (
                "title",
                "packages/transcript/src/claude_context_tests.zig:9",
                r###"{"type":"ai-title","aiTitle":"Title"}"###,
                json!({"/0/kind":"title","/0/value":"Title"}),
            ),
            (
                "agent_name",
                "packages/transcript/src/claude_context_tests.zig:9",
                r###"{"type":"agent-name","agentName":"Agent"}"###,
                json!({"/0/kind":"agent_name","/0/value":"Agent"}),
            ),
            (
                "model",
                "packages/transcript/src/claude_context_tests.zig:9",
                r###"{"type":"attachment","attachment":{"type":"model","identity":{"modelId":"model-1"}}}"###,
                json!({"/0/kind":"model","/0/value":"model-1"}),
            ),
            (
                "Claude system records become system messages and turn end",
                "packages/transcript/src/claude_context_tests.zig:23",
                r###"{"type":"system","subtype":"compact_boundary","content":"summary"}"###,
                json!({"/0/kind":"compaction"}),
            ),
            (
                "Claude system records become system messages and turn end",
                "packages/transcript/src/claude_context_tests.zig:23",
                r###"{"type":"system","subtype":"model_refusal_fallback","content":"retry"}"###,
                json!({"/0/text":"retry"}),
            ),
            (
                "command_output",
                "packages/transcript/src/claude_context_tests.zig:34",
                r###"{"type":"system","subtype":"local_command","parentUuid":"command","content":"<local-command-stdout>Usage</local-command-stdout>"}"###,
                json!({"/0/kind":"command_output"}),
            ),
            (
                "command",
                "packages/transcript/src/claude_context_tests.zig:34",
                r###"{"type":"system","subtype":"local_command","content":"<command-name>/skills</command-name>"}"###,
                json!({"/0/kind":"command"}),
            ),
            (
                "local_command",
                "packages/transcript/src/claude_context_tests.zig:34",
                r###"{"type":"system","subtype":"local_command","content":"plain"}"###,
                json!({"/0/kind":"local_command"}),
            ),
            (
                "injected",
                "packages/transcript/src/claude_context_tests.zig:34",
                r###"{"type":"user","isMeta":true,"message":{"content":"<local-command-caveat>Caveat: do not respond</local-command-caveat>"}}"###,
                json!({"/0/kind":"injected"}),
            ),
            (
                "Claude command wrappers and compact summary are system text",
                "packages/transcript/src/claude_context_tests.zig:48",
                r###"{"type":"user","isCompactSummary":true,"message":{"content":"summary"}}"###,
                json!({"/0/kind":"compact_summary"}),
            ),
            (
                "Claude shell mode input is shell_input",
                "packages/transcript/src/claude_context_tests.zig:66",
                r###"{"type":"user","message":{"content":"<bash-input>ls</bash-input>"}}"###,
                json!({"/0/kind":"shell_input"}),
            ),
            (
                "Claude shell mode stdout and stderr are shell_output",
                "packages/transcript/src/claude_context_tests.zig:73",
                r###"{"type":"user","message":{"content":"<bash-stdout>out</bash-stdout><bash-stderr></bash-stderr>"}}"###,
                json!({"/0/kind":"shell_output"}),
            ),
            (
                "Claude meta skill body is skill_body, a typed one is a user chunk",
                "packages/transcript/src/claude_context_tests.zig:82",
                r###"{"type":"user","isMeta":true,"message":{"content":[{"type":"text","text":"Base directory for this skill: /skills/flow\n\nbody"}]}}"###,
                json!({"/0/kind":"skill_body"}),
            ),
            (
                "Claude interrupt notice is interrupted",
                "packages/transcript/src/claude_context_tests.zig:91",
                r###"{"type":"user","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}"###,
                json!({"/0/kind":"interrupted"}),
            ),
            (
                "Claude meta text, a reminder, and fork rules are injected",
                "packages/transcript/src/claude_context_tests.zig:100",
                r###"{"type":"user","isMeta":true,"message":{"content":"Stop hook feedback: fix it"}}"###,
                json!({"/0/kind":"injected"}),
            ),
            (
                "Claude message from another agent is peer_message, also when Claude marks it meta",
                "packages/transcript/src/claude_context_tests.zig:113",
                r###"{"type":"user","isMeta":true,"origin":{"kind":"peer"},"message":{"content":"Another Claude session sent a message:\n<agent-message>hi</agent-message>"}}"###,
                json!({"/0/kind":"peer_message"}),
            ),
            (
                "Claude parent and source tool ids fill meta, null when absent",
                "packages/transcript/src/claude_context_tests.zig:144",
                r###"{"type":"user","isMeta":true,"parentUuid":"result-1","sourceToolUseID":"toolu_1","message":{"content":"Base directory for this skill: /skills/flow"}}"###,
                json!({"/0/meta/parent_uuid":"result-1","/0/meta/source_tool_use_id":"toolu_1"}),
            ),
            (
                "Claude queued command and fallback block become system messages",
                "packages/transcript/src/claude_context_tests.zig:166",
                r###"{"type":"assistant","message":{"content":[{"type":"fallback","from":{"model":"a"},"to":{"model":"b"}}]}}"###,
                json!({"/0/kind":"model_fallback","/0/text":"a -> b"}),
            ),
        ],
    );
}

#[test]
fn claude_tool_main() {
    check(
        "claude",
        &[
            (
                "tool result text array joins and defaults to completed",
                "packages/transcript/src/claude_tool_tests.zig:39",
                r###"{"type":"user","sessionId":"s1","uuid":"u6","timestamp":"t","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1","content":[{"type":"text","text":"one"},{"type":"image","source":"x"},{"type":"text","text":"two"}]}]}}"###,
                json!({"/0/tool_call_id":"tool-1","/0/content":"onetwo","/0/status":"completed"}),
            ),
            (
                "AskUserQuestion becomes elicitation",
                "packages/transcript/src/claude_tool_tests.zig:64",
                r###"{"type":"assistant","sessionId":"s1","uuid":"u8","timestamp":"t","message":{"content":[{"type":"tool_use","id":"tool-3","name":"AskUserQuestion","input":{"questions":[{"question":"Pick one","header":"Choice","multiSelect":true,"options":[{"label":"A","description":"first"},{"label":"B","description":"second"}]}]}}]}}"###,
                json!({"/0/tool_call_id":"tool-3","/0/questions/0/multi_select":true,"/0/questions/0/options/1/label":"B"}),
            ),
            (
                "elicitation answer replaces tool call update",
                "packages/transcript/src/claude_tool_tests.zig:78",
                r###"{"type":"user","sessionId":"s1","uuid":"u9","timestamp":"t","message":{"content":[{"type":"tool_result","tool_use_id":"tool-3","content":"answer"}]},"toolUseResult":{"questions":[],"answers":{"Pick one":"A"},"annotations":{}}}"###,
                json!({"/0/tool_call_id":"tool-3","/0/answers/0/question":"Pick one","/0/answers/0/answer":"A"}),
            ),
            (
                "hook attachment becomes hook result",
                "packages/transcript/src/claude_tool_tests.zig:115",
                r###"{"type":"attachment","sessionId":"s1","uuid":"u11","timestamp":"t","attachment":{"type":"hook_success","hookName":"SessionStart:startup","hookEvent":"SessionStart","toolUseID":"tool-5","exitCode":0}}"###,
                json!({"/0/kind":"hook_success","/0/hook_event":"SessionStart","/0/exit_code":0}),
            ),
            (
                "permission decision keeps an unknown decision string",
                "packages/transcript/src/claude_tool_tests.zig:139",
                r###"{"type":"attachment","sessionId":"s1","uuid":"u13","timestamp":"t","attachment":{"type":"hook_permission_decision","decision":"later","toolUseID":"tool-7","hookEvent":"PermissionRequest"}}"###,
                json!({"/0/decision":"later"}),
            ),
        ],
    );
}

#[test]
fn codex_message_main() {
    check(
        "codex",
        &[
            (
                "mixed user record preserves context prompt and image order",
                "packages/transcript/src/codex_message_tests.zig:58",
                r###"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>host"},{"type":"text","text":"show me this"},{"type":"input_image","media_type":"image/png"}]}}"###,
                json!({"/0/kind":"context","/0/text":"<environment_context>host","/1/text":"show me this","/2/role":"user","/2/media_type":"image/png"}),
            ),
            (
                "reasoning summary becomes thought chunks",
                "packages/transcript/src/codex_message_tests.zig:92",
                r###"{"type":"response_item","payload":{"type":"reasoning","id":"r1","summary":[{"text":"plan"}]}}"###,
                json!({"/0/text":"plan"}),
            ),
        ],
    );
}

#[test]
fn codex_tool_main() {
    check(
        "codex",
        &[
            (
                "Codex turn records become turn events",
                "packages/transcript/src/codex_tool_tests.zig:101",
                r###"{"type":"event_msg","payload":{"type":"turn_aborted"}}"###,
                json!({"/0/reason":"aborted"}),
            ),
            (
                "plan",
                "packages/transcript/src/codex_tool_tests.zig:112",
                r###"{"type":"event_msg","payload":{"type":"agent_reasoning","text":"plan"}}"###,
                json!({"/0/text":"plan"}),
            ),
            (
                "summary",
                "packages/transcript/src/codex_tool_tests.zig:112",
                r###"{"type":"compacted","payload":{"message":"summary"}}"###,
                json!({"/0/text":"summary"}),
            ),
            (
                "failed",
                "packages/transcript/src/codex_tool_tests.zig:112",
                r###"{"type":"event_msg","payload":{"type":"error","message":"failed"}}"###,
                json!({"/0/message":"failed"}),
            ),
            (
                "Codex developer and context text become system messages",
                "packages/transcript/src/codex_tool_tests.zig:132",
                r###"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"rules"}]}}"###,
                json!({"/0/kind":"developer"}),
            ),
            (
                "Codex user and tool images emit image events",
                "packages/transcript/src/codex_tool_tests.zig:141",
                r###"{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c","output":[{"type":"input_image","image_url":"data:image/png;base64,iVBO"},{"type":"input_text","text":"ok"}]}}"###,
                json!({"/0/content":"ok","/1/role":"tool","/1/media_type":"image/png"}),
            ),
            (
                "Codex tool list for the model becomes ignored",
                "packages/transcript/src/codex_tool_tests.zig:215",
                r###"{"type":"response_item","payload":{"type":"additional_tools","id":"at_1","role":"developer","tools":[]}}"###,
                json!({"/0/kind":"additional_tools"}),
            ),
        ],
    );
}
