const std = @import("std");
const root = @import("root.zig");

/// One line of a pi session file (`docs/session-format.md`, version 3). Entries form a tree by
/// `id` and `parentId`; lines are read in file order, so an abandoned branch shows too.
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
    const rec = value.object;
    const parent = rec.get("parentId") orelse .null;
    const meta: root.Meta = .{
        .session_id = "",
        .uuid = root.str(rec, "id"),
        .timestamp = root.str(rec, "timestamp"),
        .parent_uuid = if (parent == .string) parent.string else null,
    };
    const record_type = root.str(rec, "type");
    if (std.mem.eql(u8, record_type, "session")) {
        try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .cwd, .value = root.str(rec, "cwd") } });
    } else if (std.mem.eql(u8, record_type, "model_change")) {
        try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .model, .value = root.str(rec, "modelId") } });
    } else if (std.mem.eql(u8, record_type, "session_info")) {
        try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .title, .value = root.str(rec, "name") } });
    } else if (std.mem.eql(u8, record_type, "compaction")) {
        try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "compaction", .text = root.str(rec, "summary") } });
    } else if (std.mem.eql(u8, record_type, "branch_summary")) {
        try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "branch_summary", .text = root.str(rec, "summary") } });
    } else if (std.mem.eql(u8, record_type, "custom_message")) {
        try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = "injected", .text = try joinText(arena, rec.get("content") orelse .null) } });
    } else if (root.oneOf(record_type, &.{ "thinking_level_change", "custom", "label", "context_edit", "usage" })) {
        try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = record_type } });
    } else if (std.mem.eql(u8, record_type, "message")) {
        const message = rec.get("message") orelse .null;
        if (message != .object) {
            try events.append(arena, try root.unknownEvent(arena, meta, line));
        } else {
            try parseMessage(arena, &events, meta, message.object, line);
        }
    } else {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
    }
    return events.items;
}

fn parseMessage(arena: std.mem.Allocator, events: *std.ArrayList(root.Event), meta: root.Meta, message: std.json.ObjectMap, line: []const u8) !void {
    const role = root.str(message, "role");
    const content = message.get("content") orelse .null;
    if (std.mem.eql(u8, role, "user")) {
        const text = try joinText(arena, content);
        if (text.len != 0 and !try appendSkill(arena, events, meta, text)) {
            try events.append(arena, if (root.isSwarmRing(text))
                .{ .system_message = .{ .meta = meta, .kind = "swarm_ring", .text = text } }
            else
                .{ .user_message_chunk = .{ .meta = meta, .text = text } });
        }
        try appendImages(arena, events, meta, content, .user);
    } else if (std.mem.eql(u8, role, "assistant")) {
        try parseAssistant(arena, events, meta, message, content, line);
    } else if (std.mem.eql(u8, role, "toolResult")) {
        const is_error = message.get("isError") orelse .null;
        try events.append(arena, .{ .tool_call_update = .{
            .meta = meta,
            .tool_call_id = root.str(message, "toolCallId"),
            .status = if (is_error == .bool and is_error.bool) .failed else .completed,
            .content = try joinText(arena, content),
        } });
        try appendImages(arena, events, meta, content, .tool);
    } else if (std.mem.eql(u8, role, "bashExecution")) {
        // A `!command` the user ran. The view pairs the output with its input by `parent_uuid`.
        var output_meta = meta;
        output_meta.parent_uuid = meta.uuid;
        const exit_code = message.get("exitCode") orelse .null;
        try events.append(arena, .{ .system_message = .{
            .meta = meta,
            .kind = "shell_input",
            .text = try std.fmt.allocPrint(arena, "<bash-input>{s}</bash-input>", .{root.str(message, "command")}),
        } });
        try events.append(arena, .{ .system_message = .{
            .meta = output_meta,
            .kind = "shell_output",
            .text = if (exit_code == .integer)
                try std.fmt.allocPrint(arena, "<bash-stdout>{s}</bash-stdout><bash-exit-code>{d}</bash-exit-code>", .{ root.str(message, "output"), exit_code.integer })
            else
                try std.fmt.allocPrint(arena, "<bash-stdout>{s}</bash-stdout>", .{root.str(message, "output")}),
        } });
    } else if (root.oneOf(role, &.{ "system", "custom", "branchSummary", "compactionSummary" })) {
        // The prompt and context entries; a summary shows from its own compaction entry.
        try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = try std.fmt.allocPrint(arena, "message/{s}", .{role}) } });
    } else {
        try events.append(arena, try root.unknownEvent(arena, meta, line));
    }
}

