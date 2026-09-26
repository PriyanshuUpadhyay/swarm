const std = @import("std");
const root = @import("root.zig");

pub fn parse(arena: std.mem.Allocator, result: std.json.Value, meta: root.Meta, id: []const u8, result_count: usize) !?root.ToolDiff {
    const patch = result.object.get("structuredPatch") orelse return null;
    if (patch == .null) return null;
    if (patch != .array) return error.InvalidPatch;
    if (patch.array.items.len == 0) return null;
    const path = root.str(result.object, "filePath");
    if (result_count != 1 or id.len == 0 or path.len == 0) return error.InvalidPatch;
    const hunks = try arena.alloc(root.DiffHunk, patch.array.items.len);
    for (patch.array.items, hunks) |value, *hunk| {
        if (value != .object) return error.InvalidPatch;
        const obj = value.object;
        const lines = obj.get("lines") orelse return error.InvalidPatch;
        if (lines != .array) return error.InvalidPatch;
        const text = try arena.alloc([]const u8, lines.array.items.len);
        var old_count: i64 = 0;
        var new_count: i64 = 0;
        for (lines.array.items, text) |line, *target| {
            if (line != .string or line.string.len == 0 or std.mem.indexOfAny(u8, line.string, "\r\n") != null) return error.InvalidPatch;
            const s = line.string;
            switch (s[0]) {
                ' ' => {
                    old_count += 1;
                    new_count += 1;
                },
                '-' => old_count += 1,
                '+' => new_count += 1,
                '\\' => if (!std.mem.eql(u8, s, "\\ No newline at end of file")) return error.InvalidPatch,
                else => return error.InvalidPatch,
            }
            target.* = s;
        }
        hunk.* = .{
            .old_start = root.tokenCount(obj, "oldStart") orelse return error.InvalidPatch,
            .old_lines = root.tokenCount(obj, "oldLines") orelse return error.InvalidPatch,
            .new_start = root.tokenCount(obj, "newStart") orelse return error.InvalidPatch,
            .new_lines = root.tokenCount(obj, "newLines") orelse return error.InvalidPatch,
            .lines = text,
        };
        if (old_count != hunk.old_lines or new_count != hunk.new_lines) return error.InvalidPatch;
    }
    return .{ .meta = meta, .tool_call_id = id, .path = path, .hunks = hunks };
}

test "saved Claude Edit retains result and adds exact structured patch" {
    var state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer state.deinit();
    const arena = state.allocator();
    const events = try root.parseLine(arena, @embedFile("fixtures/claude-edit.jsonl"));
    try std.testing.expectEqual(@as(usize, 2), events.len);
    const update = events[0].tool_call_update;
    try std.testing.expectEqual(root.ToolStatus.completed, update.status);
    try std.testing.expectEqualStrings("The file /workspace/example.txt has been updated successfully.", update.content);
    const diff = events[1].tool_diff;
    try std.testing.expectEqualStrings(update.tool_call_id, diff.tool_call_id);
    try std.testing.expectEqualStrings("/workspace/example.txt", diff.path);
    try std.testing.expectEqualStrings("fixture-result", diff.meta.uuid);
    try std.testing.expectEqual(@as(i64, 445), diff.hunks[0].old_start);
    try std.testing.expectEqual(@as(i64, 6), diff.hunks[0].old_lines);
    try std.testing.expectEqual(@as(i64, 7), diff.hunks[0].new_lines);
    try std.testing.expectEqualStrings("+added line", diff.hunks[0].lines[3]);
}

test "failed absent malformed and ambiguous saved patches keep tool output" {
    for ([_][]const u8{ "failed", "absent", "null", "empty", "malformed", "ambiguous", "missing-id" }) |scenario| {
        var state = std.heap.ArenaAllocator.init(std.testing.allocator);
        defer state.deinit();
        const arena = state.allocator();
        var record = try std.json.parseFromSliceLeaky(std.json.Value, arena, @embedFile("fixtures/claude-edit.jsonl"), .{});
        const result = record.object.getPtr("toolUseResult").?;
        const content = record.object.getPtr("message").?.object.getPtr("content").?;
        const block = &content.array.items[0];
        if (std.mem.eql(u8, scenario, "failed")) {
            try block.object.put(arena, "is_error", .{ .bool = true });
        } else if (std.mem.eql(u8, scenario, "absent")) {
            _ = result.object.swapRemove("structuredPatch");
        } else if (std.mem.eql(u8, scenario, "null")) {
            try result.object.put(arena, "structuredPatch", .null);
        } else if (std.mem.eql(u8, scenario, "empty")) {
            result.object.getPtr("structuredPatch").?.array.clearRetainingCapacity();
        } else if (std.mem.eql(u8, scenario, "malformed")) {
            try result.object.getPtr("structuredPatch").?.array.items[0].object.put(arena, "oldLines", .{ .integer = 99 });
        } else if (std.mem.eql(u8, scenario, "ambiguous")) {
            const copy = block.*;
            try content.array.append(copy);
        } else {
            _ = block.object.swapRemove("tool_use_id");
        }
        const line = try std.json.Stringify.valueAlloc(arena, record, .{});
        const events = try root.parseLine(arena, line);
        try std.testing.expect(events[0] == .tool_call_update);
        try std.testing.expectEqualStrings("The file /workspace/example.txt has been updated successfully.", events[0].tool_call_update.content);
        try std.testing.expectEqual(if (std.mem.eql(u8, scenario, "failed")) root.ToolStatus.failed else .completed, events[0].tool_call_update.status);
        var unknowns: usize = 0;
        for (events) |event| {
            try std.testing.expect(event != .tool_diff);
            if (event == .unknown) {
                unknowns += 1;
                try std.testing.expectEqualStrings(line, event.unknown.raw);
            }
        }
        const invalid = root.oneOf(scenario, &.{ "malformed", "ambiguous", "missing-id" });
        try std.testing.expectEqual(@as(usize, if (invalid) 1 else 0), unknowns);
    }
}
