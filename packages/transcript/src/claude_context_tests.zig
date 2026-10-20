// Tests for claude context.
const std = @import("std");
const root = @import("root.zig");
const parseLine = root.parseLine;
const Event = root.Event;
const writeEventJson = root.writeEventJson;
const isSwarmRing = root.isSwarmRing;

test "Claude titles agent name and model become session info" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const title = try parseLine(arena_state.allocator(), "{\"type\":\"ai-title\",\"aiTitle\":\"Title\"}");
    const name = try parseLine(arena_state.allocator(), "{\"type\":\"agent-name\",\"agentName\":\"Agent\"}");
    const model = try parseLine(arena_state.allocator(), "{\"type\":\"attachment\",\"attachment\":{\"type\":\"model\",\"identity\":{\"modelId\":\"model-1\"}}}");
    try std.testing.expect(title[0].session_info.kind == .title);
    try std.testing.expectEqualStrings("Title", title[0].session_info.value);
    try std.testing.expect(name[0].session_info.kind == .agent_name);
    try std.testing.expectEqualStrings("Agent", name[0].session_info.value);
    try std.testing.expect(model[0].session_info.kind == .model);
    try std.testing.expectEqualStrings("model-1", model[0].session_info.value);
}

test "Claude system records become system messages and turn end" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const compact = try parseLine(arena_state.allocator(), "{\"type\":\"system\",\"subtype\":\"compact_boundary\",\"content\":\"summary\"}");
    const duration = try parseLine(arena_state.allocator(), "{\"type\":\"system\",\"subtype\":\"turn_duration\",\"durationMs\":123}");
    const fallback = try parseLine(arena_state.allocator(), "{\"type\":\"system\",\"subtype\":\"model_refusal_fallback\",\"content\":\"retry\"}");
    try std.testing.expectEqualStrings("compaction", compact[0].system_message.kind);
    try std.testing.expectEqual(@as(?i64, 123), duration[0].turn_ended.duration_ms);
    try std.testing.expectEqualStrings("retry", fallback[0].system_message.text);
}

test "Claude local_command records keep the command and command output kinds" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const output = try parseLine(arena_state.allocator(), "{\"type\":\"system\",\"subtype\":\"local_command\",\"parentUuid\":\"command\",\"content\":\"<local-command-stdout>Usage</local-command-stdout>\"}");
    const echo = try parseLine(arena_state.allocator(), "{\"type\":\"system\",\"subtype\":\"local_command\",\"content\":\"<command-name>/skills</command-name>\"}");
    const other = try parseLine(arena_state.allocator(), "{\"type\":\"system\",\"subtype\":\"local_command\",\"content\":\"plain\"}");
    const caveat = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"content\":\"<local-command-caveat>Caveat: do not respond</local-command-caveat>\"}}");
    try std.testing.expectEqualStrings("command_output", output[0].system_message.kind);
    try std.testing.expectEqualStrings("command", output[0].system_message.meta.parent_uuid orelse "");
    try std.testing.expectEqualStrings("command", echo[0].system_message.kind);
    try std.testing.expectEqualStrings("local_command", other[0].system_message.kind);
    try std.testing.expectEqualStrings("injected", caveat[0].system_message.kind);
}

test "Claude command wrappers and compact summary are system text" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const command = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":\"  <command-name>run\"}}");
    const output = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<local-command-stderr>bad\"}]}}");
    const summary = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"isCompactSummary\":true,\"message\":{\"content\":\"summary\"}}");
    try std.testing.expectEqualStrings("command", command[0].system_message.kind);
    try std.testing.expectEqualStrings("command_output", output[0].system_message.kind);
    try std.testing.expectEqualStrings("compact_summary", summary[0].system_message.kind);
}

test "Claude background task notification is system text, not a user prompt" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const notification = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":\"<task-notification>\\n<status>completed</status>\\n<summary>Background command done</summary>\\n</task-notification>\"}}");
    try std.testing.expectEqualStrings("task_notification", notification[0].system_message.kind);
}

test "Claude shell mode input is shell_input" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const input = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":\"<bash-input>ls</bash-input>\"}}");
    try std.testing.expectEqualStrings("shell_input", input[0].system_message.kind);
}

