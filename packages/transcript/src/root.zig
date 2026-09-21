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

pub const Option = struct {
    label: []const u8,
    description: []const u8,
};

pub const Question = struct {
    question: []const u8,
    header: []const u8,
    multi_select: bool,
    options: []Option,
};

pub const Answer = struct {
    question: []const u8,
    answer: []const u8,
};

pub const Elicitation = struct {
    meta: Meta,
    tool_call_id: []const u8,
    questions: []Question,
};

pub const ElicitationResult = struct {
    meta: Meta,
    tool_call_id: []const u8,
    answers: []Answer,
};

pub const HookResult = struct {
    meta: Meta,
    kind: []const u8,
    hook_event: []const u8,
    hook_name: []const u8,
    tool_call_id: []const u8,
    exit_code: ?i64,
};

pub const PermissionDecision = struct {
    meta: Meta,
    hook_event: []const u8,
    tool_call_id: []const u8,
    decision: []const u8,
};

pub const Event = union(enum) {
    user_message_chunk: Text,
    agent_message_chunk: Text,
    agent_thought_chunk: Text,
    tool_call: ToolCall,
    tool_call_update: ToolCallUpdate,
    elicitation: Elicitation,
    elicitation_result: ElicitationResult,
    hook_result: HookResult,
    permission_decision: PermissionDecision,
    unknown: Unknown,
};

fn str(obj: std.json.ObjectMap, key: []const u8) []const u8 {
    const value = obj.get(key) orelse return "";
    return if (value == .string) value.string else "";
}

fn parseQuestions(arena: std.mem.Allocator, input: std.json.Value) ![]Question {
    var questions: std.ArrayList(Question) = .empty;
    if (input != .object) return questions.items;
    const value = input.object.get("questions") orelse return questions.items;
    if (value != .array) return questions.items;
    for (value.array.items) |item| {
        if (item != .object) continue;
        var options: std.ArrayList(Option) = .empty;
        if (item.object.get("options")) |option_value| {
            if (option_value == .array) {
                for (option_value.array.items) |option| {
                    if (option != .object) continue;
                    try options.append(arena, .{
                        .label = str(option.object, "label"),
                        .description = str(option.object, "description"),
                    });
                }
            }
        }
        const multi_select = if (item.object.get("multiSelect")) |field| field == .bool and field.bool else false;
        try questions.append(arena, .{
            .question = str(item.object, "question"),
            .header = str(item.object, "header"),
            .multi_select = multi_select,
            .options = options.items,
        });
    }
    return questions.items;
}

fn parseAnswers(arena: std.mem.Allocator, value: std.json.Value) ![]Answer {
    var answers: std.ArrayList(Answer) = .empty;
    if (value != .object) return answers.items;
    var iterator = value.object.iterator();
    while (iterator.next()) |entry| {
        if (entry.value_ptr.* != .string) continue;
        try answers.append(arena, .{ .question = entry.key_ptr.*, .answer = entry.value_ptr.string });
    }
    return answers.items;
}

// The event object uses one of Stringify's 256 nesting levels, so input can use 255.
const max_event_input_depth = 255;

fn valueFitsDepth(value: std.json.Value, remaining: usize) bool {
    switch (value) {
        .array => |array| {
            if (remaining == 0) return false;
            for (array.items) |item| {
                if (!valueFitsDepth(item, remaining - 1)) return false;
            }
        },
        .object => |object| {
            if (remaining == 0) return false;
            var iterator = object.iterator();
            while (iterator.next()) |entry| {
                if (!valueFitsDepth(entry.value_ptr.*, remaining - 1)) return false;
            }
        },
        else => {},
    }
    return true;
}

