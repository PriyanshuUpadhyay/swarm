// Tests for claude message.
const std = @import("std");
const root = @import("root.zig");
const parseLine = root.parseLine;

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

test "empty thinking blocks yield no events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","message":{"content":[{"type":"thinking","thinking":""}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(0, events.len);
}

test "unknown record keeps raw line and meta" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"future-kind","sessionId":"s1","uuid":"u3","timestamp":"t"}
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

test "invalid UTF-8 becomes replacement text in unknown raw" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "caf\xc3");
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("caf\xef\xbf\xbd", events[0].unknown.raw);
}

test "unknown content block becomes unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u4","timestamp":"t","message":{"content":[{"type":"audio","source":"x"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}

test "user text survives an image block" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"user","sessionId":"s1","message":{"content":[{"type":"text","text":"first"},{"type":"image","source":"x"},{"type":"text","text":"last"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(3, events.len);
    try std.testing.expectEqualStrings("first", events[0].user_message_chunk.text);
    try std.testing.expect(events[1].image.role == .user);
    try std.testing.expectEqualStrings("last", events[2].user_message_chunk.text);
}

test "Claude bookkeeping and session link are ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const mode = try parseLine(arena_state.allocator(), "{\"type\":\"mode\"}");
    const link = try parseLine(arena_state.allocator(), "{\"sid\":\"s\",\"from\":\"a\",\"to\":\"b\",\"ts\":\"t\"}");
    const attachment = try parseLine(arena_state.allocator(), "{\"type\":\"attachment\",\"attachment\":{\"type\":\"total_tokens_reminder\"}}");
    try std.testing.expectEqualStrings("mode", mode[0].ignored.kind);
    try std.testing.expectEqualStrings("session-link", link[0].ignored.kind);
    try std.testing.expectEqualStrings("attachment/total_tokens_reminder", attachment[0].ignored.kind);
}

test "Claude credential, worktree, and relocation records are ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const credential = try parseLine(arena_state.allocator(), "{\"type\":\"attachment\",\"uuid\":\"u1\",\"attachment\":{\"type\":\"credential_org\",\"organizationUuid\":\"org-1\"}}");
    const worktree = try parseLine(arena_state.allocator(), "{\"type\":\"worktree-state\",\"worktreeSession\":{\"worktreePath\":\"/work/wt\",\"enteredExisting\":true},\"sessionId\":\"s1\"}");
    const relocated = try parseLine(arena_state.allocator(), "{\"type\":\"relocated\",\"sessionId\":\"s1\",\"relocatedCwd\":\"/work/wt\"}");
    try std.testing.expectEqualStrings("attachment/credential_org", credential[0].ignored.kind);
    try std.testing.expectEqualStrings("worktree-state", worktree[0].ignored.kind);
    try std.testing.expectEqualStrings("relocated", relocated[0].ignored.kind);
}

test "Claude 2.1.300 request records and dropped thinking are ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    for ([_][]const u8{ "api-request", "api-request-blob", "api-request-shape" }) |kind| {
        const line = try std.fmt.allocPrint(arena, "{{\"type\":\"{s}\",\"sessionId\":\"s1\",\"version\":\"2.1.300\"}}", .{kind});
        const events = try parseLine(arena, line);
        try std.testing.expectEqualStrings(kind, events[0].ignored.kind);
    }
    const drop = try parseLine(arena, "{\"type\":\"attachment\",\"attachment\":{\"type\":\"thinking_drop\",\"thinkingBlocksSent\":1}}");
    try std.testing.expectEqualStrings("attachment/thinking_drop", drop[0].ignored.kind);
}