test "Claude shell mode stdout and stderr are shell_output" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const stdout = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":\"<bash-stdout>out</bash-stdout><bash-stderr></bash-stderr>\"}}");
    const stderr = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":\" <bash-stderr>bad</bash-stderr>\"}}");
    try std.testing.expectEqualStrings("shell_output", stdout[0].system_message.kind);
    try std.testing.expectEqualStrings("shell_output", stderr[0].system_message.kind);
}

test "Claude meta skill body is skill_body, a typed one is a user chunk" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const body = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Base directory for this skill: /skills/flow\\n\\nbody\"}]}}");
    const typed = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":\"Base directory for this skill: /skills/flow\"}}");
    try std.testing.expectEqualStrings("skill_body", body[0].system_message.kind);
    try std.testing.expectEqualStrings("Base directory for this skill: /skills/flow", typed[0].user_message_chunk.text);
}

test "Claude interrupt notice is interrupted" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const plain = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"[Request interrupted by user]\"}]}}");
    const tool = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"[Request interrupted by user for tool use]\"}]}}");
    try std.testing.expectEqualStrings("interrupted", plain[0].system_message.kind);
    try std.testing.expectEqualStrings("interrupted", tool[0].system_message.kind);
}

test "Claude meta text, a reminder, and fork rules are injected" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const meta = try parseLine(arena, "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"content\":\"Stop hook feedback: fix it\"}}");
    const reminder = try parseLine(arena, "{\"type\":\"user\",\"message\":{\"content\":\"  <system-reminder>note</system-reminder>\"}}");
    const fork = try parseLine(arena, "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<fork-boilerplate>rules</fork-boilerplate>\"}]}}");
    const peer_reminder = try parseLine(arena, "{\"type\":\"user\",\"origin\":{\"kind\":\"peer\"},\"message\":{\"content\":\"<system-reminder>note</system-reminder>\"}}");
    for ([_][]Event{ meta, reminder, fork, peer_reminder }) |events| {
        try std.testing.expectEqualStrings("injected", events[0].system_message.kind);
    }
}

test "Claude message from another agent is peer_message, also when Claude marks it meta" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const peer = try parseLine(arena, "{\"type\":\"user\",\"isMeta\":true,\"origin\":{\"kind\":\"peer\"},\"message\":{\"content\":\"Another Claude session sent a message:\\n<agent-message>hi</agent-message>\"}}");
    const coordinator = try parseLine(arena, "{\"type\":\"user\",\"origin\":{\"kind\":\"coordinator\"},\"message\":{\"content\":\"done\"}}");
    const teammate = try parseLine(arena, "{\"type\":\"user\",\"message\":{\"content\":\"<teammate-message teammate_id=\\\"t1\\\">hi</teammate-message>\"}}");
    const cross_session = try parseLine(arena, "{\"type\":\"user\",\"message\":{\"content\":\"<cross-session-message>hello</cross-session-message>\"}}");
    for ([_][]Event{ peer, coordinator, teammate, cross_session }) |events| {
        try std.testing.expectEqualStrings("peer_message", events[0].system_message.kind);
    }
}

test "Claude typed bold prompt and human-origin text stay user chunks" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const bold = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":\"<b>bold</b>\"}}");
    const human = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"origin\":{\"kind\":\"human\"},\"message\":{\"content\":\"plain\"}}");
    try std.testing.expectEqualStrings("<b>bold</b>", bold[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("plain", human[0].user_message_chunk.text);
}

test "Claude typed block and reminder block in one record split into prompt and injected" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"fix the bug\"},{\"type\":\"text\",\"text\":\"<system-reminder>note</system-reminder>\"}]}}");
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings("fix the bug", events[0].user_message_chunk.text);
    try std.testing.expectEqualStrings("injected", events[1].system_message.kind);
}

test "Claude parent and source tool ids fill meta, null when absent" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const linked = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"isMeta\":true,\"parentUuid\":\"result-1\",\"sourceToolUseID\":\"toolu_1\",\"message\":{\"content\":\"Base directory for this skill: /skills/flow\"}}");
    const unlinked = try parseLine(arena_state.allocator(), "{\"type\":\"user\",\"parentUuid\":7,\"message\":{\"content\":\"hello\"}}");
    try std.testing.expectEqualStrings("result-1", linked[0].system_message.meta.parent_uuid.?);
    try std.testing.expectEqualStrings("toolu_1", linked[0].system_message.meta.source_tool_use_id.?);
    try std.testing.expectEqual(@as(?[]const u8, null), unlinked[0].user_message_chunk.meta.parent_uuid);
    try std.testing.expectEqual(@as(?[]const u8, null), unlinked[0].user_message_chunk.meta.source_tool_use_id);
}