fn parseAssistant(arena: std.mem.Allocator, events: *std.ArrayList(root.Event), meta: root.Meta, message: std.json.ObjectMap, content: std.json.Value, line: []const u8) !void {
    const model = root.str(message, "model");
    if (model.len != 0) try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .model, .value = model } });
    if (message.get("usage")) |usage| {
        if (usage == .object) {
            const input = root.tokenCount(usage.object, "input");
            const read = root.tokenCount(usage.object, "cacheRead");
            const write = root.tokenCount(usage.object, "cacheWrite");
            try events.append(arena, .{
                .usage = .{
                    .meta = meta,
                    .source = .pi,
                    .kind = .context,
                    // Pi counts cached input apart from `input`, as Claude does.
                    .context_tokens = if (input != null and read != null and write != null) input.? + read.? + write.? else null,
                    .input_tokens = input,
                    .output_tokens = root.tokenCount(usage.object, "output"),
                    .cache_read_tokens = read,
                    .cache_write_tokens = write,
                },
            });
        }
    }
    var called_tool = false;
    var has_unknown = false;
    var wrote_text = false;
    if (content == .array) {
        for (content.array.items) |block| {
            const block_type = if (block == .object) root.str(block.object, "type") else "";
            if (std.mem.eql(u8, block_type, "text")) {
                const text = root.str(block.object, "text");
                if (text.len == 0) continue;
                // Every block of one reply shares its id, and the view joins chunks of one row
                // with no separator, so a later block starts a new paragraph.
                const chunk = if (wrote_text) try std.mem.concat(arena, u8, &.{ "\n\n", text }) else text;
                try events.append(arena, .{ .agent_message_chunk = .{ .meta = meta, .text = chunk } });
                wrote_text = true;
            } else if (std.mem.eql(u8, block_type, "thinking")) {
                const thinking = root.str(block.object, "thinking");
                if (thinking.len != 0) try events.append(arena, .{ .agent_thought_chunk = .{ .meta = meta, .text = thinking } });
            } else if (std.mem.eql(u8, block_type, "toolCall")) {
                const input = block.object.get("arguments") orelse .null;
                if (!root.valueFitsDepth(input, root.max_event_input_depth)) {
                    has_unknown = true;
                    continue;
                }
                try events.append(arena, .{ .tool_call = .{
                    .meta = meta,
                    .tool_call_id = root.str(block.object, "id"),
                    .name = root.str(block.object, "name"),
                    .input = input,
                    .status = .pending,
                } });
                called_tool = true;
            } else {
                has_unknown = true;
            }
        }
    } else if (content != .null) {
        has_unknown = true;
    }
    if (has_unknown) try events.append(arena, try root.unknownEvent(arena, meta, line));
    const stop = root.str(message, "stopReason");
    // An owner's stop can reach pi as Node's AbortError, which some providers write as an error.
    const user_abort = std.mem.eql(u8, stop, "error") and
        root.oneOf(root.str(message, "errorMessage"), &.{ "This operation was aborted", "Operation aborted" });
    if (std.mem.eql(u8, stop, "error") and !user_abort) {
        // A failed reply that pi gives up on, such as a failed login, writes nothing after it, so
        // it ends the turn. A retry that works adds its reply after this turn end.
        try events.append(arena, .{ .@"error" = .{ .meta = meta, .message = root.str(message, "errorMessage") } });
        try events.append(arena, .{ .turn_ended = .{ .meta = meta, .duration_ms = null, .reason = .aborted } });
    } else if (user_abort or std.mem.eql(u8, stop, "aborted")) {
        try events.append(arena, .{ .turn_ended = .{ .meta = meta, .duration_ms = null, .reason = .aborted } });
    } else if (!called_tool and root.oneOf(stop, &.{ "stop", "length" })) {
        try events.append(arena, .{ .turn_ended = .{ .meta = meta, .duration_ms = null, .reason = .completed } });
    }
}

