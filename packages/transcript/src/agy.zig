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
    const rec = value.object;
    const step_value = rec.get("step_index") orelse {
        try events.append(arena, try root.unknownEvent(arena, null, line));
        return events.items;
    };
    if (step_value != .integer or step_value.integer < 0) {
        try events.append(arena, try root.unknownEvent(arena, null, line));
        return events.items;
    }
    const created_value = rec.get("created_at") orelse {
        try events.append(arena, try root.unknownEvent(arena, null, line));
        return events.items;
    };
    if (created_value != .string) {
        try events.append(arena, try root.unknownEvent(arena, null, line));
        return events.items;
    }
    const step_index = step_value.integer;
    const meta: root.Meta = .{
        // AGY stores the session id in the containing directory, not in each record.
        .session_id = "",
        .uuid = try std.fmt.allocPrint(arena, "{d}", .{step_index}),
        .timestamp = created_value.string,
    };
    const type_value = rec.get("type") orelse {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    };
    const status_value = rec.get("status") orelse {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    };
    const source_value = rec.get("source") orelse {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    };
    if (type_value != .string or status_value != .string or source_value != .string) {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
        return events.items;
    }

    const record_type = type_value.string;
    if (std.mem.eql(u8, record_type, "USER_INPUT")) {
        const content = rec.get("content") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (content != .string) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        var text = content.string;
        var context: []const u8 = "";
        const open = "<USER_REQUEST>";
        const close = "</USER_REQUEST>";
        const leading_trimmed = std.mem.trimStart(u8, text, " \t\r\n");
        if (std.mem.startsWith(u8, leading_trimmed, open)) {
            const request = leading_trimmed[open.len..];
            if (std.mem.lastIndexOf(u8, request, close)) |end| {
                text = std.mem.trim(u8, request[0..end], " \t\r\n");
                context = std.mem.trim(u8, request[end + close.len ..], " \t\r\n");
            } else {
                text = std.mem.trim(u8, request, " \t\r\n");
            }
        }
        if (root.isSwarmRing(text)) {
            try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "swarm_ring", .text = text } });
        } else {
            try events.append(arena, .{ .user_message_chunk = .{ .meta = meta, .text = text } });
        }
        if (context.len != 0) {
            try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "context", .text = context } });
        }
        return events.items;
    }

    if (std.mem.eql(u8, record_type, "PLANNER_RESPONSE")) {
        var has_unknown = false;
        var replied = false;
        var has_tool_call = false;
        if (rec.get("thinking")) |thinking| {
            if (thinking == .string) {
                if (thinking.string.len != 0) {
                    try events.append(arena, .{ .agent_thought_chunk = .{ .meta = meta, .text = thinking.string } });
                }
            } else if (thinking != .null) {
                has_unknown = true;
            }
        }
        if (rec.get("content")) |content| {
            if (content == .string) {
                if (content.string.len != 0) {
                    try events.append(arena, .{ .agent_message_chunk = .{ .meta = meta, .text = content.string } });
                    replied = true;
                }
            } else if (content != .null) {
                has_unknown = true;
            }
        }
        if (rec.get("tool_calls")) |tool_calls| {
            if (tool_calls == .array) {
                for (tool_calls.array.items, 1..) |tool_call, call_number| {
                    if (tool_call != .object) {
                        has_unknown = true;
                        continue;
                    }
                    const name = tool_call.object.get("name") orelse {
                        has_unknown = true;
                        continue;
                    };
                    const input = tool_call.object.get("args") orelse {
                        has_unknown = true;
                        continue;
                    };
                    if (name != .string or input != .object or !root.valueFitsDepth(input, root.max_event_input_depth)) {
                        has_unknown = true;
                        continue;
                    }
                    const offset = std.math.cast(i64, call_number) orelse {
                        has_unknown = true;
                        continue;
                    };
                    const call_id_index = std.math.add(i64, step_index, offset) catch {
                        has_unknown = true;
                        continue;
                    };
                    try events.append(arena, .{
                        .tool_call = .{
                            .meta = meta,
                            // Observed AGY records pair calls with the next result steps in call order.
                            .tool_call_id = try std.fmt.allocPrint(arena, "{d}", .{call_id_index}),
                            .name = name.string,
                            .input = input,
                            .status = .pending,
                        },
                    });
                    has_tool_call = true;
                }
            } else if (tool_calls != .null) {
                has_unknown = true;
            }
        }
        if (has_unknown) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
        } else if (events.items.len == 0) {
            try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "PLANNER_RESPONSE" } });
        } else if (replied and !has_tool_call and std.mem.eql(u8, status_value.string, "DONE")) {
            // AGY writes no turn end; a finished reply that calls no tool hands control back, as in
            // any agent loop. A reply with no text (thinking only) is not taken as an end.
            try events.append(arena, .{ .turn_ended = .{ .meta = meta, .duration_ms = null, .reason = .completed } });
        }
        return events.items;
    }

    if (std.mem.eql(u8, record_type, "ERROR_MESSAGE")) {
        try events.append(arena, .{ .@"error" = .{ .meta = meta, .message = root.str(rec, "error") } });
        return events.items;
    }

    if (std.mem.eql(u8, record_type, "SYSTEM_MESSAGE") or std.mem.eql(u8, record_type, "CHECKPOINT")) {
        try events.append(arena, .{ .system_message = .{
            .meta = meta,
            .kind = if (std.mem.eql(u8, record_type, "CHECKPOINT")) "compaction" else "system",
            .text = root.str(rec, "content"),
        } });
        return events.items;
    }

    if (root.oneOf(record_type, &.{ "EPHEMERAL_MESSAGE", "CONVERSATION_HISTORY", "DIRECTORY_RULES" })) {
        try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = record_type } });
        return events.items;
    }

    if (root.oneOf(record_type, &.{
        "GENERIC",        "VIEW_FILE",        "RUN_COMMAND", "GREP_SEARCH", "CODE_ACTION",
        "LIST_DIRECTORY", "READ_URL_CONTENT", "SEARCH_WEB",  "MCP_TOOL",    "INVOKE_SUBAGENT",
    })) {
        const content = rec.get("content") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        if (content != .string) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        const status: root.ToolStatus = if (std.mem.eql(u8, status_value.string, "DONE"))
            .completed
        else if (std.mem.eql(u8, status_value.string, "RUNNING"))
            .pending
        else if (std.mem.eql(u8, status_value.string, "ERROR") or std.mem.eql(u8, status_value.string, "INVALID"))
            .failed
        else {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        };
        try events.append(arena, .{ .tool_call_update = .{
            .meta = meta,
            .tool_call_id = meta.uuid,
            .status = status,
            .content = content.string,
        } });
        return events.items;
    }

    try events.append(arena, try root.unknownEvent(arena, meta, line));
    return events.items;
}

test {
    _ = @import("agy_tests.zig");
}
