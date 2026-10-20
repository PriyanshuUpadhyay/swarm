// Tests for claude tool.
const std = @import("std");
const root = @import("root.zig");
const parseLine = root.parseLine;
const ToolStatus = root.ToolStatus;
const str = root.str;
const max_event_input_depth = root.max_event_input_depth;

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

test "tool input deeper than the JSON writer limit becomes unknown" {
    var input: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer input.deinit();
    try input.writer.writeAll("{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"deep\",\"name\":\"Read\",\"input\":");
    for (0..max_event_input_depth + 1) |_| try input.writer.writeByte('[');
    try input.writer.writeByte('0');
    for (0..max_event_input_depth + 1) |_| try input.writer.writeByte(']');
    try input.writer.writeAll("}]}}");

    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), input.written());
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(input.written(), events[0].unknown.raw);
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

test "AskUserQuestion becomes elicitation" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u8","timestamp":"t","message":{"content":[{"type":"tool_use","id":"tool-3","name":"AskUserQuestion","input":{"questions":[{"question":"Pick one","header":"Choice","multiSelect":true,"options":[{"label":"A","description":"first"},{"label":"B","description":"second"}]}]}}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("tool-3", events[0].elicitation.tool_call_id);
    try std.testing.expectEqual(1, events[0].elicitation.questions.len);
    try std.testing.expect(events[0].elicitation.questions[0].multi_select);
    try std.testing.expectEqualStrings("B", events[0].elicitation.questions[0].options[1].label);
}

test "elicitation answer replaces tool call update" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"user","sessionId":"s1","uuid":"u9","timestamp":"t","message":{"content":[{"type":"tool_result","tool_use_id":"tool-3","content":"answer"}]},"toolUseResult":{"questions":[],"answers":{"Pick one":"A"},"annotations":{}}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("tool-3", events[0].elicitation_result.tool_call_id);
    try std.testing.expectEqualStrings("Pick one", events[0].elicitation_result.answers[0].question);
    try std.testing.expectEqualStrings("A", events[0].elicitation_result.answers[0].answer);
}

test "non-object elicitation answers keep the tool call update" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tool-3","content":"failed answer","is_error":true}]},"toolUseResult":{"answers":null}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqual(ToolStatus.failed, events[0].tool_call_update.status);
    try std.testing.expectEqualStrings("failed answer", events[0].tool_call_update.content);
}

test "AskUserQuestion missing fields keeps false and empty defaults" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u10","timestamp":"t","message":{"content":[{"type":"tool_use","id":"tool-4","name":"AskUserQuestion","input":{"questions":[{}]}}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expect(!events[0].elicitation.questions[0].multi_select);
    try std.testing.expectEqual(0, events[0].elicitation.questions[0].options.len);
}

test "hook attachment becomes hook result" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"attachment","sessionId":"s1","uuid":"u11","timestamp":"t","attachment":{"type":"hook_success","hookName":"SessionStart:startup","hookEvent":"SessionStart","toolUseID":"tool-5","exitCode":0}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("hook_success", events[0].hook_result.kind);
    try std.testing.expectEqualStrings("SessionStart", events[0].hook_result.hook_event);
    try std.testing.expectEqual(@as(?i64, 0), events[0].hook_result.exit_code);
}

test "hook result permits a missing exit code" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"attachment","sessionId":"s1","uuid":"u12","timestamp":"t","attachment":{"type":"hook_cancelled","hookName":"PreToolUse","hookEvent":"PreToolUse","toolUseID":"tool-6"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expect(events[0].hook_result.exit_code == null);
}

test "permission decision keeps an unknown decision string" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"attachment","sessionId":"s1","uuid":"u13","timestamp":"t","attachment":{"type":"hook_permission_decision","decision":"later","toolUseID":"tool-7","hookEvent":"PermissionRequest"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("later", events[0].permission_decision.decision);
}

test "unknown attachment type becomes unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"attachment","sessionId":"s1","uuid":"u14","timestamp":"t","attachment":{"type":"other"}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}
