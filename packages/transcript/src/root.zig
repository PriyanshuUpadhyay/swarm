const std = @import("std");

pub const Format = enum { claude, codex, agy };

pub const Meta = struct {
    session_id: []const u8,
    uuid: []const u8,
    timestamp: []const u8,
};

pub const Text = struct { meta: Meta, text: []const u8 };

pub const Ignored = struct { meta: Meta, kind: []const u8 };
pub const TurnStarted = struct { meta: Meta };
pub const TurnEnded = struct { meta: Meta, duration_ms: ?i64, reason: enum { completed, aborted } };
pub const ErrorEvent = struct { meta: Meta, message: []const u8 };
pub const SystemMessage = struct { meta: Meta, kind: []const u8, text: []const u8 };
pub const SessionInfoKind = enum { title, agent_name, model, cwd };
pub const SessionInfo = struct { meta: Meta, kind: SessionInfoKind, value: []const u8 };
pub const Image = struct { meta: Meta, role: enum { user, agent, tool }, media_type: []const u8 };

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
    ignored: Ignored,
    turn_started: TurnStarted,
    turn_ended: TurnEnded,
    @"error": ErrorEvent,
    system_message: SystemMessage,
    session_info: SessionInfo,
    image: Image,
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

pub fn str(obj: std.json.ObjectMap, key: []const u8) []const u8 {
    const value = obj.get(key) orelse return "";
    return if (value == .string) value.string else "";
}

pub fn oneOf(value: []const u8, choices: []const []const u8) bool {
    for (choices) |choice| {
        if (std.mem.eql(u8, value, choice)) return true;
    }
    return false;
}

fn claudeTextKind(rec: std.json.ObjectMap, text: []const u8) []const u8 {
    if (rec.get("isCompactSummary")) |flag| {
        if (flag == .bool and flag.bool) return "compact_summary";
    }
    const trimmed = std.mem.trimStart(u8, text, " \t\r\n");
    if (oneOfPrefix(trimmed, &.{ "<command-name>", "<command-message>", "<command-args>" })) return "command";
    if (oneOfPrefix(trimmed, &.{ "<local-command-stdout>", "<local-command-stderr>", "<local-command-caveat>" })) return "command_output";
    return "";
}

pub fn oneOfPrefix(value: []const u8, prefixes: []const []const u8) bool {
    for (prefixes) |prefix| {
        if (std.mem.startsWith(u8, value, prefix)) return true;
    }
    return false;
}

