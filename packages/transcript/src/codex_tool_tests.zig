// Codex tool, turn, and usage tests.
const std = @import("std");
const root = @import("root.zig");
const parseLine = @import("codex.zig").parseLine;

test "function call parses JSON arguments" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"function_call","id":"f1","call_id":"call-1","name":"read","arguments":"{\"path\":\"a\"}"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("call-1", events[0].tool_call.tool_call_id);
    try std.testing.expectEqualStrings("read", events[0].tool_call.name);
    try std.testing.expectEqual(root.ToolStatus.pending, events[0].tool_call.status);
    try std.testing.expectEqualStrings("a", root.str(events[0].tool_call.input.object, "path"));
}

test "custom tool call keeps invalid JSON input as a string" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"custom_tool_call","call_id":"call-2","name":"render","input":"not json"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("not json", events[0].tool_call.input.string);
}

test "function call output keeps string content" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"ok"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("call-1", events[0].tool_call_update.tool_call_id);
    try std.testing.expectEqualStrings("ok", events[0].tool_call_update.content);
    try std.testing.expectEqual(root.ToolStatus.completed, events[0].tool_call_update.status);
}

test "custom tool call output joins text parts" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call-2","output":[{"type":"text","text":"one"},{"type":"text","text":"two"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("onetwo", events[0].tool_call_update.content);
}

test "function and custom outputs join input_text parts by call id" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const function_line =
        \\{"type":"response_item","payload":{"type":"function_call_output","call_id":"f1","output":[{"type":"input_text","text":"one"},{"type":"input_text","text":"two"}]}}
    ;
    const custom_line =
        \\{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c1","output":[{"type":"input_text","text":"three"}]}}
    ;
    const function_events = try parseLine(arena_state.allocator(), function_line);
    const custom_events = try parseLine(arena_state.allocator(), custom_line);
    try std.testing.expectEqual(1, function_events.len);
    try std.testing.expectEqualStrings("f1", function_events[0].tool_call_update.tool_call_id);
    try std.testing.expectEqualStrings("onetwo", function_events[0].tool_call_update.content);
    try std.testing.expectEqual(1, custom_events.len);
    try std.testing.expectEqualStrings("c1", custom_events[0].tool_call_update.tool_call_id);
    try std.testing.expectEqualStrings("three", custom_events[0].tool_call_update.content);
}

test "tool input deeper than the JSON writer limit becomes unknown" {
    var input: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer input.deinit();
    try input.writer.writeAll("{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"call_id\":\"deep\",\"name\":\"read\",\"arguments\":\"");
    for (0..root.max_event_input_depth + 1) |_| try input.writer.writeByte('[');
    try input.writer.writeByte('0');
    for (0..root.max_event_input_depth + 1) |_| try input.writer.writeByte(']');
    try input.writer.writeAll("\"}}");

    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), input.written());
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(input.written(), events[0].unknown.raw);
}

test "bookkeeping record becomes ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"event_msg","timestamp":"t","payload":{"type":"item_completed"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("event_msg/item_completed", events[0].ignored.kind);
}

test "Codex turn records become turn events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const started = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}");
    const completed = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"duration_ms\":42}}");
    const aborted = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\"}}");
    try std.testing.expect(started[0] == .turn_started);
    try std.testing.expectEqual(@as(?i64, 42), completed[0].turn_ended.duration_ms);
    try std.testing.expect(aborted[0].turn_ended.reason == .aborted);
}

test "Codex reasoning compaction and error become named events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const thought = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_reasoning\",\"text\":\"plan\"}}");
    const compact = try parseLine(arena_state.allocator(), "{\"type\":\"compacted\",\"payload\":{\"message\":\"summary\"}}");
    const err = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"error\",\"message\":\"failed\"}}");
    try std.testing.expectEqualStrings("plan", thought[0].agent_thought_chunk.text);
    try std.testing.expectEqualStrings("summary", compact[0].system_message.text);
    try std.testing.expectEqualStrings("failed", err[0].@"error".message);
}

test "Codex session fields become session info" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "{\"type\":\"turn_context\",\"payload\":{\"model\":\"m\",\"cwd\":\"/work\"}}");
    try std.testing.expectEqual(2, events.len);
    try std.testing.expect(events[0].session_info.kind == .model);
    try std.testing.expectEqualStrings("/work", events[1].session_info.value);
}

