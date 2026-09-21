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
    const record_type = root.str(record, "type");
    if (payload_value == null or payload_value.? != .object) {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    }

    const payload = payload_value.?.object;
    const payload_type = root.str(payload, "type");
    if (std.mem.eql(u8, record_type, "session_meta") or std.mem.eql(u8, record_type, "turn_context")) {
        for ([_]struct { key: []const u8, kind: root.SessionInfoKind }{
            .{ .key = "model", .kind = .model },
            .{ .key = "cwd", .kind = .cwd },
        }) |field| {
            const entry = payload.get(field.key) orelse continue;
            if (entry != .string) continue;
            try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = field.kind, .value = entry.string } });
        }
        if (events.items.len == 0) try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = record_type } });
        return events.items;
    }
    if (std.mem.eql(u8, record_type, "event_msg")) {
        if (std.mem.eql(u8, payload_type, "agent_reasoning")) {
            try events.append(arena, .{ .agent_thought_chunk = .{ .meta = meta, .text = root.str(payload, "text") } });
        } else if (std.mem.eql(u8, payload_type, "task_started")) {
            try events.append(arena, .{ .turn_started = .{ .meta = meta } });
        } else if (std.mem.eql(u8, payload_type, "task_complete") or std.mem.eql(u8, payload_type, "turn_aborted")) {
            const duration = payload.get("duration_ms") orelse .null;
            try events.append(arena, .{ .turn_ended = .{
                .meta = meta,
                .duration_ms = if (duration == .integer) duration.integer else null,
                .reason = if (std.mem.eql(u8, payload_type, "task_complete")) .completed else .aborted,
            } });
        } else if (std.mem.eql(u8, payload_type, "context_compacted")) {
            try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "compaction", .text = root.str(payload, "message") } });
        } else if (std.mem.eql(u8, payload_type, "error")) {
            try events.append(arena, .{ .@"error" = .{ .meta = meta, .message = root.str(payload, "message") } });
        } else if (root.oneOf(payload_type, &.{
            "agent_message",           "user_message",        "exec_command_end",   "patch_apply_end",
            "mcp_tool_call_end",       "web_search_end",      "item_completed",     "token_count",
            "thread_settings_applied", "entered_review_mode", "exited_review_mode", "sub_agent_activity",
        })) {
            try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = try std.fmt.allocPrint(arena, "event_msg/{s}", .{payload_type}) } });
        } else {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
        }
        return events.items;
    }
    if (std.mem.eql(u8, record_type, "compacted")) {
        try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "compaction", .text = root.str(payload, "message") } });
        return events.items;
    }
    if (root.oneOf(record_type, &.{ "token_usage_record", "world_state", "inter_agent_communication_metadata" })) {
        try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = record_type } });
        return events.items;
    }
    if (!std.mem.eql(u8, record_type, "response_item")) {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    }
    if (std.mem.eql(u8, payload_type, "ghost_snapshot")) {
        try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = payload_type } });
        return events.items;
    }
    if (std.mem.eql(u8, payload_type, "message")) {
        const role_value = payload.get("role") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (role_value != .string or
            !root.oneOf(role_value.string, &.{ "user", "assistant", "developer", "system" }))
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
        const is_user = std.mem.eql(u8, role_value.string, "user");
        const is_context = std.mem.eql(u8, role_value.string, "developer") or std.mem.eql(u8, role_value.string, "system");
        for (content.array.items) |part| {
            if (part != .object) {
                if (!has_unknown) {
                    try events.append(arena, try root.unknownEvent(arena, meta, line));
                    has_unknown = true;
                }
                continue;
            }
            const part_type = root.str(part.object, "type");
            if (std.mem.eql(u8, part_type, "input_image")) {
                const media_type = root.str(part.object, "media_type");
                try events.append(arena, .{ .image = .{ .meta = meta, .role = if (is_user) .user else if (is_context) .tool else .agent, .media_type = media_type } });
                continue;
            }
            if (!root.oneOf(part_type, &.{ "input_text", "output_text" })) {
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
            if (is_context) {
                try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = role_value.string, .text = text.string } });
            } else if (is_user) {
                const trimmed = std.mem.trimStart(u8, text.string, " \t\r\n");
                if (root.oneOfPrefix(trimmed, &.{ "<environment_context>", "<user_instructions>", "# AGENTS.md", "<permissions instructions>" })) {
                    try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "context", .text = text.string } });
                } else {
                    try events.append(arena, .{ .user_message_chunk = chunk });
                }
            } else {
                try events.append(arena, .{ .agent_message_chunk = chunk });
            }
        }
        if (events.items.len == 0) try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    }

    if (std.mem.eql(u8, payload_type, "reasoning")) {
        const summary = payload.get("summary") orelse {
            try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "response_item/reasoning" } });
            return events.items;
        };
        if (summary == .null) {
            try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "response_item/reasoning" } });
            return events.items;
        }
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
        if (events.items.len == 0) try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "response_item/reasoning" } });
        return events.items;
    }

    if (std.mem.eql(u8, payload_type, "agent_message")) {
        const content = payload.get("content") orelse .null;
        if (content != .array) {
            try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "response_item/agent_message" } });
            return events.items;
        }
        for (content.array.items) |part| {
            if (part != .object) continue;
            if (std.mem.eql(u8, root.str(part.object, "type"), "input_text")) {
                try events.append(arena, .{ .agent_message_chunk = .{ .meta = meta, .text = root.str(part.object, "text") } });
            }
        }
        if (events.items.len == 0) try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "response_item/agent_message" } });
        return events.items;
    }

    if (std.mem.eql(u8, payload_type, "web_search_call") or std.mem.eql(u8, payload_type, "tool_search_call")) {
        const is_web = std.mem.eql(u8, payload_type, "web_search_call");
        const input = payload.get(if (is_web) "action" else "arguments") orelse .null;
        if (!root.valueFitsDepth(input, root.max_event_input_depth)) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        try events.append(arena, .{ .tool_call = .{
            .meta = meta,
            .tool_call_id = root.str(payload, "call_id"),
            .name = if (is_web) "web_search" else "tool_search",
            .input = input,
            .status = .completed,
        } });
        return events.items;
    }

    if (std.mem.eql(u8, payload_type, "tool_search_output")) {
        const tools = payload.get("tools") orelse .null;
        const content = if (tools == .null or !root.valueFitsDepth(tools, root.max_event_input_depth))
            ""
        else
            try std.json.Stringify.valueAlloc(arena, tools, .{});
        try events.append(arena, .{ .tool_call_update = .{
            .meta = meta,
            .tool_call_id = root.str(payload, "call_id"),
            .status = .completed,
            .content = content,
        } });
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
                if (part == .object and std.mem.eql(u8, root.str(part.object, "type"), "input_image")) continue;
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
        if (output == .array) {
            for (output.array.items) |part| {
                if (part != .object or !std.mem.eql(u8, root.str(part.object, "type"), "input_image")) continue;
                try events.append(arena, .{ .image = .{ .meta = meta, .role = .tool, .media_type = root.str(part.object, "media_type") } });
            }
        }
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
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("response_item/reasoning", events[0].ignored.kind);
}

test "empty reasoning text yields no events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"reasoning","summary":[{"text":""}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expect(events[0] == .ignored);
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
