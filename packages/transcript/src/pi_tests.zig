// pi transcript tests.
const std = @import("std");
const root = @import("root.zig");
const parseLine = @import("pi.zig").parseLine;
const sessionIdFromPath = @import("pi.zig").sessionIdFromPath;

fn fixtureEvents(arena: std.mem.Allocator) ![]root.Event {
    var all: std.ArrayList(root.Event) = .empty;
    var lines = std.mem.splitScalar(u8, @embedFile("fixtures/pi-chat.jsonl"), '\n');
    while (lines.next()) |line| {
        if (line.len == 0) continue;
        try all.appendSlice(arena, try parseLine(arena, line));
    }
    return all.items;
}

test "a pi chat becomes rows, with no unknown record" {
    var state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer state.deinit();
    const events = try fixtureEvents(state.allocator());
    var tags: std.ArrayList(u8) = .empty;
    for (events) |event| {
        try std.testing.expect(event != .unknown);
        try tags.appendSlice(state.allocator(), @tagName(event));
        try tags.append(state.allocator(), ' ');
    }
    try std.testing.expectEqualStrings(
        "session_info session_info ignored ignored user_message_chunk " ++
            "session_info usage agent_thought_chunk tool_call tool_call_update " ++
            "session_info usage agent_message_chunk turn_ended system_message " ++
            "system_message system_message session_info usage turn_ended " ++
            "session_info usage error turn_ended system_message session_info ",
        tags.items,
    );
}

test "pi records keep their ids, tool links, and turn ends" {
    var state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer state.deinit();
    const events = try fixtureEvents(state.allocator());
    try std.testing.expectEqualStrings("/work/project", events[0].session_info.value);
    try std.testing.expectEqualStrings("Run the tests", events[4].user_message_chunk.text);
    try std.testing.expectEqualStrings("e5", events[4].user_message_chunk.meta.uuid);
    try std.testing.expectEqualStrings("e4", events[4].user_message_chunk.meta.parent_uuid.?);
    try std.testing.expectEqual(@as(?i64, 1350), events[6].usage.context_tokens);
    const call = events[8].tool_call;
    try std.testing.expectEqualStrings("bash", call.name);
    try std.testing.expectEqualStrings("cargo test", call.input.object.get("command").?.string);
    const result = events[9].tool_call_update;
    try std.testing.expectEqualStrings(call.tool_call_id, result.tool_call_id);
    try std.testing.expectEqual(root.ToolStatus.failed, result.status);
    try std.testing.expectEqualStrings("1 failed", result.content);
    try std.testing.expectEqual(.completed, events[13].turn_ended.reason);
    try std.testing.expectEqualStrings("swarm_ring", events[14].system_message.kind);
    try std.testing.expectEqualStrings("<bash-input>ls</bash-input>", events[15].system_message.text);
    try std.testing.expectEqualStrings("e9", events[16].system_message.meta.parent_uuid.?);
    try std.testing.expectEqual(.aborted, events[19].turn_ended.reason);
    try std.testing.expectEqualStrings("overloaded", events[22].@"error".message);
    // A failed reply ends the turn, or a chat whose login failed stays busy for good.
    try std.testing.expectEqual(.aborted, events[23].turn_ended.reason);
    try std.testing.expectEqualStrings("compaction", events[24].system_message.kind);
    try std.testing.expectEqual(root.SessionInfoKind.title, events[25].session_info.kind);
}

test "two text blocks of one pi reply stay apart" {
    var state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer state.deinit();
    const events = try parseLine(state.allocator(),
        \\{"type":"message","id":"a1","parentId":"u1","timestamp":"t","message":{"role":"assistant","content":[{"type":"text","text":"First."},{"type":"text","text":"Second."}],"stopReason":"toolUse"}}
    );
    // The view joins chunks of one row with no separator.
    try std.testing.expectEqualStrings("First.", events[0].agent_message_chunk.text);
    try std.testing.expectEqualStrings("\n\nSecond.", events[1].agent_message_chunk.text);
}