fn claudeTextEvent(meta: Meta, rec: std.json.ObjectMap, is_user: bool, value: []const u8) Event {
    if (is_user) {
        const kind = claudeTextKind(rec, value);
        if (kind.len != 0) return .{ .system_message = .{ .meta = meta, .kind = kind, .text = value } };
        return .{ .user_message_chunk = .{ .meta = meta, .text = value } };
    }
    return .{ .agent_message_chunk = .{ .meta = meta, .text = value } };
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

pub fn parseLine(arena: std.mem.Allocator, line: []const u8) ![]Event {
    var events: std.ArrayList(Event) = .empty;
    const root = std.json.parseFromSliceLeaky(std.json.Value, arena, line, .{}) catch {
        try events.append(arena, try unknownEvent(arena, null, line));
        return events.items;
    };
    if (root != .object) {
        try events.append(arena, try unknownEvent(arena, null, line));
        return events.items;
    }
    const rec = root.object;
    const meta: Meta = .{ .session_id = str(rec, "sessionId"), .uuid = str(rec, "uuid"), .timestamp = str(rec, "timestamp") };
    const record_type = str(rec, "type");
    if (std.mem.eql(u8, record_type, "attachment")) {
        const attachment = rec.get("attachment") orelse {
            try events.append(arena, try unknownEvent(arena, meta, line));
            return events.items;
        };
        if (attachment != .object) {
            try events.append(arena, try unknownEvent(arena, meta, line));
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
        } else if (std.mem.eql(u8, kind, "queued_command")) {
            try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = kind, .text = str(attachment.object, "prompt") } });
        } else if (std.mem.eql(u8, kind, "model")) {
            const identity = attachment.object.get("identity") orelse .null;
            const model = if (identity == .object) str(identity.object, "modelId") else "";
            try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .model, .value = model } });
        } else if (oneOf(kind, &.{
            "total_tokens_reminder",  "bash_output_audience_note", "batching_reminder_sent",
            "silent_turn_reminder",   "skill_listing",             "deferred_tools_delta",
            "mcp_instructions_delta", "prompt_snapshot",           "edited_text_file",
            "agent_listing_delta",    "auto_mode",                 "date",
            "session_context",        "instructions",              "environment",
            "deferred_tools_record",  "command_permissions",       "diagnostics",
            "file",                   "remote_session_change",     "nested_memory",
            "task_reminder",          "compact_file_reference",    "invoked_skills",
            "date_change",            "dynamic_skill",             "task_status",
            "plan_mode_exit",         "thinking_stripped",         "read_truncation_notice",
            "plan_mode",
        })) {
            try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = try std.fmt.allocPrint(arena, "attachment/{s}", .{kind}) } });
        } else {
            try events.append(arena, try unknownEvent(arena, meta, line));
        }
        return events.items;
    }
    if (std.mem.eql(u8, record_type, "ai-title") or std.mem.eql(u8, record_type, "custom-title")) {
        try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .title, .value = str(rec, if (std.mem.eql(u8, record_type, "ai-title")) "aiTitle" else "customTitle") } });
        return events.items;
    }
    if (std.mem.eql(u8, record_type, "agent-name")) {
        try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .agent_name, .value = str(rec, "agentName") } });
        return events.items;
    }
    if (std.mem.eql(u8, record_type, "system")) {
        const subtype = str(rec, "subtype");
        if (std.mem.eql(u8, subtype, "turn_duration")) {
            const duration = rec.get("durationMs") orelse .null;
            try events.append(arena, .{ .turn_ended = .{ .meta = meta, .reason = .completed, .duration_ms = if (duration == .integer) duration.integer else null } });
        } else if (oneOf(subtype, &.{ "compact_boundary", "away_summary", "informational", "local_command", "scheduled_task_fire", "stop_hook_summary", "model_refusal_fallback" })) {
            const kind: []const u8 = if (std.mem.eql(u8, subtype, "compact_boundary")) "compaction" else if (std.mem.eql(u8, subtype, "model_refusal_fallback")) "model_fallback" else subtype;
            const content = str(rec, "content");
            try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = kind, .text = if (content.len != 0) content else str(rec, "summary") } });
        } else {
            try events.append(arena, try unknownEvent(arena, meta, line));
        }
        return events.items;
    }
    if (oneOf(record_type, &.{
        "last-prompt",        "atis-latch",          "mode",                      "permission-mode",          "file-history-snapshot",
        "file-history-delta", "queue-operation",     "pr-link",                   "bridge-session",           "cost-state",
        "frame-link",         "history-suppression", "artifact-autoreact-ledger", "artifact-comment-monitor", "continued-in",
    })) {
        try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = record_type } });
        return events.items;
    }
    if (record_type.len == 0 and rec.contains("sid") and rec.contains("from") and rec.contains("to") and rec.contains("ts")) {
        try events.append(arena, .{ .ignored = .{ .meta = meta, .kind = "session-link" } });
        return events.items;
    }
    const is_user = std.mem.eql(u8, record_type, "user");
    if (!is_user and !std.mem.eql(u8, record_type, "assistant")) {
        try events.append(arena, try unknownEvent(arena, meta, line));
        return events.items;
    }
    const message = rec.get("message") orelse return events.items;
    if (message != .object) return events.items;
    const content = message.object.get("content") orelse return events.items;
    if (content == .string and is_user) {
        try events.append(arena, claudeTextEvent(meta, rec, is_user, content.string));
    }
    if (content != .array) return events.items;
    var has_unknown = false;
    for (content.array.items) |block| {
        if (block != .object) continue;
        const block_type = str(block.object, "type");
        if (std.mem.eql(u8, block_type, "text")) {
            try events.append(arena, claudeTextEvent(meta, rec, is_user, str(block.object, "text")));
        } else if (std.mem.eql(u8, block_type, "image")) {
            const source = block.object.get("source") orelse .null;
            const media_type = if (source == .object) str(source.object, "media_type") else "";
            try events.append(arena, .{ .image = .{ .meta = meta, .role = if (is_user) .user else .agent, .media_type = media_type } });
        } else if (std.mem.eql(u8, block_type, "fallback") and !is_user) {
            const from = block.object.get("from") orelse .null;
            const to = block.object.get("to") orelse .null;
            try events.append(arena, .{ .system_message = .{
                .meta = meta,
                .kind = "model_fallback",
                .text = try std.fmt.allocPrint(arena, "{s} -> {s}", .{
                    if (from == .object) str(from.object, "model") else "",
                    if (to == .object) str(to.object, "model") else "",
                }),
            } });
        } else if (std.mem.eql(u8, block_type, "thinking")) {
            const thinking = str(block.object, "thinking");
            if (thinking.len != 0) {
                try events.append(arena, .{ .agent_thought_chunk = .{ .meta = meta, .text = thinking } });
            }
        } else if (std.mem.eql(u8, block_type, "tool_use") and !is_user) {
            const input = block.object.get("input") orelse .null;
            if (!valueFitsDepth(input, max_event_input_depth)) {
                if (!has_unknown) {
                    try events.append(arena, try unknownEvent(arena, meta, line));
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
            try events.append(arena, try unknownEvent(arena, meta, line));
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

pub fn writePageJson(writer: *std.Io.Writer, start_offset: u64, end_offset: u64) std.Io.Writer.Error!void {
    var stringify: std.json.Stringify = .{ .writer = writer, .options = .{ .escape_unicode = true } };
    try stringify.beginObject();
    try stringify.objectField("type");
    try stringify.write("page");
    try stringify.objectField("start_offset");
    try stringify.write(start_offset);
    try stringify.objectField("end_offset");
    try stringify.write(end_offset);
    try stringify.endObject();
}

/// Returns the first byte of the last `line_count` lines in `[0, end_offset)`.
/// A newline at `end_offset - 1` ends the last line and does not start an empty line.
pub fn findTailStart(file: std.Io.File, io: std.Io, end_offset: u64, line_count: u64, buffer: []u8) !u64 {
    std.debug.assert(line_count > 0);
    std.debug.assert(buffer.len > 0);

    var cursor = end_offset;
    var boundaries: u64 = 0;
    while (cursor > 0) {
        const chunk_start = cursor - @min(cursor, @as(u64, @intCast(buffer.len)));
        const chunk_len: usize = @intCast(cursor - chunk_start);
        const read_len = try file.readPositionalAll(io, buffer[0..chunk_len], chunk_start);
        if (read_len != chunk_len) return error.EndOfStream;

        var index = read_len;
        while (index > 0) {
            index -= 1;
            if (buffer[index] != '\n') continue;
            const absolute = chunk_start + @as(u64, @intCast(index));
            if (absolute + 1 == end_offset) continue;
            boundaries += 1;
            if (boundaries == line_count) return absolute + 1;
        }
        cursor = chunk_start;
    }
    return 0;
}

pub fn agySessionIdFromPath(path: []const u8) []const u8 {
    var parts = std.mem.splitScalar(u8, path, std.fs.path.sep);
    while (parts.next()) |part| {
        if (std.mem.eql(u8, part, "brain")) {
            const session_id = parts.next() orelse return "";
            if (session_id.len != 0 and parts.next() != null) return session_id;
        }
    }
    return "";
}

const Translator = struct {
    line_buffer: std.Io.Writer.Allocating,
    arena_state: std.heap.ArenaAllocator,
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    owned_session_id: ?[]u8 = null,
    offset: u64 = 0,
    unknown_log: ?*std.Io.Writer = null,

    fn init(gpa: std.mem.Allocator, format: Format, session_id: []const u8) Translator {
        return .{
            .line_buffer = .init(gpa),
            .arena_state = .init(gpa),
            .gpa = gpa,
            .format = format,
            .session_id = session_id,
        };
    }

    fn deinit(translator: *Translator) void {
        translator.line_buffer.deinit();
        translator.arena_state.deinit();
        if (translator.owned_session_id) |session_id| translator.gpa.free(session_id);
    }

    fn translateAvailable(translator: *Translator, reader: *std.Io.Reader, writer: *std.Io.Writer, parse_final: bool) !void {
        while (true) {
            _ = try reader.streamDelimiterEnding(&translator.line_buffer.writer, '\n');
            const at_end = end: {
                const byte = reader.takeByte() catch |err| switch (err) {
                    error.EndOfStream => break :end true,
                    else => return err,
                };
                std.debug.assert(byte == '\n');
                break :end false;
            };
            if (at_end and (translator.line_buffer.written().len == 0 or !parse_final)) return;
            _ = translator.arena_state.reset(.retain_capacity);
            const line = std.mem.trimEnd(u8, translator.line_buffer.written(), "\r");
            const events = try switch (translator.format) {
                .claude => parseLine(translator.arena_state.allocator(), line),
                .codex => @import("codex.zig").parseLine(translator.arena_state.allocator(), line),
                .agy => @import("agy.zig").parseLine(translator.arena_state.allocator(), line),
            };
            if (translator.format == .codex) {
                for (events) |event| {
                    const meta = switch (event) {
                        .unknown => |unknown| unknown.meta orelse continue,
                        inline else => |value| value.meta,
                    };
                    if (meta.session_id.len == 0) continue;
                    const session_id = try translator.gpa.dupe(u8, meta.session_id);
                    if (translator.owned_session_id) |old| translator.gpa.free(old);
                    translator.owned_session_id = session_id;
                    translator.session_id = session_id;
                    break;
                }
            }
            for (events) |*event| {
                if (translator.format != .claude) {
                    switch (event.*) {
                        .unknown => |*unknown| {
                            if (unknown.meta) |*meta| meta.session_id = translator.session_id;
                        },
                        inline else => |*value| value.meta.session_id = translator.session_id,
                    }
                }
                try writeEventJson(writer, event.*);
                try writer.writeByte('\n');
                if (event.* == .unknown) translator.logUnknown(line);
            }
            try writer.flush();
            translator.offset += translator.line_buffer.written().len + @intFromBool(!at_end);
            translator.line_buffer.clearRetainingCapacity();
            if (at_end) return;
        }
    }

    fn logUnknown(translator: *Translator, line: []const u8) void {
        const log = translator.unknown_log orelse return;
        var kind: []const u8 = "-";
        var detail: []const u8 = "";
        const parsed = std.json.parseFromSliceLeaky(std.json.Value, translator.arena_state.allocator(), line, .{}) catch .null;
        if (parsed == .object) {
            const rec = parsed.object;
            const record_type = str(rec, "type");
            if (record_type.len != 0) kind = record_type;
            if (translator.format == .claude and std.mem.eql(u8, record_type, "attachment")) {
                if (rec.get("attachment")) |attachment| {
                    if (attachment == .object) detail = str(attachment.object, "type");
                }
            } else if (translator.format == .claude and std.mem.eql(u8, record_type, "system")) {
                detail = str(rec, "subtype");
            } else if (translator.format == .codex and (std.mem.eql(u8, record_type, "response_item") or std.mem.eql(u8, record_type, "event_msg"))) {
                if (rec.get("payload")) |payload| {
                    if (payload == .object) detail = str(payload.object, "type");
                }
            } else if (translator.format == .agy) {
                detail = str(rec, "status");
            }
        }
        log.print("transcript: unknown format={s} offset={d} kind=", .{ @tagName(translator.format), translator.offset }) catch return;
        for (kind) |byte| {
            log.writeByte(if (std.ascii.isAlphanumeric(byte) or byte == '_' or byte == '-' or byte == '/') byte else '_') catch return;
        }
        if (detail.len != 0) {
            log.writeByte('/') catch return;
            for (detail) |byte| {
                log.writeByte(if (std.ascii.isAlphanumeric(byte) or byte == '_' or byte == '-' or byte == '/') byte else '_') catch return;
            }
        }
        log.writeByte('\n') catch return;
        log.flush() catch {};
    }
};

fn translateFileRange(
    translator: *Translator,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
    input_buffer: []u8,
    parse_final: bool,
) !void {
    var offset = start_offset;
    while (offset < end_offset) {
        const read_len: usize = @intCast(@min(end_offset - offset, @as(u64, @intCast(input_buffer.len))));
        const actual = try file.readPositionalAll(io, input_buffer[0..read_len], offset);
        if (actual != read_len) return error.EndOfStream;
        offset += actual;
        var reader = std.Io.Reader.fixed(input_buffer[0..actual]);
        try translator.translateAvailable(&reader, writer, false);
    }
    if (parse_final) {
        var reader = std.Io.Reader.fixed("");
        try translator.translateAvailable(&reader, writer, true);
    }
}

pub fn translate(gpa: std.mem.Allocator, format: Format, session_id: []const u8, reader: *std.Io.Reader, writer: *std.Io.Writer) !void {
    try translateWithLog(gpa, format, session_id, reader, writer, null);
}

pub fn translateWithLog(gpa: std.mem.Allocator, format: Format, session_id: []const u8, reader: *std.Io.Reader, writer: *std.Io.Writer, unknown_log: ?*std.Io.Writer) !void {
    var translator: Translator = .init(gpa, format, session_id);
    defer translator.deinit();
    translator.unknown_log = unknown_log;
    try translator.translateAvailable(reader, writer, true);
}

/// Translates `[start_offset, end_offset)`, including a final line without a newline.
pub fn translateWindow(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
) !void {
    try translateWindowWithLog(gpa, format, session_id, file, io, start_offset, end_offset, writer, null);
}

pub fn translateWindowWithLog(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
    unknown_log: ?*std.Io.Writer,
) !void {
    var translator: Translator = .init(gpa, format, session_id);
    defer translator.deinit();
    translator.offset = start_offset;
    translator.unknown_log = unknown_log;
    var input_buffer: [64 * 1024]u8 = undefined;
    try translateFileRange(&translator, file, io, start_offset, end_offset, writer, &input_buffer, true);
}

pub fn translateFollow(gpa: std.mem.Allocator, format: Format, session_id: []const u8, file: std.Io.File, io: std.Io, writer: *std.Io.Writer) !void {
    try translateFollowWithLog(gpa, format, session_id, file, io, writer, null);
}

pub fn translateFollowWithLog(gpa: std.mem.Allocator, format: Format, session_id: []const u8, file: std.Io.File, io: std.Io, writer: *std.Io.Writer, unknown_log: ?*std.Io.Writer) !void {
    try translateFollowWindowWithLog(gpa, format, session_id, file, io, 0, 0, writer, unknown_log);
}

/// Translates the initial window, holds its final partial line, and follows from `end_offset`.
pub fn translateFollowWindow(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
) !void {
    try translateFollowWindowWithLog(gpa, format, session_id, file, io, start_offset, end_offset, writer, null);
}

pub fn translateFollowWindowWithLog(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
    unknown_log: ?*std.Io.Writer,
) !void {
    var translator: Translator = .init(gpa, format, session_id);
    defer translator.deinit();
    translator.offset = start_offset;
    translator.unknown_log = unknown_log;
    var input_buffer: [64 * 1024]u8 = undefined;
    try translateFileRange(&translator, file, io, start_offset, end_offset, writer, &input_buffer, false);
    var offset = end_offset;
    while (true) {
        const read_len = try file.readPositionalAll(io, &input_buffer, offset);
        if (read_len == 0) {
            if (std.c.getppid() == 1) return;
            try std.Io.sleep(io, .fromMilliseconds(200), .awake);
            continue;
        }
        offset += read_len;
        var reader = std.Io.Reader.fixed(input_buffer[0..read_len]);
        try translator.translateAvailable(&reader, writer, false);
    }
}

fn expectTailStart(content: []const u8, end_offset: u64, line_count: u64, chunk_size: usize, expected: u64) !void {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const file = try tmp.dir.createFile(std.testing.io, "input.jsonl", .{ .read = true });
    defer file.close(std.testing.io);
    var write_buffer: [128]u8 = undefined;
    var file_writer: std.Io.File.Writer = .init(file, std.testing.io, &write_buffer);
    try file_writer.interface.writeAll(content);
    try file_writer.interface.flush();

    var scan_buffer: [128]u8 = undefined;
    try std.testing.expectEqual(expected, try findTailStart(file, std.testing.io, end_offset, line_count, scan_buffer[0..chunk_size]));
}

test "backward scan handles smaller equal and larger line counts" {
    const input = "one\ntwo\nthree\n";
    try expectTailStart(input, input.len, 2, 64, 4);
    try expectTailStart(input, input.len, 3, 64, 0);
    try expectTailStart(input, input.len, 4, 64, 0);
}

test "backward scan counts a final line with or without a newline" {
    try expectTailStart("one\ntwo\nthree\n", 14, 1, 64, 8);
    try expectTailStart("one\ntwo\nthree", 13, 1, 64, 8);
}

test "backward scan crosses chunks within a long line" {
    const input = "a\n123456789\nz\n";
    try expectTailStart(input, input.len, 2, 4, 2);
}

test "backward scan treats a before offset in the middle as a line end" {
    const input = "one\ntwo\nthree\n";
    try expectTailStart(input, 6, 1, 3, 4);
}

test "backward scan handles an empty file" {
    try expectTailStart("", 0, 1, 4, 0);
}

test "AGY session id comes from brain directory" {
    try std.testing.expectEqualStrings("conv-1", agySessionIdFromPath("/work/brain/conv-1/steps.jsonl"));
    try std.testing.expectEqualStrings("conv-2", agySessionIdFromPath("brain/conv-2/logs/steps.jsonl"));
    try std.testing.expectEqualStrings("", agySessionIdFromPath("/work/steps.jsonl"));
}

test "Codex session_meta fills later event ids" {
    var reader = std.Io.Reader.fixed(
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-1\"}}\n" ++
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]}}\n",
    );
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try translate(std.testing.allocator, .codex, "", &reader, &output.writer);
    try std.testing.expectEqual(2, std.mem.count(u8, output.written(), "\"session_id\":\"codex-1\""));
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"text\":\"hello\"") != null);
}

