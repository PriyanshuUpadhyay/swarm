const std = @import("std");
const root = @import("root.zig");

pub fn parseLine(arena: std.mem.Allocator, line: []const u8) ![]root.Event {
    var events: std.ArrayList(root.Event) = .empty;
    const value = std.json.parseFromSliceLeaky(std.json.Value, arena, line, .{}) catch {
        try events.append(arena, try root.unknownEvent(arena, null, line));
        return events.items;
    };
    if (value != .object) {
        try events.append(arena, try root.unknownEvent(arena, null, line));
        return events.items;
    }

    const record = value.object;
    const payload_value = record.get("payload");
    const meta: root.Meta = .{
        .session_id = if (std.mem.eql(u8, root.str(record, "type"), "session_meta") and payload_value != null and payload_value.? == .object)
            root.str(payload_value.?.object, "id")
        else
            "",
        .uuid = if (payload_value) |payload| if (payload == .object) root.str(payload.object, "id") else "" else "",
        .timestamp = root.str(record, "timestamp"),
    };
    if (!std.mem.eql(u8, root.str(record, "type"), "response_item") or payload_value == null or payload_value.? != .object) {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    }

    const payload = payload_value.?.object;
    const payload_type = root.str(payload, "type");
    if (std.mem.eql(u8, payload_type, "message")) {
        const role_value = payload.get("role") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (role_value != .string or
            (!std.mem.eql(u8, role_value.string, "user") and !std.mem.eql(u8, role_value.string, "assistant")))
        {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        const content = payload.get("content") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (content != .array) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        var has_unknown = false;
        for (content.array.items) |part| {
            if (part != .object) {
                if (!has_unknown) {
                    try events.append(arena, try root.unknownEvent(arena, meta, line));
                    has_unknown = true;
                }
                continue;
            }
            const text = part.object.get("text") orelse {
                if (!has_unknown) {
                    try events.append(arena, try root.unknownEvent(arena, meta, line));
                    has_unknown = true;
                }
                continue;
            };
            if (text != .string) {
                if (!has_unknown) {
                    try events.append(arena, try root.unknownEvent(arena, meta, line));
                    has_unknown = true;
                }
                continue;
            }
            const chunk: root.Text = .{ .meta = meta, .text = text.string };
            try events.append(arena, if (std.mem.eql(u8, role_value.string, "user"))
                .{ .user_message_chunk = chunk }
            else
                .{ .agent_message_chunk = chunk });
        }
        if (events.items.len == 0) try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    }

    if (std.mem.eql(u8, payload_type, "reasoning")) {
        const summary = payload.get("summary") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (summary != .array) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        var has_unknown = false;
        for (summary.array.items) |part| {
            if (part != .object) {
                if (!has_unknown) {
                    try events.append(arena, try root.unknownEvent(arena, meta, line));
                    has_unknown = true;
                }
                continue;
            }
            const text = part.object.get("text") orelse {
                if (!has_unknown) {
                    try events.append(arena, try root.unknownEvent(arena, meta, line));
                    has_unknown = true;
                }
                continue;
            };
            if (text != .string) {
                if (!has_unknown) {
                    try events.append(arena, try root.unknownEvent(arena, meta, line));
                    has_unknown = true;
                }
                continue;
            }
            if (text.string.len != 0) {
                try events.append(arena, .{ .agent_thought_chunk = .{ .meta = meta, .text = text.string } });
            }
        }
        return events.items;
    }

    const is_function_call = std.mem.eql(u8, payload_type, "function_call");
    const is_custom_call = std.mem.eql(u8, payload_type, "custom_tool_call");
    if (is_function_call or is_custom_call) {
        const call_id = payload.get("call_id") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        const name = payload.get("name") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        const input = payload.get(if (is_function_call) "arguments" else "input") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (call_id != .string or name != .string or input != .string) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        const parsed_input = std.json.parseFromSliceLeaky(std.json.Value, arena, input.string, .{}) catch std.json.Value{ .string = input.string };
        if (!root.valueFitsDepth(parsed_input, root.max_event_input_depth)) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        try events.append(arena, .{ .tool_call = .{
            .meta = meta,
            .tool_call_id = call_id.string,
            .name = name.string,
            .input = parsed_input,
            .status = .pending,
        } });
        return events.items;
    }

    const is_function_output = std.mem.eql(u8, payload_type, "function_call_output");
    const is_custom_output = std.mem.eql(u8, payload_type, "custom_tool_call_output");
    if (is_function_output or is_custom_output) {
        const call_id = payload.get("call_id") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        const output = payload.get("output") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (call_id != .string) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        var content: []const u8 = "";
        var has_unknown = false;
        if (output == .string) {
            content = output.string;
        } else if (output == .array) {
            var total: usize = 0;
            for (output.array.items) |part| {
                if (part != .object or
                    (!std.mem.eql(u8, root.str(part.object, "type"), "text") and
                        !std.mem.eql(u8, root.str(part.object, "type"), "input_text")) or
                    part.object.get("text") == null or part.object.get("text").? != .string)
                {
                    has_unknown = true;
                    continue;
                }
                total += part.object.get("text").?.string.len;
            }
            const joined = try arena.alloc(u8, total);
            var offset: usize = 0;
            for (output.array.items) |part| {
                if (part != .object or
                    (!std.mem.eql(u8, root.str(part.object, "type"), "text") and
                        !std.mem.eql(u8, root.str(part.object, "type"), "input_text")) or
                    part.object.get("text") == null or part.object.get("text").? != .string)
                {
                    continue;
                }
                const text = part.object.get("text").?.string;
                @memcpy(joined[offset..][0..text.len], text);
                offset += text.len;
            }
            content = joined;
        } else {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        try events.append(arena, .{ .tool_call_update = .{
            .meta = meta,
            .tool_call_id = call_id.string,
            .status = .completed,
            .content = content,
        } });
        if (has_unknown) try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    }

    try events.append(arena, try root.unknownEvent(arena, meta, line));
    return events.items;
}

test "user message content becomes user chunks" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","timestamp":"t","payload":{"type":"message","id":"m1","role":"user","content":[{"type":"input_text","text":"one"},{"type":"input_text","text":"two"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("one", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("", events[0].user_message_chunk.meta.session_id);
    try std.testing.expectEqualStrings("m1", events[0].user_message_chunk.meta.uuid);
    try std.testing.expectEqualStrings("t", events[0].user_message_chunk.meta.timestamp);
    try std.testing.expectEqualStrings("two", events[1].user_message_chunk.text);
}

test "assistant message content becomes agent chunks" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("done", events[0].agent_message_chunk.text);
}