pub fn parseLine(arena: std.mem.Allocator, line: []const u8) ![]Event {
    var events: std.ArrayList(Event) = .empty;
    const root = std.json.parseFromSliceLeaky(std.json.Value, arena, line, .{}) catch {
        const raw = if (std.unicode.utf8ValidateSlice(line)) line else try std.fmt.allocPrint(arena, "{f}", .{std.unicode.fmtUtf8(line)});
        try events.append(arena, .{ .unknown = .{ .meta = null, .raw = raw } });
        return events.items;
    };
    if (root != .object) {
        try events.append(arena, .{ .unknown = .{ .meta = null, .raw = line } });
        return events.items;
    }
    const rec = root.object;
    const meta: Meta = .{ .session_id = str(rec, "sessionId"), .uuid = str(rec, "uuid"), .timestamp = str(rec, "timestamp") };
    const record_type = str(rec, "type");
    if (std.mem.eql(u8, record_type, "attachment")) {
        const attachment = rec.get("attachment") orelse {
            try events.append(arena, .{ .unknown = .{ .meta = meta, .raw = line } });
            return events.items;
        };
        if (attachment != .object) {
            try events.append(arena, .{ .unknown = .{ .meta = meta, .raw = line } });
            return events.items;
        }
        const kind = str(attachment.object, "type");
        const is_hook_result = std.mem.eql(u8, kind, "hook_success") or
            std.mem.eql(u8, kind, "hook_non_blocking_error") or
            std.mem.eql(u8, kind, "hook_blocking_error") or
            std.mem.eql(u8, kind, "hook_cancelled") or
            std.mem.eql(u8, kind, "hook_additional_context");
        if (is_hook_result) {
            const exit_code = if (attachment.object.get("exitCode")) |value| if (value == .integer) value.integer else null else null;
            try events.append(arena, .{ .hook_result = .{
                .meta = meta,
                .kind = kind,
                .hook_event = str(attachment.object, "hookEvent"),
                .hook_name = str(attachment.object, "hookName"),
                .tool_call_id = str(attachment.object, "toolUseID"),
                .exit_code = exit_code,
            } });
        } else if (std.mem.eql(u8, kind, "hook_permission_decision")) {
            try events.append(arena, .{ .permission_decision = .{
                .meta = meta,
                .hook_event = str(attachment.object, "hookEvent"),
                .tool_call_id = str(attachment.object, "toolUseID"),
                .decision = str(attachment.object, "decision"),
            } });
        } else {
            try events.append(arena, .{ .unknown = .{ .meta = meta, .raw = line } });
        }
        return events.items;
    }
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
    var has_unknown = false;
    for (content.array.items) |block| {
        if (block != .object) continue;
        const block_type = str(block.object, "type");
        if (std.mem.eql(u8, block_type, "text")) {
            const chunk: Text = .{ .meta = meta, .text = str(block.object, "text") };
            try events.append(arena, if (is_user) .{ .user_message_chunk = chunk } else .{ .agent_message_chunk = chunk });
        } else if (std.mem.eql(u8, block_type, "thinking")) {
            try events.append(arena, .{ .agent_thought_chunk = .{ .meta = meta, .text = str(block.object, "thinking") } });
        } else if (std.mem.eql(u8, block_type, "tool_use") and !is_user) {
            const input = block.object.get("input") orelse .null;
            if (!valueFitsDepth(input, max_event_input_depth)) {
                if (!has_unknown) {
                    try events.append(arena, .{ .unknown = .{ .meta = meta, .raw = line } });
                    has_unknown = true;
                }
                continue;
            }
            if (std.mem.eql(u8, str(block.object, "name"), "AskUserQuestion")) {
                try events.append(arena, .{ .elicitation = .{
                    .meta = meta,
                    .tool_call_id = str(block.object, "id"),
                    .questions = try parseQuestions(arena, input),
                } });
            } else {
                try events.append(arena, .{ .tool_call = .{
                    .meta = meta,
                    .tool_call_id = str(block.object, "id"),
                    .name = str(block.object, "name"),
                    .input = input,
                    .status = .pending,
                } });
            }
        } else if (std.mem.eql(u8, block_type, "tool_result") and is_user) {
            if (rec.get("toolUseResult")) |tool_use_result| {
                if (tool_use_result == .object) {
                    if (tool_use_result.object.get("answers")) |answers| {
                        if (answers == .object) {
                            try events.append(arena, .{ .elicitation_result = .{
                                .meta = meta,
                                .tool_call_id = str(block.object, "tool_use_id"),
                                .answers = try parseAnswers(arena, answers),
                            } });
                            continue;
                        }
                    }
                }
            }
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
        } else if (!has_unknown) {
            try events.append(arena, .{ .unknown = .{ .meta = meta, .raw = line } });
            has_unknown = true;
        }
    }
    return events.items;
}