test "AGY path id fills event meta" {
    var reader = std.Io.Reader.fixed(
        "{\"type\":\"USER_INPUT\",\"status\":\"DONE\",\"source\":\"USER_EXPLICIT\",\"step_index\":1,\"created_at\":\"t\",\"content\":\"hello\"}\n",
    );
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try translate(std.testing.allocator, .agy, agySessionIdFromPath("brain/agy-1/steps.jsonl"), &reader, &output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"session_id\":\"agy-1\"") != null);
}

test "Codex input without session_meta leaves session id empty" {
    var reader = std.Io.Reader.fixed(
        "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]}}\n",
    );
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try translate(std.testing.allocator, .codex, "", &reader, &output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"session_id\":\"\"") != null);
}

test "Claude translation keeps record session id" {
    var reader = std.Io.Reader.fixed("{\"type\":\"user\",\"sessionId\":\"claude-1\",\"message\":{\"content\":\"hello\"}}\n");
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try translate(std.testing.allocator, .claude, "", &reader, &output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"session_id\":\"claude-1\"") != null);
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

test "Claude queued command and fallback block become system messages" {
    var arena_state: std.heap.ArenaAllocator = .init(std.testing.allocator);
    defer arena_state.deinit();
    const queued = try parseLine(arena_state.allocator(), "{\"type\":\"attachment\",\"attachment\":{\"type\":\"queued_command\",\"prompt\":\"go\"}}");
    const fallback = try parseLine(arena_state.allocator(), "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"fallback\",\"from\":{\"model\":\"a\"},\"to\":{\"model\":\"b\"}}]}}");
    try std.testing.expectEqualStrings("go", queued[0].system_message.text);
    try std.testing.expectEqualStrings("model_fallback", fallback[0].system_message.kind);
    try std.testing.expectEqualStrings("a -> b", fallback[0].system_message.text);
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

test "ignored event writes its kind" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .ignored = .{ .meta = .{ .session_id = "", .uuid = "", .timestamp = "" }, .kind = "mode" } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"kind\":\"mode\"") != null);
}