test "event JSON writes parent_uuid when meta has one" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try writeEventJson(&output.writer, .{ .system_message = .{
        .meta = .{ .session_id = "s1", .uuid = "output-1", .timestamp = "t", .parent_uuid = "input-1" },
        .kind = "shell_output",
        .text = "<bash-stdout>out</bash-stdout>",
    } });
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"parent_uuid\":\"input-1\"") != null);
}

test "Claude queued command and fallback block become system messages" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const queued = try parseLine(arena_state.allocator(), "{\"type\":\"attachment\",\"attachment\":{\"type\":\"queued_command\",\"prompt\":\"go\"}}");
    const fallback = try parseLine(arena_state.allocator(), "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"fallback\",\"from\":{\"model\":\"a\"},\"to\":{\"model\":\"b\"}}]}}");
    const typed = try parseLine(arena_state.allocator(), "{\"type\":\"attachment\",\"attachment\":{\"type\":\"queued_command\",\"prompt\":\"go\",\"origin\":{\"kind\":\"human\"}}}");
    try std.testing.expectEqualStrings("go", queued[0].system_message.text);
    try std.testing.expectEqualStrings("queued_command", queued[0].system_message.kind);
    try std.testing.expectEqualStrings("queued_prompt", typed[0].system_message.kind);
    try std.testing.expectEqualStrings("model_fallback", fallback[0].system_message.kind);
    try std.testing.expectEqualStrings("a -> b", fallback[0].system_message.text);
}

test "a swarm ring is every non-blank line starting with the ring phrase" {
    try std.testing.expect(isSwarmRing("swarm: new message. Run swarm inbox and read each body at /home/owner/.swarm/<body_path>."));
    try std.testing.expect(isSwarmRing("  swarm: new message. Run swarm inbox, read each body\n\n swarm: new message. Run swarm inbox and read\n"));
    try std.testing.expect(!isSwarmRing(""));
    try std.testing.expect(!isSwarmRing(" \n\t"));
    try std.testing.expect(!isSwarmRing("swarm: new message'. When it arrives: run swarm inbox"));
    try std.testing.expect(!isSwarmRing("swarm: new message. Run swarm inbox and read\nthen fix the build"));
    try std.testing.expect(!isSwarmRing("Please wait for swarm: new message. Run swarm inbox"));
}

test "Claude swarm ring is a swarm_ring system message, typed or queued" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const ring = "swarm: new message. Run swarm inbox and read each body at /home/owner/.swarm/<body_path>. Run swarm ack <seq> only after you finish that message.";
    const typed = try parseLine(arena, "{\"type\":\"user\",\"uuid\":\"u1\",\"message\":{\"role\":\"user\",\"content\":\"" ++ ring ++ "\"}}");
    const queued = try parseLine(arena, "{\"type\":\"attachment\",\"attachment\":{\"type\":\"queued_command\",\"prompt\":\"" ++ ring ++ "\",\"origin\":{\"kind\":\"human\"}}}");
    const near = try parseLine(arena, "{\"type\":\"user\",\"uuid\":\"u2\",\"message\":{\"role\":\"user\",\"content\":\"swarm: new message'. When it arrives: run swarm inbox\"}}");
    try std.testing.expectEqualStrings("swarm_ring", typed[0].system_message.kind);
    try std.testing.expectEqualStrings(ring, typed[0].system_message.text);
    try std.testing.expectEqualStrings("u1", typed[0].system_message.meta.uuid);
    try std.testing.expectEqualStrings("swarm_ring", queued[0].system_message.kind);
    try std.testing.expectEqualStrings("swarm: new message'. When it arrives: run swarm inbox", near[0].user_message_chunk.text);
}

test "unseen Claude type stays unknown" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const events = try parseLine(arena_state.allocator(), "{\"type\":\"future-record\"}");
    try std.testing.expect(events[0] == .unknown);
}

test "many unknown blocks emit one unknown beside known events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u4","timestamp":"t","message":{"content":[{"type":"image"},{"type":"text","text":"kept"},{"type":"audio"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(3, events.len);
    try std.testing.expect(events[0].image.role == .agent);
    try std.testing.expectEqualStrings("kept", events[1].agent_message_chunk.text);
    try std.testing.expectEqualStrings(line, events[2].unknown.raw);
}