test "Codex developer and context text become system messages" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const developer = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"developer\",\"content\":[{\"type\":\"input_text\",\"text\":\"rules\"}]}}");
    const context = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"# AGENTS.md instructions\"}]}}");
    try std.testing.expectEqualStrings("developer", developer[0].system_message.kind);
    try std.testing.expectEqualStrings("context", context[0].system_message.kind);
}

test "Codex user and tool images emit image events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const user = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_image\",\"media_type\":\"image/png\",\"image_url\":\"secret\"},{\"type\":\"input_text\",\"text\":\"look\"}]}}");
    const tool = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call_output\",\"call_id\":\"c\",\"output\":[{\"type\":\"input_image\",\"image_url\":\"data:image/png;base64,iVBO\"},{\"type\":\"input_text\",\"text\":\"ok\"}]}}");
    try std.testing.expect(user[0].image.role == .user);
    try std.testing.expectEqualStrings("image/png", user[0].image.media_type);
    try std.testing.expectEqualStrings("look", user[1].user_message_chunk.text);
    try std.testing.expectEqualStrings("ok", tool[0].tool_call_update.content);
    try std.testing.expect(tool[1].image.role == .tool);
    // Codex 0.162 writes a screenshot with no media_type; its data URL names the type.
    try std.testing.expectEqualStrings("image/png", tool[1].image.media_type);
}

test "Codex question tools become elicitation" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    // Measured in a Codex 0.162.1 rollout on 2026-11-03 and 2026-10-23.
    const async_line =
        \\{"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","arguments":"{\"questions\":[{\"title\":\"Sign in now?\",\"options\":[\"Yes\",\"Later\"]}]}","call_id":"call-a"}}
    ;
    const blocking_line =
        \\{"type":"response_item","payload":{"type":"function_call","name":"request_user_input","arguments":"{\"questions\":[{\"header\":\"Color\",\"id\":\"color\",\"question\":\"Red or blue?\",\"options\":[{\"label\":\"Red\",\"description\":\"Choose red.\"},{\"label\":\"Blue\",\"description\":\"Choose blue.\"}]}]}","call_id":"call-b"}}
    ;
    const asked = try parseLine(arena_state.allocator(), async_line);
    try std.testing.expectEqual(1, asked.len);
    try std.testing.expectEqualStrings("call-a", asked[0].elicitation.tool_call_id);
    try std.testing.expectEqualStrings("Sign in now?", asked[0].elicitation.questions[0].question);
    try std.testing.expectEqualStrings("Later", asked[0].elicitation.questions[0].options[1].label);
    const blocking = try parseLine(arena_state.allocator(), blocking_line);
    try std.testing.expectEqualStrings("call-b", blocking[0].elicitation.tool_call_id);
    try std.testing.expectEqualStrings("Red or blue?", blocking[0].elicitation.questions[0].question);
    try std.testing.expectEqualStrings("Color", blocking[0].elicitation.questions[0].header);
    try std.testing.expectEqualStrings("Choose blue.", blocking[0].elicitation.questions[0].options[1].description);
}