/// A `<skill name="N" location="L">` block that pi expanded from `/skill:N` (pi 1.2.0
/// `_expandSkillCommand`), or that the owner pasted, becomes the Claude-shaped `command` and
/// `skill_body` events, so the view shows the owner's words and the skill behind a disclosure.
fn appendSkill(arena: std.mem.Allocator, events: *std.ArrayList(root.Event), meta: root.Meta, text: []const u8) !bool {
    const open = "<skill name=\"";
    const start = std.mem.indexOf(u8, text, open) orelse return false;
    const head = text[start + open.len ..];
    const name_end = std.mem.indexOf(u8, head, "\" location=\"") orelse return false;
    if (name_end == 0) return false;
    const tag_end = std.mem.indexOf(u8, head, "\">\n") orelse return false;
    const body_start = tag_end + "\">\n".len;
    const close = "\n</skill>";
    const rest = head[body_start..];
    // pi's regex ends the body at the first close that ends the text or comes before `\n\n`, so a
    // body may hold a `</skill>` line. A pasted block may end anywhere, so it falls back to the first.
    var body_len = std.mem.indexOf(u8, rest, close) orelse return false;
    var next: ?usize = body_len;
    while (next) |at| : (next = std.mem.indexOfPos(u8, rest, at + 1, close)) {
        const tail = rest[at + close.len ..];
        if (tail.len == 0 or std.mem.startsWith(u8, tail, "\n\n")) {
            body_len = at;
            break;
        }
    }
    const name = head[0..name_end];
    const before = std.mem.trim(u8, text[0..start], " \t\r\n");
    const after = std.mem.trim(u8, rest[body_len + close.len ..], " \t\r\n");
    const words = if (before.len != 0 and after.len != 0)
        try std.fmt.allocPrint(arena, "{s}\n\n{s}", .{ before, after })
    else if (before.len != 0) before else after;
    try events.append(arena, .{ .system_message = .{
        .meta = meta,
        .kind = "command",
        .text = try std.fmt.allocPrint(arena, "<command-message>{s}</command-message>\n<command-name>/skill:{s}</command-name>\n<command-args>{s}</command-args>", .{ name, name, words }),
    } });
    var body_meta = meta;
    body_meta.parent_uuid = meta.uuid;
    try events.append(arena, .{ .system_message = .{ .meta = body_meta, .kind = "skill_body", .text = rest[0..body_len] } });
    return true;
}

/// The text blocks of a string or block-array content, joined.
fn joinText(arena: std.mem.Allocator, content: std.json.Value) ![]const u8 {
    if (content == .string) return content.string;
    if (content != .array) return "";
    var parts: std.ArrayList([]const u8) = .empty;
    for (content.array.items) |block| {
        if (block == .object and std.mem.eql(u8, root.str(block.object, "type"), "text")) {
            try parts.append(arena, root.str(block.object, "text"));
        }
    }
    return std.mem.join(arena, "\n", parts.items);
}

fn appendImages(arena: std.mem.Allocator, events: *std.ArrayList(root.Event), meta: root.Meta, content: std.json.Value, role: @FieldType(root.Image, "role")) !void {
    if (content != .array) return;
    for (content.array.items) |block| {
        if (block == .object and std.mem.eql(u8, root.str(block.object, "type"), "image")) {
            try events.append(arena, .{ .image = .{ .meta = meta, .role = role, .media_type = root.str(block.object, "mimeType") } });
        }
    }
}

/// The session id in a pi file name, `<time>_<id>.jsonl`.
pub fn sessionIdFromPath(path: []const u8) []const u8 {
    const name = std.fs.path.basename(path);
    if (!std.mem.endsWith(u8, name, ".jsonl")) return "";
    const stem = name[0 .. name.len - ".jsonl".len];
    const cut = std.mem.lastIndexOfScalar(u8, stem, '_') orelse return "";
    return stem[cut + 1 ..];
}

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