pub fn writeEventJson(writer: *std.Io.Writer, event: Event) std.Io.Writer.Error!void {
    // Escape all non-ASCII code points so byte 0x0A is the only line separator.
    var stringify: std.json.Stringify = .{ .writer = writer, .options = .{ .escape_unicode = true } };
    try stringify.beginObject();
    try stringify.objectField("type");
    try stringify.write(@tagName(event));
    switch (event) {
        inline else => |value| {
            inline for (@typeInfo(@TypeOf(value)).@"struct".fields) |field| {
                if (comptime !std.mem.eql(u8, field.name, "meta")) {
                    try stringify.objectField(field.name);
                    try stringify.write(@field(value, field.name));
                }
            }
            try stringify.objectField("meta");
            try stringify.write(value.meta);
        },
    }
    try stringify.endObject();
}

pub fn translate(gpa: std.mem.Allocator, reader: *std.Io.Reader, writer: *std.Io.Writer) !void {
    var line_buffer: std.Io.Writer.Allocating = .init(gpa);
    defer line_buffer.deinit();
    var arena_state: std.heap.ArenaAllocator = .init(gpa);
    defer arena_state.deinit();
    while (true) {
        line_buffer.clearRetainingCapacity();
        _ = arena_state.reset(.retain_capacity);
        _ = try reader.streamDelimiterEnding(&line_buffer.writer, '\n');
        const at_end = end: {
            const byte = reader.takeByte() catch |err| switch (err) {
                error.EndOfStream => break :end true,
                else => return err,
            };
            std.debug.assert(byte == '\n');
            break :end false;
        };
        if (line_buffer.written().len == 0 and at_end) break;
        const line = std.mem.trimEnd(u8, line_buffer.written(), "\r");
        const events = try parseLine(arena_state.allocator(), line);
        for (events) |event| {
            try writeEventJson(writer, event);
            try writer.writeByte('\n');
        }
        if (at_end) break;
    }
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
        \\{"type":"assistant","sessionId":"s1","uuid":"u4","timestamp":"t","message":{"content":[{"type":"image","source":"x"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(1, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
}

test "many unknown blocks emit one unknown beside known events" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const line =
        \\{"type":"assistant","sessionId":"s1","uuid":"u4","timestamp":"t","message":{"content":[{"type":"image"},{"type":"text","text":"kept"},{"type":"audio"}]}}
    ;
    const events = try parseLine(arena_state.allocator(), line);
    try std.testing.expectEqual(2, events.len);
    try std.testing.expectEqualStrings(line, events[0].unknown.raw);
    try std.testing.expectEqualStrings("kept", events[1].agent_message_chunk.text);
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

test "event JSON output uses a type and nested meta" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try writeEventJson(&output.writer, .{ .agent_message_chunk = .{
        .meta = .{ .session_id = "s1", .uuid = "u1", .timestamp = "t" },
        .text = "hello",
    } });
    try std.testing.expectEqualStrings(
        "{\"type\":\"agent_message_chunk\",\"text\":\"hello\",\"meta\":{\"session_id\":\"s1\",\"uuid\":\"u1\",\"timestamp\":\"t\"}}",
        output.written(),
    );
}

test "event JSON output escapes all non-ASCII code points" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try writeEventJson(&output.writer, .{ .agent_message_chunk = .{
        .meta = .{ .session_id = "", .uuid = "", .timestamp = "" },
        .text = "\x7f\u{009b}\u{0085}\u{2028}\u{2029}\u{202e}",
    } });
    for (output.written()) |byte| try std.testing.expect(byte < 0x80);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\\u007f\\u009b\\u0085\\u2028\\u2029\\u202e") != null);
}

test "translate reuses its per-line arena" {
    var input_storage: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer input_storage.deinit();
    for (0..32) |_| {
        try input_storage.writer.writeAll("{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n");
    }
    var reader = std.Io.Reader.fixed(input_storage.written());
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();

    var fixed_buffer: [16 * 1024]u8 = undefined;
    var fixed = std.heap.FixedBufferAllocator.init(&fixed_buffer);
    var outer_arena: std.heap.ArenaAllocator = .init(fixed.allocator());
    defer outer_arena.deinit();
    try translate(outer_arena.allocator(), &reader, &output.writer);

    try std.testing.expectEqual(32, std.mem.count(u8, output.written(), "\n"));
}