test "reasoning summary becomes thought chunks" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"reasoning","id":"r1","summary":[{"text":"plan"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("plan", events[0].agent_thought_chunk.text);
}

test "empty reasoning summary yields no events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"reasoning","id":"r1","summary":[]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(0, events.len);
}

test "empty reasoning text yields no events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"reasoning","summary":[{"text":""}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(0, events.len);
}

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

test "invalid JSON becomes unknown without meta" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "not json");
    try std.testing.expectEqual(1, events.len);
    try std.testing.expect(events[0].unknown.meta == null);
    try std.testing.expectEqualStrings("not json", events[0].unknown.raw);
}

test "wrong field type becomes unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"function_call","call_id":7,"name":"read","arguments":"{}"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}

test "known message parts survive one unknown part" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":4},{"type":"output_text","text":"kept"},{"type":"output_text"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
    try std.testing.expectEqualStrings("kept", events[1].agent_message_chunk.text);
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

test "bookkeeping record becomes unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"event_msg","timestamp":"t","payload":{"type":"item_completed"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}

test "codex translation makes the line unknown" {
    var reader = std.Io.Reader.fixed("codex line\n");
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try root.translate(std.testing.allocator, .codex, "", &reader, &output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"type\":\"unknown\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"raw\":\"codex line\"") != null);
}
