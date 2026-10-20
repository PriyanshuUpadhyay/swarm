// agy transcript tests.
const std = @import("std");
const root = @import("root.zig");
const parseLine = @import("agy.zig").parseLine;

test "agy translation makes the line unknown" {
    var reader = std.Io.Reader.fixed("agy line\n");
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try root.translate(std.testing.allocator, .agy, "", &reader, &output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"type\":\"unknown\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"raw\":\"agy line\"") != null);
}

test "USER_INPUT becomes a user message" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":12,"created_at":"2026-09-21T10:00:00Z","content":"hello"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("hello", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("12", events[0].user_message_chunk.meta.uuid);
    try std.testing.expectEqualStrings("", events[0].user_message_chunk.meta.session_id);
}

test "USER_INPUT swarm ring inside the request wrapper is a swarm_ring system message" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":14,"created_at":"t","content":"<USER_REQUEST>\nswarm: new message. Run swarm inbox and read each body at /home/owner/.swarm/<body_path>.\n</USER_REQUEST>"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("swarm_ring", events[0].system_message.kind);
    try std.testing.expectEqualStrings("14", events[0].system_message.meta.uuid);
}

test "USER_INPUT strips request wrapper after leading whitespace" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const wrapped =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":12,"created_at":"t","content":" \n<USER_REQUEST>\n  hello  \n</USER_REQUEST> \n"}
    ;
    const events = try parseLine(arena_state.allocator(), wrapped);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("hello", events[0].user_message_chunk.text);
}

test "USER_INPUT keeps metadata and settings after request as context" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":12,"created_at":"t","content":"<USER_REQUEST>hello</USER_REQUEST>\n<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>\n<USER_SETTINGS_CHANGE>settings</USER_SETTINGS_CHANGE>"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("hello", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("context", events[1].system_message.kind);
    try std.testing.expectEqualStrings("<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>\n<USER_SETTINGS_CHANGE>settings</USER_SETTINGS_CHANGE>", events[1].system_message.text);
}

test "USER_INPUT keeps metadata after request as context" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":12,"created_at":"t","content":"<USER_REQUEST>hello</USER_REQUEST>\n<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("hello", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>", events[1].system_message.text);
}

test "USER_INPUT keeps artifact tags inside request" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":12,"created_at":"t","content":"<USER_REQUEST>use <ARTIFACT>file</ARTIFACT> here</USER_REQUEST>\n<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("use <ARTIFACT>file</ARTIFACT> here", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>", events[1].system_message.text);
}

test "USER_INPUT strips opening request tag without closing tag" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":12,"created_at":"t","content":"<USER_REQUEST>\n hello \n"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("hello", events[0].user_message_chunk.text);
}

test "USER_INPUT uses last closing request tag" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":12,"created_at":"t","content":"<USER_REQUEST>first</USER_REQUEST> second</USER_REQUEST>\n<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("first</USER_REQUEST> second", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("<ADDITIONAL_METADATA>data</ADDITIONAL_METADATA>", events[1].system_message.text);
}

test "PLANNER_RESPONSE emits thought message and pending tool calls in order" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":20,"created_at":"t","thinking":"plan","content":"done","tool_calls":[{"name":"view_file","args":{"toolAction":"read","toolSummary":"Read file","AbsolutePath":"/tmp/a"}},{"name":"run_command","args":{"toolAction":"run","toolSummary":"Run command","CommandLine":"pwd"}}]}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(4, events.len);
    try std.testing.expectEqualStrings("plan", events[0].agent_thought_chunk.text);
    try std.testing.expectEqualStrings("done", events[1].agent_message_chunk.text);
    try std.testing.expectEqualStrings("21", events[2].tool_call.tool_call_id);
    try std.testing.expectEqualStrings("view_file", events[2].tool_call.name);
    try std.testing.expectEqual(root.ToolStatus.pending, events[2].tool_call.status);
    try std.testing.expectEqualStrings("22", events[3].tool_call.tool_call_id);
}

test "GENERIC final results become tool call updates" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const done =
        \\{"type":"GENERIC","status":"DONE","source":"MODEL","step_index":21,"created_at":"t","content":"Created At: t\\nresult"}
    ;
    const failed =
        \\{"type":"GENERIC","status":"INVALID","source":"MODEL","step_index":22,"created_at":"t","content":"Created At: t\\nbad"}
    ;
    const done_events = try parseLine(arena_state.allocator(), done);
    const failed_events = try parseLine(arena_state.allocator(), failed);
    try std.testing.expectEqual(1, done_events.len);
    try std.testing.expectEqualStrings("21", done_events[0].tool_call_update.tool_call_id);
    try std.testing.expectEqual(root.ToolStatus.completed, done_events[0].tool_call_update.status);
    try std.testing.expectEqual(root.ToolStatus.failed, failed_events[0].tool_call_update.status);
}

test "invalid JSON becomes unknown without meta" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "not json");
    try std.testing.expectEqual(1, events.len);
    try std.testing.expect(events[0].unknown.meta == null);
    try std.testing.expectEqualStrings("not json", events[0].unknown.raw);
}

test "wrong field type becomes one unknown event" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":23,"created_at":"t","content":true}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}

test "missing and negative step indexes become unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const missing =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","created_at":"t","content":"hello"}
    ;
    const negative =
        \\{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":-1,"created_at":"t","content":"hello"}
    ;
    const missing_events = try parseLine(arena_state.allocator(), missing);
    const negative_events = try parseLine(arena_state.allocator(), negative);
    try std.testing.expectEqual(1, missing_events.len);
    try std.testing.expect(missing_events[0].unknown.meta == null);
    try std.testing.expectEqual(1, negative_events.len);
    try std.testing.expect(negative_events[0].unknown.meta == null);
}