test "turn started event writes meta" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .turn_started = .{ .meta = .{ .session_id = "s", .uuid = "", .timestamp = "" } } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"type\":\"turn_started\"") != null);
}

test "turn ended event writes duration and reason" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .turn_ended = .{ .meta = .{ .session_id = "", .uuid = "", .timestamp = "" }, .duration_ms = 42, .reason = .completed } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"duration_ms\":42,\"reason\":\"completed\"") != null);
}

test "error event writes message" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .@"error" = .{ .meta = .{ .session_id = "", .uuid = "", .timestamp = "" }, .message = "bad" } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"type\":\"error\",\"message\":\"bad\"") != null);
}

test "system message writes kind and text" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .system_message = .{ .meta = .{ .session_id = "", .uuid = "", .timestamp = "" }, .kind = "compaction", .text = "summary" } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"kind\":\"compaction\",\"text\":\"summary\"") != null);
}

test "session info writes enum kind and value" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .session_info = .{ .meta = .{ .session_id = "", .uuid = "", .timestamp = "" }, .kind = .model, .value = "m" } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"kind\":\"model\",\"value\":\"m\"") != null);
}

test "image writes role and media type without source data" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .image = .{ .meta = .{ .session_id = "", .uuid = "", .timestamp = "" }, .role = .user, .media_type = "image/png" } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"role\":\"user\",\"media_type\":\"image/png\"") != null);
}

