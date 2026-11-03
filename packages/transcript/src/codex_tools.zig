// Codex tool calls and their outputs, mapped to transcript events.
const std = @import("std");
const root = @import("root.zig");
const q = @import("codex_questions.zig");

/// Appends the events of one Codex `response_item` tool record. False when the record is no
/// tool call, search, or tool output, so the caller maps it.
pub fn append(
    arena: std.mem.Allocator,
    events: *std.ArrayList(root.Event),
    meta: root.Meta,
    payload: std.json.ObjectMap,
    payload_type: []const u8,
    line: []const u8,
) !bool {
    if (std.mem.eql(u8, payload_type, "web_search_call") or std.mem.eql(u8, payload_type, "tool_search_call")) {
        const is_web = std.mem.eql(u8, payload_type, "web_search_call");
        const input = payload.get(if (is_web) "action" else "arguments") orelse .null;
        if (!root.valueFitsDepth(input, root.max_event_input_depth)) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        }
        try events.append(arena, .{ .tool_call = .{
            .meta = meta,
            .tool_call_id = root.str(payload, "call_id"),
            .name = if (is_web) "web_search" else "tool_search",
            .input = input,
            .status = if (is_web) .completed else .pending,
        } });
        return true;
    }

    if (std.mem.eql(u8, payload_type, "tool_search_output")) {
        const tools = payload.get("tools") orelse .null;
        if (!root.valueFitsDepth(tools, root.max_event_input_depth)) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        }
        const content = if (tools == .null) "" else try std.json.Stringify.valueAlloc(arena, tools, .{});
        try events.append(arena, .{ .tool_call_update = .{
            .meta = meta,
            .tool_call_id = root.str(payload, "call_id"),
            .status = .completed,
            .content = content,
        } });
        return true;
    }

    const is_function_call = std.mem.eql(u8, payload_type, "function_call");
    const is_custom_call = std.mem.eql(u8, payload_type, "custom_tool_call");
    if (is_function_call or is_custom_call) {
        const call_id = payload.get("call_id") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        };
        const name = payload.get("name") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        };
        const input = payload.get(if (is_function_call) "arguments" else "input") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        };
        if (call_id != .string or name != .string or input != .string) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        }
        const parsed_input = std.json.parseFromSliceLeaky(std.json.Value, arena, input.string, .{}) catch std.json.Value{ .string = input.string };
        if (!root.valueFitsDepth(parsed_input, root.max_event_input_depth)) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        }
        if (root.oneOf(name.string, &.{ "request_user_input_async", "request_user_input" })) {
            try events.append(arena, .{ .elicitation = .{
                .meta = meta,
                .tool_call_id = call_id.string,
                .questions = try q.questions(arena, parsed_input),
            } });
            return true;
        }
        try events.append(arena, .{ .tool_call = .{
            .meta = meta,
            .tool_call_id = call_id.string,
            .name = name.string,
            .input = parsed_input,
            .status = .pending,
        } });
        return true;
    }

    const is_function_output = std.mem.eql(u8, payload_type, "function_call_output");
    const is_custom_output = std.mem.eql(u8, payload_type, "custom_tool_call_output");
    if (is_function_output or is_custom_output) {
        const call_id = payload.get("call_id") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        };
        const output = payload.get("output") orelse {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
        };
        if (call_id != .string) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return true;
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
            return true;
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
                try events.append(arena, .{ .image = .{ .meta = meta, .role = .tool, .media_type = q.imageMediaType(part.object) } });
            }
        }
        if (has_unknown) try events.append(arena, try root.unknownEvent(arena, meta, line));
        return true;
    }

    return false;
}