test "known planner parts survive malformed parts with one unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":30,"created_at":"t","thinking":"keep","content":false,"tool_calls":[false,{"name":"run_command","args":{"toolAction":"run","toolSummary":"Run"}},null]}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(3, events.len);
    try std.testing.expectEqualStrings("keep", events[0].agent_thought_chunk.text);
    try std.testing.expectEqualStrings("32", events[1].tool_call.tool_call_id);
    try std.testing.expectEqualStrings(line, events[2].unknown.raw);
}

test "tool input deeper than the event writer limit becomes unknown" {
    var input: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer input.deinit();
    try input.writer.writeAll("{\"type\":\"PLANNER_RESPONSE\",\"status\":\"DONE\",\"source\":\"MODEL\",\"step_index\":40,\"created_at\":\"t\",\"tool_calls\":[{\"name\":\"run_command\",\"args\":{\"value\":");
    for (0..root.max_event_input_depth + 1) |_| try input.writer.writeByte('[');
    try input.writer.writeByte('0');
    for (0..root.max_event_input_depth + 1) |_| try input.writer.writeByte(']');
    try input.writer.writeAll("}}]}");

    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), input.written());
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(input.written(), events[0].unknown.raw);
}

test "a final planner reply with no tool call ends the turn" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const reply =
        \\{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":7,"created_at":"t","thinking":"check","content":"The reviewer approved."}
    ;
    const reply_events = try parseLine(arena_state.allocator(), reply);
    try std.testing.expectEqual(3, reply_events.len);
    try std.testing.expectEqualStrings("The reviewer approved.", reply_events[1].agent_message_chunk.text);
    try std.testing.expectEqual(.completed, reply_events[2].turn_ended.reason);
    try std.testing.expect(reply_events[2].turn_ended.duration_ms == null);
    try std.testing.expectEqualStrings("7", reply_events[2].turn_ended.meta.uuid);

    const thinking_only =
        \\{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":8,"created_at":"t","thinking":"plan","tool_calls":[]}
    ;
    const thinking_events = try parseLine(arena_state.allocator(), thinking_only);
    try std.testing.expectEqual(1, thinking_events.len);
    try std.testing.expectEqualStrings("plan", thinking_events[0].agent_thought_chunk.text);
}

test "RUNNING generic is pending and checkpoint is compaction" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const running =
        \\{"type":"GENERIC","status":"RUNNING","source":"MODEL","step_index":41,"created_at":"t","content":"Created At: t"}
    ;
    const checkpoint =
        \\{"type":"CHECKPOINT","status":"DONE","source":"SYSTEM","step_index":42,"created_at":"t","content":"saved"}
    ;
    const running_events = try parseLine(arena_state.allocator(), running);
    const checkpoint_events = try parseLine(arena_state.allocator(), checkpoint);
    try std.testing.expectEqual(1, running_events.len);
    try std.testing.expectEqual(root.ToolStatus.pending, running_events[0].tool_call_update.status);
    try std.testing.expectEqual(1, checkpoint_events.len);
    try std.testing.expectEqualStrings("compaction", checkpoint_events[0].system_message.kind);
}

test "AGY named tool results join planner calls by step index" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "{\"type\":\"VIEW_FILE\",\"status\":\"DONE\",\"source\":\"MODEL\",\"step_index\":21,\"created_at\":\"t\",\"content\":\"file\"}");
    try std.testing.expectEqualStrings("21", events[0].tool_call_update.tool_call_id);
    try std.testing.expectEqualStrings("file", events[0].tool_call_update.content);
}

test "AGY error and system records become named events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const err = try parseLine(arena_state.allocator(), "{\"type\":\"ERROR_MESSAGE\",\"status\":\"DONE\",\"source\":\"SYSTEM\",\"step_index\":1,\"created_at\":\"t\",\"error\":\"bad\"}");
    const sys = try parseLine(arena_state.allocator(), "{\"type\":\"SYSTEM_MESSAGE\",\"status\":\"DONE\",\"source\":\"SYSTEM\",\"step_index\":2,\"created_at\":\"t\",\"content\":\"note\"}");
    try std.testing.expectEqualStrings("bad", err[0].@"error".message);
    try std.testing.expectEqualStrings("system", sys[0].system_message.kind);
}

test "AGY empty planner and bookkeeping become ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const planner = try parseLine(arena_state.allocator(), "{\"type\":\"PLANNER_RESPONSE\",\"status\":\"DONE\",\"source\":\"MODEL\",\"step_index\":1,\"created_at\":\"t\"}");
    const ephemeral = try parseLine(arena_state.allocator(), "{\"type\":\"EPHEMERAL_MESSAGE\",\"status\":\"DONE\",\"source\":\"SYSTEM\",\"step_index\":2,\"created_at\":\"t\"}");
    try std.testing.expectEqualStrings("PLANNER_RESPONSE", planner[0].ignored.kind);
    try std.testing.expectEqualStrings("EPHEMERAL_MESSAGE", ephemeral[0].ignored.kind);
}

test "null planner fields match absent fields" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":50,"created_at":"t","thinking":null,"content":null,"tool_calls":[{"name":"list_dir","args":{}}]}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("51", events[0].tool_call.tool_call_id);
}

test "tool call id overflow makes the call unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":9223372036854775807,"created_at":"t","tool_calls":[{"name":"list_dir","args":{}}]}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}