test "unknown log gives format offset and kind" {
    var reader = std.Io.Reader.fixed("{\"type\":\"user\",\"message\":{\"content\":\"ok\"}}\n{\"type\":\"new_record\"}\n");
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    var log: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer log.deinit();
    try translateWithLog(std.testing.allocator, .claude, "", &reader, &out.writer, &log.writer);
    try std.testing.expectEqualStrings("transcript: unknown format=claude offset=43 kind=new_record\n", log.written());
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

test "follow translation holds a final piece until its newline arrives" {
    var translator: Translator = .init(std.testing.allocator, .claude, "");
    defer translator.deinit();
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();

    var first = std.Io.Reader.fixed(
        "{\"type\":\"user\",\"message\":{\"content\":\"first\"}}\n" ++
            "{\"type\":\"user\",\"message\":{\"content\":\"sec",
    );
    try translator.translateAvailable(&first, &output.writer, false);
    try std.testing.expectEqual(1, std.mem.count(u8, output.written(), "\n"));
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "unknown") == null);

    var second = std.Io.Reader.fixed("ond\"}}\n");
    try translator.translateAvailable(&second, &output.writer, false);
    try std.testing.expectEqual(2, std.mem.count(u8, output.written(), "\n"));
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "unknown") == null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "second") != null);
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
    try translate(outer_arena.allocator(), .claude, "", &reader, &output.writer);

    try std.testing.expectEqual(32, std.mem.count(u8, output.written(), "\n"));
}

