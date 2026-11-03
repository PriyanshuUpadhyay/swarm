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

/// The question ids of each open Codex question call, so its tool output maps only for that call
/// and in question order. The `Translator` owns it across lines. A window that starts after the
/// call has no entry, so the output text decides alone there.
pub const State = struct {
    gpa: std.mem.Allocator,
    calls: std.StringHashMapUnmanaged([]u8) = .empty,

    pub fn init(gpa: std.mem.Allocator) State {
        return .{ .gpa = gpa };
    }

    pub fn deinit(state: *State) void {
        var entries = state.calls.iterator();
        while (entries.next()) |entry| {
            state.gpa.free(entry.key_ptr.*);
            state.gpa.free(entry.value_ptr.*);
        }
        state.calls.deinit(state.gpa);
    }

    /// Keeps the call's question ids, joined by newlines.
    pub fn open(state: *State, call_id: []const u8, input: std.json.Value) !void {
        var ids: std.ArrayList(u8) = .empty;
        errdefer ids.deinit(state.gpa);
        if (input == .object) if (input.object.get("questions")) |items| if (items == .array) for (items.array.items, 0..) |item, index| {
            if (index != 0) try ids.append(state.gpa, '\n');
            if (item == .object) try ids.appendSlice(state.gpa, root.str(item.object, "id"));
        };
        const value = try ids.toOwnedSlice(state.gpa);
        errdefer state.gpa.free(value);
        const entry = try state.calls.getOrPut(state.gpa, call_id);
        if (entry.found_existing) {
            state.gpa.free(entry.value_ptr.*);
        } else {
            entry.key_ptr.* = state.gpa.dupe(u8, call_id) catch |err| {
                state.calls.removeByPtr(entry.key_ptr);
                return err;
            };
        }
        entry.value_ptr.* = value;
    }

    /// The call's question ids, copied to the arena, and the call is closed. Null when the call
    /// is no open question call.
    pub fn take(state: *State, arena: std.mem.Allocator, call_id: []const u8) !?[]const []const u8 {
        const entry = state.calls.fetchRemove(call_id) orelse return null;
        defer state.gpa.free(entry.key);
        defer state.gpa.free(entry.value);
        var ids: std.ArrayList([]const u8) = .empty;
        var parts = std.mem.splitScalar(u8, entry.value, '\n');
        while (parts.next()) |id| try ids.append(arena, try arena.dupe(u8, id));
        return ids.items;
    }
};

pub const QuestionOutput = union(enum) { none, accepted, answers: []root.Answer };

/// What a Codex question tool's output says: `{"accepted":true}` from `request_user_input_async`,
/// which only says the question shows, or `{"answers":{id:{"answers":[…]}}}` from
/// `request_user_input`, which holds the owner's answer (codex-rs/core/src/tools/handlers).
/// `ids` are the open question call's ids, null when the call is unknown. `{"accepted":true}`
/// counts only for a question call, because `send_message_to_user_async` returns it too. The
/// answers follow `ids`, because Codex writes them from a HashMap.
pub fn questionOutput(arena: std.mem.Allocator, text: []const u8, ids: ?[]const []const u8) !QuestionOutput {
    // Codex's serde output is compact, so a prefix check skips the parse of other tool output.
    if (std.mem.eql(u8, text, "{\"accepted\":true}")) return if (ids != null) .accepted else .none;
    if (!std.mem.startsWith(u8, text, "{\"answers\":")) return .none;
    const value = std.json.parseFromSliceLeaky(std.json.Value, arena, text, .{}) catch return .none;
    if (value != .object or value.object.count() != 1) return .none;
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
    if (ids) |order| std.mem.sort(root.Answer, list.items, order, struct {
        fn rank(names: []const []const u8, answer: root.Answer) usize {
            for (names, 0..) |name, index| if (std.mem.eql(u8, name, answer.question)) return index;
            return names.len;
        }
        fn lessThan(names: []const []const u8, a: root.Answer, b: root.Answer) bool {
            return rank(names, a) < rank(names, b);
        }
    }.lessThan);
    return .{ .answers = list.items };
}
