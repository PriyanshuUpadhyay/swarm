// JSON fields, event limits, and Claude message helpers.
const std = @import("std");
const root = @import("root.zig");
const Meta = root.Meta;
const Event = root.Event;
const Question = root.Question;
const Option = root.Option;
const Answer = root.Answer;

pub fn str(obj: std.json.ObjectMap, key: []const u8) []const u8 {
    const value = obj.get(key) orelse return "";
    return if (value == .string) value.string else "";
}

pub fn optionalStr(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const value = obj.get(key) orelse return null;
    return if (value == .string) value.string else null;
}

pub fn tokenCount(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    const value = obj.get(key) orelse return null;
    return if (value == .integer and value.integer >= 0) value.integer else null;
}

pub fn nonnegativeNumber(obj: std.json.ObjectMap, key: []const u8) ?f64 {
    const value = obj.get(key) orelse return null;
    const number: f64 = switch (value) {
        .integer => @floatFromInt(value.integer),
        .float => value.float,
        else => return null,
    };
    return if (std.math.isFinite(number) and number >= 0) number else null;
}

pub fn claudeInputTokens(usage: std.json.ObjectMap) ?i64 {
    const input = tokenCount(usage, "input_tokens") orelse return null;
    const read = tokenCount(usage, "cache_read_input_tokens") orelse return null;
    const write = tokenCount(usage, "cache_creation_input_tokens") orelse return null;
    const sum = std.math.add(i64, input, read) catch return null;
    return std.math.add(i64, sum, write) catch null;
}

pub fn oneOf(value: []const u8, choices: []const []const u8) bool {
    for (choices) |choice| {
        if (std.mem.eql(u8, value, choice)) return true;
    }
    return false;
}

/// A ring that `swarm` types into an agent's pane: text with a non-blank line whose every
/// non-blank line, trimmed, starts with the ring phrase. Several rings can land in one message.
pub fn isSwarmRing(text: []const u8) bool {
    var any = false;
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |line| {
        const trimmed = std.mem.trim(u8, line, " \t\r");
        if (trimmed.len == 0) continue;
        if (!std.mem.startsWith(u8, trimmed, "swarm: new message. Run swarm inbox")) return false;
        any = true;
    }
    return any;
}

pub fn claudeTextKind(rec: std.json.ObjectMap, text: []const u8) []const u8 {
    if (rec.get("isCompactSummary")) |flag| {
        if (flag == .bool and flag.bool) return "compact_summary";
    }
    if (isSwarmRing(text)) return "swarm_ring";
    const trimmed = std.mem.trimStart(u8, text, " \t\r\n");
    if (oneOfPrefix(trimmed, &.{ "<command-name>", "<command-message>", "<command-args>" })) return "command";
    if (oneOfPrefix(trimmed, &.{ "<local-command-stdout>", "<local-command-stderr>" })) return "command_output";
    if (std.mem.startsWith(u8, trimmed, "<task-notification>")) return "task_notification";
    if (std.mem.startsWith(u8, trimmed, "<bash-input>")) return "shell_input";
    if (oneOfPrefix(trimmed, &.{ "<bash-stdout>", "<bash-stderr>" })) return "shell_output";
    const is_meta = if (rec.get("isMeta")) |flag| flag == .bool and flag.bool else false;
    if (is_meta and std.mem.startsWith(u8, trimmed, "Base directory for this skill:")) return "skill_body";
    if (std.mem.startsWith(u8, trimmed, "[Request interrupted by user")) return "interrupted";
    // Claude Code writes these as user records, but the owner did not type them.
    if (oneOfPrefix(trimmed, &.{ "<system-reminder>", "<fork-boilerplate", "<local-command-caveat>" })) return "injected";
    // Another agent's message starts a turn. Claude marks a peer's message isMeta, so the origin
    // check runs before the isMeta check.
    const origin = rec.get("origin") orelse .null;
    const from_agent = origin == .object and if (optionalStr(origin.object, "kind")) |kind| !std.mem.eql(u8, kind, "human") else false;
    if (from_agent or oneOfPrefix(trimmed, &.{ "<teammate-message", "<cross-session-message" })) return "peer_message";
    if (is_meta) return "injected";
    return "";
}

pub fn oneOfPrefix(value: []const u8, prefixes: []const []const u8) bool {
    for (prefixes) |prefix| {
        if (std.mem.startsWith(u8, value, prefix)) return true;
    }
    return false;
}

pub fn claudeTextEvent(meta: Meta, rec: std.json.ObjectMap, is_user: bool, value: []const u8) Event {
    if (is_user) {
        const kind = claudeTextKind(rec, value);
        if (kind.len != 0) return .{ .system_message = .{ .meta = meta, .kind = kind, .text = value } };
        return .{ .user_message_chunk = .{ .meta = meta, .text = value } };
    }
    return .{ .agent_message_chunk = .{ .meta = meta, .text = value } };
}

pub fn parseQuestions(arena: std.mem.Allocator, input: std.json.Value) ![]Question {
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

pub fn parseAnswers(arena: std.mem.Allocator, value: std.json.Value) ![]Answer {
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
pub const max_event_input_depth = 255;

pub fn valueFitsDepth(value: std.json.Value, remaining: usize) bool {
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

pub fn unknownEvent(arena: std.mem.Allocator, meta: ?Meta, line: []const u8) !Event {
    const raw = if (std.unicode.utf8ValidateSlice(line)) line else try std.fmt.allocPrint(arena, "{f}", .{std.unicode.fmtUtf8(line)});
    return .{ .unknown = .{ .meta = meta, .raw = raw } };
}