test "translate flushes each input line" {
    const BufferedSink = struct {
        const Self = @This();

        sink: std.Io.Writer.Allocating,
        writer: std.Io.Writer,
        buffer: [1024]u8,

        fn init(self: *Self) void {
            self.* = .{
                .sink = .init(std.testing.allocator),
                .writer = undefined,
                .buffer = undefined,
            };
            self.writer = .{ .vtable = &.{ .drain = drain }, .buffer = &self.buffer };
        }

        fn deinit(self: *Self) void {
            self.sink.deinit();
        }

        fn drain(writer: *std.Io.Writer, data: []const []const u8, splat: usize) std.Io.Writer.Error!usize {
            const self: *Self = @fieldParentPtr("writer", writer);
            try self.sink.writer.writeAll(writer.buffered());
            writer.end = 0;
            var written: usize = 0;
            for (data[0 .. data.len - 1]) |slice| {
                try self.sink.writer.writeAll(slice);
                written += slice.len;
            }
            for (0..splat) |_| {
                try self.sink.writer.writeAll(data[data.len - 1]);
                written += data[data.len - 1].len;
            }
            return written;
        }
    };

    var reader = std.Io.Reader.fixed("{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n");
    var output: BufferedSink = undefined;
    output.init();
    defer output.deinit();

    try translate(std.testing.allocator, .claude, "", &reader, &output.writer);

    try std.testing.expectEqualStrings(
        "{\"type\":\"user_message_chunk\",\"text\":\"hello\",\"meta\":{\"session_id\":\"\",\"uuid\":\"\",\"timestamp\":\"\"}}\n",
        output.sink.written(),
    );
}