test "Codex question replies become elicitation results" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const reply_line =
        \\{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<send_user_message_question_reply>\n[{\"answer\":\"Later\",\"question\":\"Sign in now?\",\"questionItemId\":\"[\\\"request_user_input_async\\\",\\\"call-a\\\",0]\"}]\n</send_user_message_question_reply>"}]}}
    ;
    const verified_line =
        \\{"type":"retained_context","payload":{"type":"verified_answer","call_id":"call-b","questions":[{"question":"Red or blue?\nRed: Choose red.","answer":"Red"}]}}
    ;
    const reply = try parseLine(arena_state.allocator(), reply_line);
    try std.testing.expectEqual(1, reply.len);
    try std.testing.expectEqualStrings("call-a", reply[0].elicitation_result.tool_call_id);
    try std.testing.expectEqualStrings("Sign in now?", reply[0].elicitation_result.answers[0].question);
    try std.testing.expectEqualStrings("Later", reply[0].elicitation_result.answers[0].answer);
    // Codex writes `verified_answer` only with its GuardianApproval feature, so the blocking
    // tool's own output carries the answer, and the record would repeat it.
    const verified = try parseLine(arena_state.allocator(), verified_line);
    try std.testing.expectEqualStrings("retained_context/verified_answer", verified[0].ignored.kind);
    const blocking = try parseLine(arena_state.allocator(),
        \\{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-b","output":"{\"answers\":{\"color\":{\"answers\":[\"Red\"]}}}"}}
    );
    try std.testing.expectEqual(1, blocking.len);
    try std.testing.expectEqualStrings("call-b", blocking[0].elicitation_result.tool_call_id);
    try std.testing.expectEqualStrings("color", blocking[0].elicitation_result.answers[0].question);
    try std.testing.expectEqualStrings("Red", blocking[0].elicitation_result.answers[0].answer);
    // The async tool's output only says Codex showed the question.
    const accepted = try parseLine(arena_state.allocator(),
        \\{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-a","output":"{\"accepted\":true}"}}
    );
    try std.testing.expectEqualStrings("question_accepted", accepted[0].ignored.kind);
    // The Codex TUI also takes one bare reply object.
    const one = try parseLine(arena_state.allocator(),
        \\{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<send_user_message_question_reply>{\"answer\":\"Yes\",\"question\":\"Q\",\"questionItemId\":\"[\\\"request_user_input_async\\\",\\\"call-c\\\",0]\"}</send_user_message_question_reply>"}]}}
    );
    try std.testing.expectEqualStrings("call-c", one[0].elicitation_result.tool_call_id);
    try std.testing.expectEqualStrings("Yes", one[0].elicitation_result.answers[0].answer);
    // A user who types the tag by hand, with no reply in it, keeps their message.
    const typed = try parseLine(arena_state.allocator(),
        \\{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<send_user_message_question_reply> is the tag"}]}}
    );
    try std.testing.expectEqualStrings("<send_user_message_question_reply> is the tag", typed[0].user_message_chunk.text);
}

test "Codex tool list for the model becomes ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"additional_tools","id":"at_1","role":"developer","tools":[]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("additional_tools", events[0].ignored.kind);
}

test "Codex web and tool search calls keep inputs and ids" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const web = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"web_search_call\",\"action\":{\"query\":\"a\"}}}");
    const search = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"tool_search_call\",\"call_id\":\"c\",\"arguments\":{\"query\":\"tool\"}}}");
    const output = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"tool_search_output\",\"call_id\":\"c\",\"tools\":[{\"name\":\"t\"}]}}");
    try std.testing.expectEqualStrings("web_search", web[0].tool_call.name);
    try std.testing.expect(web[0].tool_call.status == .completed);
    try std.testing.expectEqualStrings("c", search[0].tool_call.tool_call_id);
    try std.testing.expect(search[0].tool_call.status == .pending);
    try std.testing.expectEqualStrings("c", output[0].tool_call_update.tool_call_id);
    try std.testing.expectEqualStrings("[{\"name\":\"t\"}]", output[0].tool_call_update.content);
}

test "Codex agent message reads text and ignores encrypted parts" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"agent_message\",\"content\":[{\"type\":\"encrypted_content\",\"data\":\"secret\"},{\"type\":\"input_text\",\"text\":\"hello\"}]}}");
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("hello", events[0].agent_message_chunk.text);
}

test "codex translation makes the line unknown" {
    var reader = std.Io.Reader.fixed("codex line\n");
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try root.translate(std.testing.allocator, .codex, "", &reader, &output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"type\":\"unknown\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"raw\":\"codex line\"") != null);
}

test "Codex context uses last total and does not add cached input twice" {
    var arena: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena.deinit();
    const events = try parseLine(arena.allocator(),
        \\{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":80,"output_tokens":20,"total_tokens":120},"total_token_usage":{"input_tokens":9000,"output_tokens":1000},"model_context_window":200}}}
    );
    try std.testing.expectEqual(@as(?i64, 120), events[0].usage.context_tokens);
    try std.testing.expectEqual(@as(?i64, 100), events[0].usage.input_tokens);
    try std.testing.expectEqual(@as(?i64, 80), events[0].usage.cache_read_tokens);
    try std.testing.expectEqual(@as(?i64, 9000), events[0].usage.session_input_tokens);
    try std.testing.expectEqual(@as(?i64, 200), events[0].usage.context_capacity_tokens);
    const missing = try parseLine(arena.allocator(),
        \\{"type":"event_msg","payload":{"type":"token_count","info":null}}
    );
    try std.testing.expect(missing[0] == .ignored);
}