test "a stop that pi writes as an abort error ends the turn as interrupted, with no error row" {
    var state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer state.deinit();
    const events = try parseLine(state.allocator(),
        \\{"type":"message","id":"a1","parentId":"u1","timestamp":"t","message":{"role":"assistant","content":[],"stopReason":"error","errorMessage":"This operation was aborted"}}
    );
    try std.testing.expectEqual(@as(usize, 1), events.len);
    try std.testing.expectEqual(.aborted, events[0].turn_ended.reason);
}

test "a pi session id comes from its file name" {
    try std.testing.expectEqualStrings("01a1-b2", sessionIdFromPath("/s/--w--/2026-10-14T10-00-00-000Z_01a1-b2.jsonl"));
    try std.testing.expectEqualStrings("", sessionIdFromPath("/s/x.json"));
}

test "a pi skill message becomes a command row and the skill body it opens" {
    var state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer state.deinit();
    // The shape pi 1.2.0 `_expandSkillCommand` writes for `/skill:plan fix this now`.
    const events = try parseLine(state.allocator(),
        \\{"type":"message","id":"u1","parentId":"a0","timestamp":"t","message":{"role":"user","content":[{"type":"text","text":"<skill name=\"plan\" location=\"/s/plan/SKILL.md\">\nReferences are relative to /s/plan.\n\n# Plan\nStep one.\n</skill>\n\nfix this now"}]}}
    );
    try std.testing.expectEqual(@as(usize, 2), events.len);
    const command = events[0].system_message;
    try std.testing.expectEqualStrings("command", command.kind);
    try std.testing.expectEqualStrings("u1", command.meta.uuid);
    try std.testing.expectEqualStrings(
        "<command-message>plan</command-message>\n<command-name>/skill:plan</command-name>\n<command-args>fix this now</command-args>",
        command.text,
    );
    const body = events[1].system_message;
    try std.testing.expectEqualStrings("skill_body", body.kind);
    try std.testing.expectEqualStrings("u1", body.meta.parent_uuid.?);
    try std.testing.expectEqualStrings("References are relative to /s/plan.\n\n# Plan\nStep one.", body.text);
}

test "a pasted skill block keeps the words around it, and a broken one stays a user message" {
    var state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer state.deinit();
    const pasted = try parseLine(state.allocator(),
        \\{"type":"message","id":"u2","timestamp":"t","message":{"role":"user","content":"look at this\n<skill name=\"x\" location=\"/x\">\nbody\n</skill>\nand fix it"}}
    );
    try std.testing.expectEqualStrings(
        "<command-message>x</command-message>\n<command-name>/skill:x</command-name>\n<command-args>look at this\n\nand fix it</command-args>",
        pasted[0].system_message.text,
    );
    try std.testing.expectEqualStrings("body", pasted[1].system_message.text);
    const broken = try parseLine(state.allocator(),
        \\{"type":"message","id":"u3","timestamp":"t","message":{"role":"user","content":"<skill name=\"x\" location=\"/x\">\nno end"}}
    );
    try std.testing.expectEqualStrings("<skill name=\"x\" location=\"/x\">\nno end", broken[0].user_message_chunk.text);
}

test "a pi skill block ends where pi's own regex ends it, and needs a name" {
    var state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer state.deinit();
    // A skill that documents this format holds a `</skill>` line in its body.
    const nested = try parseLine(state.allocator(),
        \\{"type":"message","id":"u4","timestamp":"t","message":{"role":"user","content":"<skill name=\"doc\" location=\"/d\">\nwrite\n</skill>\nlike this\n</skill>\n\ngo"}}
    );
    try std.testing.expectEqualStrings("write\n</skill>\nlike this", nested[1].system_message.text);
    try std.testing.expect(std.mem.endsWith(u8, nested[0].system_message.text, "<command-args>go</command-args>"));
    const unnamed = try parseLine(state.allocator(),
        \\{"type":"message","id":"u5","timestamp":"t","message":{"role":"user","content":"<skill name=\"\" location=\"/x\">\nbody\n</skill>"}}
    );
    try std.testing.expectEqual(@as(usize, 1), unnamed.len);
    try std.testing.expect(unnamed[0] == .user_message_chunk);
}
