const std = @import("std");

pub const Format = enum { claude, codex, agy, pi };

pub const Meta = struct {
    session_id: []const u8,
    uuid: []const u8,
    timestamp: []const u8,
    parent_uuid: ?[]const u8 = null,
    source_tool_use_id: ?[]const u8 = null,
};

pub const Text = struct { meta: Meta, text: []const u8 };

/// `raw` is the whole source line; the translator fills it, so the app can show the record.
pub const Ignored = struct { meta: Meta, kind: []const u8, raw: []const u8 = "" };
pub const TurnStarted = struct { meta: Meta };
pub const TurnEnded = struct { meta: Meta, duration_ms: ?i64, reason: enum { completed, aborted } };
pub const ErrorEvent = struct { meta: Meta, message: []const u8 };
pub const SystemMessage = struct { meta: Meta, kind: []const u8, text: []const u8 };
pub const SessionInfoKind = enum { title, agent_name, model, cwd };
pub const SessionInfo = struct { meta: Meta, kind: SessionInfoKind, value: []const u8 };
pub const Usage = struct {
    meta: Meta,
    source: enum { claude, codex, pi },
    kind: enum { context, cost },
    context_tokens: ?i64 = null,
    context_capacity_tokens: ?i64 = null,
    input_tokens: ?i64 = null,
    output_tokens: ?i64 = null,
    cache_read_tokens: ?i64 = null,
    cache_write_tokens: ?i64 = null,
    session_input_tokens: ?i64 = null,
    session_output_tokens: ?i64 = null,
    cost_usd: ?f64 = null,
    cost_completeness: enum { complete, partial, unknown } = .unknown,
};
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

pub const DiffHunk = struct {
    old_start: i64,
    old_lines: i64,
    new_start: i64,
    new_lines: i64,
    lines: []const []const u8,
};

pub const ToolDiff = struct {
    meta: Meta,
    tool_call_id: []const u8,
    path: []const u8,
    hunks: []const DiffHunk,
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

/// `operation` is an open set (enqueue, dequeue, remove, popAll so far); `dequeue` has no content.
pub const QueueOperation = struct {
    meta: Meta,
    operation: []const u8,
    content: ?[]const u8,
    reason: ?[]const u8,
};

pub const Event = union(enum) {
    ignored: Ignored,
    turn_started: TurnStarted,
    turn_ended: TurnEnded,
    @"error": ErrorEvent,
    system_message: SystemMessage,
    session_info: SessionInfo,
    usage: Usage,
    image: Image,
    user_message_chunk: Text,
    agent_message_chunk: Text,
    agent_thought_chunk: Text,
    tool_call: ToolCall,
    tool_call_update: ToolCallUpdate,
    tool_diff: ToolDiff,
    elicitation: Elicitation,
    elicitation_result: ElicitationResult,
    hook_result: HookResult,
    permission_decision: PermissionDecision,
    queue_operation: QueueOperation,
    unknown: Unknown,
};

const helpers = @import("event_helpers.zig");
pub const str = helpers.str;
pub const tokenCount = helpers.tokenCount;
pub const oneOf = helpers.oneOf;
pub const isSwarmRing = helpers.isSwarmRing;
pub const oneOfPrefix = helpers.oneOfPrefix;
pub const max_event_input_depth = helpers.max_event_input_depth;
pub const valueFitsDepth = helpers.valueFitsDepth;
pub const unknownEvent = helpers.unknownEvent;

pub const parseLine = @import("claude.zig").parseLine;
const translation = @import("translation.zig");
pub const writeEventJson = translation.writeEventJson;
pub const writePageJson = translation.writePageJson;
pub const findTailStart = translation.findTailStart;
pub const piSessionIdFromPath = translation.piSessionIdFromPath;
pub const agySessionIdFromPath = translation.agySessionIdFromPath;
pub const translate = translation.translate;
pub const translateWithLog = translation.translateWithLog;
pub const translateWindow = translation.translateWindow;
pub const translateWindowWithLog = translation.translateWindowWithLog;
pub const translateFollow = translation.translateFollow;
pub const translateFollowWithLog = translation.translateFollowWithLog;
pub const translateFollowWindow = translation.translateFollowWindow;
pub const translateFollowWindowWithLog = translation.translateFollowWindowWithLog;
pub const translateFollowFromWithLog = translation.translateFollowFromWithLog;

test {
    _ = @import("translation_window_tests.zig");
    _ = @import("claude_message_tests.zig");
    _ = @import("claude_context_tests.zig");
    _ = @import("claude_tool_tests.zig");
    _ = @import("event_json_tests.zig");
    _ = @import("translation_stream_tests.zig");
    _ = @import("codex_message_tests.zig");
    _ = @import("codex_tool_tests.zig");
}
