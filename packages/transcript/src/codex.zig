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
        } else if (std.mem.eql(u8, payload_type, "token_count")) {
            const info = payload.get("info") orelse .null;
            const last = if (info == .object) info.object.get("last_token_usage") orelse .null else .null;
            if (last == .object) {
                const total = info.object.get("total_token_usage") orelse .null;
                try events.append(arena, .{ .usage = .{
                    .meta = meta,
                    .source = .codex,
                    .kind = .context,
                    .context_tokens = root.tokenCount(last.object, "total_tokens"),
                    .context_capacity_tokens = root.tokenCount(info.object, "model_context_window"),
                    .input_tokens = root.tokenCount(last.object, "input_tokens"),
                    .output_tokens = root.tokenCount(last.object, "output_tokens"),
                    .cache_read_tokens = root.tokenCount(last.object, "cached_input_tokens"),
                    .cache_write_tokens = root.tokenCount(last.object, "cache_write_input_tokens"),
                    .session_input_tokens = if (total == .object) root.tokenCount(total.object, "input_tokens") else null,
                    .session_output_tokens = if (total == .object) root.tokenCount(total.object, "output_tokens") else null,
                } });
            } else {
                try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "event_msg/token_count" } });
            }
        } else if (std.mem.eql(u8, payload_type, "error")) {
            try events.append(arena, .{ .@"error" = .{ .meta = meta, .message = root.str(payload, "message") } });
        } else if (std.mem.eql(u8, payload_type, "user_message")) {
            try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "event_msg/user_message" } });
        } else if (root.oneOf(payload_type, &.{
            "agent_message",           "exec_command_end",    "patch_apply_end",
            "mcp_tool_call_end",       "web_search_end",      "item_completed",
            "thread_settings_applied", "entered_review_mode", "exited_review_mode",
            "sub_agent_activity",
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
            if (!root.oneOf(part_type, &.{ "input_text", "output_text" }) and
                !(is_user and std.mem.eql(u8, part_type, "text")))
            {
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
                // Measured on 2026-09-22; an unlisted tag is a user chunk, never unknown.
                const context_prefixes = [_][]const u8{
                    "<environment_context>",
                    "<user_instructions>",
                    "<permissions instructions>",
                    "<skill>",
                    "<user_shell_command>",
                    "<turn_aborted>",
                    "<recommended_plugins>",
                    "<subagent_notification>",
                    "<no retained transcript delta entries>",
                    "# AGENTS.md",
                };
                var is_tagged_context = false;
                for (context_prefixes) |prefix| {
                    if (std.mem.startsWith(u8, trimmed, prefix)) {
                        is_tagged_context = true;
                        break;
                    }
                }
                if (is_tagged_context) {
                    try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "context", .text = text.string } });
                } else if (root.isSwarmRing(text.string)) {
                    try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "swarm_ring", .text = text.string } });
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
            .status = if (is_web) .completed else .pending,
        } });
        return events.items;
    }

    if (std.mem.eql(u8, payload_type, "tool_search_output")) {
        const tools = payload.get("tools") orelse .null;
        if (!root.valueFitsDepth(tools, root.max_event_input_depth)) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
            return events.items;
        }
        const content = if (tools == .null) "" else try std.json.Stringify.valueAlloc(arena, tools, .{});
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
