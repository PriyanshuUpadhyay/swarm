// Codex message and reasoning tests.
const std = @import("std");
const root = @import("root.zig");
const parseLine = @import("codex.zig").parseLine;

test "role user text parts become user chunks unless tagged context" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","timestamp":"t","payload":{"type":"message","id":"m1","role":"user","content":[{"type":"input_text","text":"one"},{"type":"input_text","text":"  # AGENTS.md rules"},{"type":"text","text":"<new_tag> two"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(3, events.len);
    try std.testing.expectEqualStrings("one", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("", events[0].user_message_chunk.meta.session_id);
    try std.testing.expectEqualStrings("m1", events[0].user_message_chunk.meta.uuid);
    try std.testing.expectEqualStrings("t", events[0].user_message_chunk.meta.timestamp);
    try std.testing.expectEqualStrings("context", events[1].system_message.kind);
    try std.testing.expectEqualStrings("  # AGENTS.md rules", events[1].system_message.text);
    try std.testing.expectEqualStrings("<new_tag> two", events[2].user_message_chunk.text);
}

test "markdown headings stay user chunks" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","timestamp":"t","payload":{"type":"message","id":"m1","role":"user","content":[{"type":"input_text","text":"# Context from my IDE"},{"type":"input_text","text":"# Validator task"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("# Context from my IDE", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("# Validator task", events[1].user_message_chunk.text);
}

test "event user message stays ignored when it has text" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"event_msg","timestamp":"t","payload":{"type":"user_message","message":"hello","images":[]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("event_msg/user_message", events[0].ignored.kind);
    try std.testing.expectEqualStrings("t", events[0].ignored.meta.timestamp);
}

test "event user message with no text stays ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const missing = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"}}");
    const empty = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"\"}}");
    try std.testing.expectEqual(1, missing.len);
    try std.testing.expectEqualStrings("event_msg/user_message", missing[0].ignored.kind);
    try std.testing.expectEqual(1, empty.len);
    try std.testing.expectEqualStrings("event_msg/user_message", empty[0].ignored.kind);
}

test "mixed user record preserves context prompt and image order" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>host"},{"type":"text","text":"show me this"},{"type":"input_image","media_type":"image/png"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(3, events.len);
    try std.testing.expectEqualStrings("context", events[0].system_message.kind);
    try std.testing.expectEqualStrings("<environment_context>host", events[0].system_message.text);
    try std.testing.expectEqualStrings("show me this", events[1].user_message_chunk.text);
    try std.testing.expect(events[2].image.role == .user);
    try std.testing.expectEqualStrings("image/png", events[2].image.media_type);
}

test "event agent message stays ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"hello\"}}");
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("event_msg/agent_message", events[0].ignored.kind);
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

test "empty reasoning summary becomes ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"reasoning","id":"r1","summary":[]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings("response_item/reasoning", events[0].ignored.kind);
}

test "empty reasoning text becomes ignored" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","payload":{"type":"reasoning","summary":[{"text":""}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expect(events[0] == .ignored);
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

test "Codex swarm ring is a swarm_ring system message, a near miss stays a user chunk" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"response_item","timestamp":"t","payload":{"type":"message","id":"m2","role":"user","content":[{"type":"input_text","text":"swarm: new message. Run swarm inbox, read each body at /home/owner/.swarm/<body_path>, then swarm ack <seq>."},{"type":"input_text","text":"swarm: new message'. When it arrives: run swarm inbox"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("swarm_ring", events[0].system_message.kind);
    try std.testing.expectEqualStrings("m2", events[0].system_message.meta.uuid);
    try std.testing.expectEqualStrings("swarm: new message'. When it arrives: run swarm inbox", events[1].user_message_chunk.text);
}
