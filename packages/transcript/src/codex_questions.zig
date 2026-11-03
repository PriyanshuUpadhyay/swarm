// Codex question records and image parts, mapped to transcript events.
const std = @import("std");
const root = @import("root.zig");

/// A Codex image part's media type, or the type its `data:` URL names when Codex 0.162 leaves
/// `media_type` out, as it does for a tool screenshot.
pub fn imageMediaType(part: std.json.ObjectMap) []const u8 {
    const media_type = root.str(part, "media_type");
    if (media_type.len != 0) return media_type;
    const url = root.str(part, "image_url");
    if (!std.mem.startsWith(u8, url, "data:")) return "";
    const rest = url["data:".len..];
    return rest[0 .. std.mem.indexOfAny(u8, rest, ";,") orelse return ""];
}

/// The questions of `request_user_input_async` (`title`, options as strings) or of
/// `request_user_input` (`question`, `header`, options as `{label, description}`).
pub fn questions(arena: std.mem.Allocator, input: std.json.Value) ![]root.Question {
    var list: std.ArrayList(root.Question) = .empty;
    if (input != .object) return list.items;
    const items = input.object.get("questions") orelse return list.items;
    if (items != .array) return list.items;
    for (items.array.items) |item| {
        if (item != .object) continue;
        var options: std.ArrayList(root.Option) = .empty;
        if (item.object.get("options")) |values| if (values == .array) for (values.array.items) |option| {
            switch (option) {
                .string => |label| try options.append(arena, .{ .label = label, .description = "" }),
                .object => |object| try options.append(arena, .{ .label = root.str(object, "label"), .description = root.str(object, "description") }),
                else => {},
            }
        };
        const question = root.str(item.object, "question");
        try list.append(arena, .{
            .question = if (question.len != 0) question else root.str(item.object, "title"),
            .header = root.str(item.object, "header"),
            .multi_select = false,
            .options = options.items,
        });
    }
    return list.items;
}

pub const QuestionReply = struct { tool_call_id: []const u8, answers: []root.Answer };

/// The owner's answer to an async question, as the Codex TUI and desktop send it:
/// `<send_user_message_question_reply>[{answer, question, questionItemId}]</…>`, where
/// `questionItemId` is the JSON text `["request_user_input_async", call_id, index]`
/// (codex-rs/tui/src/async_question_reply.rs). Null when the text is not a whole reply.
pub fn questionReply(arena: std.mem.Allocator, text: []const u8) !?QuestionReply {
    const open = "<send_user_message_question_reply>";
    const close = "</send_user_message_question_reply>";
    const trimmed = std.mem.trimEnd(u8, text, " \t\r\n");
    if (!std.mem.startsWith(u8, trimmed, open) or !std.mem.endsWith(u8, trimmed, close)) return null;
    if (trimmed.len < open.len + close.len) return null;
    const body = trimmed[open.len .. trimmed.len - close.len];
    const value = std.json.parseFromSliceLeaky(std.json.Value, arena, body, .{}) catch return null;
    const items: []const std.json.Value = switch (value) {
        .array => |array| array.items,
        // One reply may come bare; copy it into the arena so the slice outlives this switch.
        .object => try arena.dupe(std.json.Value, &.{value}),
        else => return null,
    };
    if (items.len == 0) return null;
    var answers: std.ArrayList(root.Answer) = .empty;
    var tool_call_id: []const u8 = "";
    for (items) |item| {
        if (item != .object) return null;
        const id = std.json.parseFromSliceLeaky(std.json.Value, arena, root.str(item.object, "questionItemId"), .{}) catch return null;
        if (id != .array or id.array.items.len < 2 or id.array.items[1] != .string) return null;
        if (tool_call_id.len == 0) tool_call_id = id.array.items[1].string;
        try answers.append(arena, .{ .question = root.str(item.object, "question"), .answer = root.str(item.object, "answer") });
    }
    return .{ .tool_call_id = tool_call_id, .answers = answers.items };
}

pub const QuestionOutput = union(enum) { none, accepted, answers: []root.Answer };

/// What a Codex question tool's output says: `{"accepted":true}` from `request_user_input_async`,
/// which only says the question shows, or `{"answers":{id:{"answers":[…]}}}` from
/// `request_user_input`, which holds the owner's answer (codex-rs/core/src/tools/handlers).
pub fn questionOutput(arena: std.mem.Allocator, text: []const u8) !QuestionOutput {
    const value = std.json.parseFromSliceLeaky(std.json.Value, arena, text, .{}) catch return .none;
    if (value != .object or value.object.count() != 1) return .none;
    if (value.object.get("accepted")) |accepted| return if (accepted == .bool and accepted.bool) .accepted else .none;
    const answers = value.object.get("answers") orelse return .none;
    if (answers != .object) return .none;
    var list: std.ArrayList(root.Answer) = .empty;
    var entries = answers.object.iterator();
    while (entries.next()) |entry| {
        const picked = if (entry.value_ptr.* == .object) entry.value_ptr.object.get("answers") orelse return .none else return .none;
        if (picked != .array) return .none;
        var text_parts: std.ArrayList([]const u8) = .empty;
        for (picked.array.items) |item| {
            if (item != .string) return .none;
            try text_parts.append(arena, item.string);
        }
        try list.append(arena, .{ .question = entry.key_ptr.*, .answer = try std.mem.join(arena, "\n", text_parts.items) });
    }
    return .{ .answers = list.items };
}
