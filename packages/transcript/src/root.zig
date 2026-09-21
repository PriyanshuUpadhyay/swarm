const std = @import("std");

pub const Meta = struct {
    session_id: []const u8,
    uuid: []const u8,
    timestamp: []const u8,
};

pub const Text = struct { meta: Meta, text: []const u8 };

pub const Event = union(enum) {
    user_message_chunk: Text,
    agent_message_chunk: Text,
    agent_thought_chunk: Text,
};

fn str(obj: std.json.ObjectMap, key: []const u8) []const u8 {
    const value = obj.get(key) orelse return "";
    return if (value == .string) value.string else "";
}

pub fn parseLine(arena: std.mem.Allocator, line: []const u8) ![]Event {
    var events: std.ArrayList(Event) = .empty;
    const root = try std.json.parseFromSliceLeaky(std.json.Value, arena, line, .{});
    if (root != .object) return events.items;
    const rec = root.object;
    const meta: Meta = .{ .session_id = str(rec, "sessionId"), .uuid = str(rec, "uuid"), .timestamp = str(rec, "timestamp") };
    const is_user = std.mem.eql(u8, str(rec, "type"), "user");
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
