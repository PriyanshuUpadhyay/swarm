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
    const tool = try parseLine(arena_state.allocator(), "{\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call_output\",\"call_id\":\"c\",\"output\":[{\"type\":\"input_image\",\"image_url\":\"secret\"},{\"type\":\"input_text\",\"text\":\"ok\"}]}}");
    try std.testing.expect(user[0].image.role == .user);
    try std.testing.expectEqualStrings("look", user[1].user_message_chunk.text);
    try std.testing.expectEqualStrings("ok", tool[0].tool_call_update.content);
    try std.testing.expect(tool[1].image.role == .tool);
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
