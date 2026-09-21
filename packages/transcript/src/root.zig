const std = @import("std");

pub const Meta = struct {
    session_id: []const u8,
    uuid: []const u8,
    timestamp: []const u8,
};

pub const Text = struct { meta: Meta, text: []const u8 };

pub const Unknown = struct {
    meta: ?Meta,
    raw: []const u8,
};

pub const ToolStatus = enum { pending, completed, failed };

pub const ToolCall = struct {
    meta: Meta,
    tool_call_id: []const u8,
    name: []const u8,
    input: std.json.Value,
    status: ToolStatus,
};

pub const ToolCallUpdate = struct {
    meta: Meta,
    tool_call_id: []const u8,
    status: ToolStatus,
    content: []const u8,
};

pub const Event = union(enum) {
    user_message_chunk: Text,
    agent_message_chunk: Text,
    agent_thought_chunk: Text,
    tool_call: ToolCall,
    tool_call_update: ToolCallUpdate,
    unknown: Unknown,
};

fn str(obj: std.json.ObjectMap, key: []const u8) []const u8 {
    const value = obj.get(key) orelse return "";
    return if (value == .string) value.string else "";
}

pub fn parseLine(arena: std.mem.Allocator, line: []const u8) ![]Event {
    var events: std.ArrayList(Event) = .empty;
    const root = std.json.parseFromSliceLeaky(std.json.Value, arena, line, .{}) catch {
        try events.append(arena, .{ .unknown = .{ .meta = null, .raw = line } });
        return events.items;
    };
    if (root != .object) {
        try events.append(arena, .{ .unknown = .{ .meta = null, .raw = line } });
        return events.items;
    }
    const rec = root.object;
    const meta: Meta = .{ .session_id = str(rec, "sessionId"), .uuid = str(rec, "uuid"), .timestamp = str(rec, "timestamp") };
    const record_type = str(rec, "type");
    const is_user = std.mem.eql(u8, record_type, "user");
    if (!is_user and !std.mem.eql(u8, record_type, "assistant")) {
        try events.append(arena, .{ .unknown = .{ .meta = meta, .raw = line } });
        return events.items;
    }
    const message = rec.get("message") orelse return events.items;
    if (message != .object) return events.items;
    const content = message.object.get("content") orelse return events.items;
    if (content == .string and is_user) {
        try events.append(arena, .{ .user_message_chunk = .{ .meta = meta, .text = content.string } });
    }
    if (content != .array) return events.items;
    for (content.array.items) |block| {
        if (block != .object) continue;
        const block_type = str(block.object, "type");
        if (std.mem.eql(u8, block_type, "text")) {
            const chunk: Text = .{ .meta = meta, .text = str(block.object, "text") };
            try events.append(arena, if (is_user) .{ .user_message_chunk = chunk } else .{ .agent_message_chunk = chunk });
        } else if (std.mem.eql(u8, block_type, "thinking")) {
            try events.append(arena, .{ .agent_thought_chunk = .{ .meta = meta, .text = str(block.object, "thinking") } });
        } else if (std.mem.eql(u8, block_type, "tool_use") and !is_user) {
            try events.append(arena, .{ .tool_call = .{
                .meta = meta,
                .tool_call_id = str(block.object, "id"),
                .name = str(block.object, "name"),
                .input = block.object.get("input") orelse .null,
                .status = .pending,
            } });
        } else if (std.mem.eql(u8, block_type, "tool_result") and is_user) {
            const result_content = block.object.get("content") orelse .null;
            var result_text: []const u8 = "";
            if (result_content == .string) {
                result_text = result_content.string;
            } else if (result_content == .array) {
                var total: usize = 0;
                for (result_content.array.items) |part| {
                    if (part == .object and std.mem.eql(u8, str(part.object, "type"), "text")) {
                        total += str(part.object, "text").len;
                    }
                }
                const joined = try arena.alloc(u8, total);
                var offset: usize = 0;
                for (result_content.array.items) |part| {
                    if (part != .object or !std.mem.eql(u8, str(part.object, "type"), "text")) continue;
                    const text = str(part.object, "text");
                    @memcpy(joined[offset..][0..text.len], text);
                    offset += text.len;
                }
                result_text = joined;
            }
            const is_error = if (block.object.get("is_error")) |value| value == .bool and value.bool else false;
            try events.append(arena, .{ .tool_call_update = .{
                .meta = meta,
                .tool_call_id = str(block.object, "tool_use_id"),
                .status = if (is_error) .failed else .completed,
                .content = result_text,
            } });
        } else {
            try events.append(arena, .{ .unknown = .{ .meta = meta, .raw = line } });
        }
    }
    return events.items;
}

test "user string content becomes user_message_chunk" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"user","sessionId":"s1","uuid":"u1","timestamp":"2026-09-21T10:00:00Z","message":{"role":"user","content":"hello"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("hello", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("s1", events[0].user_message_chunk.meta.session_id);
}

test "assistant text and thinking blocks become two chunks" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u2","timestamp":"t","message":{"content":[{"type":"thinking","thinking":"plan"},{"type":"text","text":"done"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("plan", events[0].agent_thought_chunk.text);
    try std.testing.expectEqualStrings("done", events[1].agent_message_chunk.text);
}

test "unknown record keeps raw line and meta" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"mode","sessionId":"s1","uuid":"u3","timestamp":"t","mode":"plan"}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
    try std.testing.expectEqualStrings("s1", events[0].unknown.meta.?.session_id);
}

test "invalid JSON becomes unknown without meta" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line = "not json";
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
    try std.testing.expect(events[0].unknown.meta == null);
}

test "unknown content block becomes unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u4","timestamp":"t","message":{"content":[{"type":"image","source":"x"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}

test "assistant tool use becomes pending tool call" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u5","timestamp":"t","message":{"content":[{"type":"tool_use","id":"tool-1","name":"Read","input":{"path":"a"}}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("tool-1", events[0].tool_call.tool_call_id);
    try std.testing.expectEqualStrings("Read", events[0].tool_call.name);
    try std.testing.expectEqual(ToolStatus.pending, events[0].tool_call.status);
    try std.testing.expectEqualStrings("a", str(events[0].tool_call.input.object, "path"));
}

test "tool result text array joins and defaults to completed" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"user","sessionId":"s1","uuid":"u6","timestamp":"t","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1","content":[{"type":"text","text":"one"},{"type":"image","source":"x"},{"type":"text","text":"two"}]}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("tool-1", events[0].tool_call_update.tool_call_id);
    try std.testing.expectEqualStrings("onetwo", events[0].tool_call_update.content);
    try std.testing.expectEqual(ToolStatus.completed, events[0].tool_call_update.status);
}

test "tool result with is_error becomes failed" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"user","sessionId":"s1","uuid":"u7","timestamp":"t","message":{"content":[{"type":"tool_result","tool_use_id":"tool-2","content":"bad","is_error":true}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqual(ToolStatus.failed, events[0].tool_call_update.status);
    try std.testing.expectEqualStrings("bad", events[0].tool_call_update.content);
}