test "Claude assistant model is reported without treating synthetic messages as models" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-4-6\",\"content\":[{\"type\":\"text\",\"text\":\"Hello\"}]}}");
    try std.testing.expectEqual(@as(usize, 2), events.len);
    try std.testing.expectEqualStrings("claude-opus-4-6", events[0].session_info.value);
    try std.testing.expectEqualStrings("Hello", events[1].agent_message_chunk.text);
    const synthetic = try parseLine(arena_state.allocator(), "{\"type\":\"assistant\",\"message\":{\"model\":\"<synthetic>\",\"content\":[{\"type\":\"text\",\"text\":\"No response\"}]}}");
    try std.testing.expectEqual(@as(usize, 1), synthetic.len);
    try std.testing.expect(synthetic[0] == .agent_message_chunk);
}

test "Claude context includes cache while cost snapshots remain separate" {
    var arena: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena.deinit();
    const events = try parseLine(arena.allocator(),
        \\{"type":"assistant","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":80,"cache_creation_input_tokens":20,"output_tokens":7},"content":[]}}
    );
    try std.testing.expectEqual(@as(?i64, 110), events[0].usage.context_tokens);
    try std.testing.expectEqual(@as(?i64, 7), events[0].usage.output_tokens);
    try std.testing.expectEqual(@as(?i64, null), events[0].usage.context_capacity_tokens);
    const cost = try parseLine(arena.allocator(),
        \\{"type":"cost-state","totalCostUSD":0,"hasUnknownModelCost":true}
    );
    try std.testing.expectEqual(@as(?f64, 0), cost[0].usage.cost_usd);
    try std.testing.expect(cost[0].usage.cost_completeness == .partial);
    const missing = try parseLine(arena.allocator(),
        \\{"type":"assistant","message":{"usage":{"input_tokens":10,"output_tokens":7},"content":[]}}
    );
    try std.testing.expectEqual(@as(?i64, null), missing[0].usage.context_tokens);
    const invalid = try parseLine(arena.allocator(),
        \\{"type":"cost-state","totalCostUSD":-1}
    );
    try std.testing.expectEqual(@as(?f64, null), invalid[0].usage.cost_usd);
}

test "Claude queue operations keep operation, content, and reason" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const enqueue = try parseLine(arena, "{\"type\":\"queue-operation\",\"operation\":\"enqueue\",\"timestamp\":\"t1\",\"sessionId\":\"s1\",\"content\":\"add a test\"}");
    const dequeue = try parseLine(arena, "{\"type\":\"queue-operation\",\"operation\":\"dequeue\",\"timestamp\":\"t2\",\"sessionId\":\"s1\"}");
    const remove = try parseLine(arena, "{\"type\":\"queue-operation\",\"operation\":\"remove\",\"timestamp\":\"t3\",\"sessionId\":\"s1\",\"content\":\"add a test\",\"reason\":\"absorbed_mid_turn\",\"commandUuid\":\"c1\",\"deliveryId\":\"d1\"}");
    const pop_all = try parseLine(arena, "{\"type\":\"queue-operation\",\"operation\":\"popAll\",\"timestamp\":\"t4\",\"sessionId\":\"s1\",\"content\":\"keep the old name\"}");
    try std.testing.expectEqualStrings("enqueue", enqueue[0].queue_operation.operation);
    try std.testing.expectEqualStrings("add a test", enqueue[0].queue_operation.content.?);
    try std.testing.expectEqual(@as(?[]const u8, null), enqueue[0].queue_operation.reason);
    try std.testing.expectEqualStrings("s1", enqueue[0].queue_operation.meta.session_id);
    try std.testing.expectEqualStrings("dequeue", dequeue[0].queue_operation.operation);
    try std.testing.expectEqual(@as(?[]const u8, null), dequeue[0].queue_operation.content);
    try std.testing.expectEqualStrings("remove", remove[0].queue_operation.operation);
    try std.testing.expectEqualStrings("add a test", remove[0].queue_operation.content.?);
    try std.testing.expectEqualStrings("absorbed_mid_turn", remove[0].queue_operation.reason.?);
    try std.testing.expectEqualStrings("popAll", pop_all[0].queue_operation.operation);
    try std.testing.expectEqualStrings("keep the old name", pop_all[0].queue_operation.content.?);
}
