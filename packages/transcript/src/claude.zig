// Claude transcript record parsing.
const std = @import("std");
const types = @import("root.zig");
const helpers = @import("event_helpers.zig");
const Meta = types.Meta;
const Event = types.Event;
const str = helpers.str;
const optionalStr = helpers.optionalStr;
const oneOf = helpers.oneOf;
const claudeTextKind = helpers.claudeTextKind;
const nonnegativeNumber = helpers.nonnegativeNumber;
const claudeInputTokens = helpers.claudeInputTokens;
const tokenCount = helpers.tokenCount;
const claudeTextEvent = helpers.claudeTextEvent;
const valueFitsDepth = helpers.valueFitsDepth;
const max_event_input_depth = helpers.max_event_input_depth;
const parseQuestions = helpers.parseQuestions;
const parseAnswers = helpers.parseAnswers;
const unknownEvent = helpers.unknownEvent;
const isSwarmRing = helpers.isSwarmRing;

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
    const meta: Meta = .{
        .session_id = str(rec, "sessionId"),
        .uuid = str(rec, "uuid"),
        .timestamp = str(rec, "timestamp"),
        .parent_uuid = optionalStr(rec, "parentUuid"),
        .source_tool_use_id = optionalStr(rec, "sourceToolUseID"),
    };
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
            // A queued message the owner typed is their prompt; CLI-made ones such as task
            // notifications stay system text.
            const origin = attachment.object.get("origin") orelse .null;
            const human = origin == .object and std.mem.eql(u8, str(origin.object, "kind"), "human");
            const prompt = str(attachment.object, "prompt");
            const prompt_kind: []const u8 = if (isSwarmRing(prompt)) "swarm_ring" else if (human) "queued_prompt" else kind;
            try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = prompt_kind, .text = prompt } });
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
            "plan_mode",              "credential_org",            "thinking_drop",
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
            const content = str(rec, "content");
            // Claude Code also writes a slash command's echo and output as local_command records.
            const command_kind = if (std.mem.eql(u8, subtype, "local_command")) claudeTextKind(rec, content) else "";
            const is_command = std.mem.eql(u8, command_kind, "command") or std.mem.eql(u8, command_kind, "command_output");
            const kind: []const u8 = if (is_command) command_kind else if (std.mem.eql(u8, subtype, "compact_boundary")) "compaction" else if (std.mem.eql(u8, subtype, "model_refusal_fallback")) "model_fallback" else subtype;
            try events.append(arena, .{ .system_message = .{ .meta = meta, .kind = kind, .text = if (content.len != 0) content else str(rec, "summary") } });
        } else {
            try events.append(arena, try unknownEvent(arena, meta, line));
        }
        return events.items;
    }
    if (std.mem.eql(u8, record_type, "cost-state")) {
        const unknown_cost = rec.get("hasUnknownModelCost") orelse .null;
        try events.append(arena, .{ .usage = .{
            .meta = meta,
            .source = .claude,
            .kind = .cost,
            .cost_usd = nonnegativeNumber(rec, "totalCostUSD"),
            .cost_completeness = if (unknown_cost == .bool)
                (if (unknown_cost.bool) .partial else .complete)
            else
                .unknown,
        } });
        return events.items;
    }
    if (std.mem.eql(u8, record_type, "queue-operation")) {
        try events.append(arena, .{ .queue_operation = .{
            .meta = meta,
            .operation = str(rec, "operation"),
            .content = optionalStr(rec, "content"),
            .reason = optionalStr(rec, "reason"),
        } });
        return events.items;
    }
    if (oneOf(record_type, &.{
        "last-prompt",               "atis-latch",               "mode",              "permission-mode", "file-history-snapshot",
        "file-history-delta",        "pr-link",                  "bridge-session",    "frame-link",      "history-suppression",
        "artifact-autoreact-ledger", "artifact-comment-monitor", "continued-in",      "worktree-state",  "relocated",
        // Claude Code 2.1.300 keeps each raw model request; the user and assistant records hold the chat.
        "api-request",               "api-request-blob",         "api-request-shape",
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
    if (!is_user) {
        const model = str(message.object, "model");
        if (model.len > 0 and !std.mem.eql(u8, model, "<synthetic>")) {
            try events.append(arena, .{ .session_info = .{ .meta = meta, .kind = .model, .value = model } });
        }
        if (!std.mem.eql(u8, model, "<synthetic>")) {
            if (message.object.get("usage")) |usage| {
                if (usage == .object) {
                    const input = claudeInputTokens(usage.object);
                    try events.append(arena, .{ .usage = .{
                        .meta = meta,
                        .source = .claude,
                        .kind = .context,
                        .context_tokens = input,
                        .input_tokens = input,
                        .output_tokens = tokenCount(usage.object, "output_tokens"),
                        .cache_read_tokens = tokenCount(usage.object, "cache_read_input_tokens"),
                        .cache_write_tokens = tokenCount(usage.object, "cache_creation_input_tokens"),
                    } });
                }
            }
        }
    }
    const content = message.object.get("content") orelse return events.items;
    if (content == .string and is_user) {
        try events.append(arena, claudeTextEvent(meta, rec, is_user, content.string));
    }
    if (content != .array) return events.items;
    var result_count: usize = 0;
    for (content.array.items) |block| {
        if (block == .object and std.mem.eql(u8, str(block.object, "type"), "tool_result")) result_count += 1;
    }
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
            const result = rec.get("toolUseResult") orelse .null;
            if (!is_error and result == .object and result.object.contains("structuredPatch")) {
                // The saved patch belongs to the record, so a multi-result record cannot
                // assign it to a tool without additional source evidence.
                const diff = @import("diff.zig").parse(arena, result, meta, str(block.object, "tool_use_id"), result_count) catch |err| switch (err) {
                    error.InvalidPatch => invalid: {
                        if (!has_unknown) {
                            try events.append(arena, try unknownEvent(arena, meta, line));
                            has_unknown = true;
                        }
                        break :invalid null;
                    },
                    else => return err,
                };
                if (diff) |value| try events.append(arena, .{ .tool_diff = value });
            }
        } else if (!has_unknown) {
            try events.append(arena, try unknownEvent(arena, meta, line));
            has_unknown = true;
        }
    }
    return events.items;
}
