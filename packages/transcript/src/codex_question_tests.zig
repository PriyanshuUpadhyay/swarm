const std = @import("std");
const codex = @import("codex.zig");
const q = @import("codex_questions.zig");

fn parse(arena: std.mem.Allocator, state: *q.State, line: []const u8) ![]@import("root.zig").Event {
    return codex.parseLineState(arena, line, state);
}

test "Codex accepted output hides only for a question call" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    var state = q.State.init(std.testing.allocator);
    defer state.deinit();
    // `send_message_to_user_async` returns the same `{"accepted":true}` as the async question.
    _ = try parse(arena, &state,
        \\{"type":"response_item","payload":{"type":"function_call","call_id":"call-m","name":"send_message_to_user_async","arguments":"{\"message\":\"hi\"}"}}
    );
    const message = try parse(arena, &state,
        \\{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-m","output":"{\"accepted\":true}"}}
    );
    try std.testing.expectEqualStrings("{\"accepted\":true}", message[0].tool_call_update.content);
    _ = try parse(arena, &state,
        \\{"type":"response_item","payload":{"type":"function_call","call_id":"call-a","name":"request_user_input_async","arguments":"{\"questions\":[{\"id\":\"go\",\"title\":\"Go?\"}]}"}}
    );
    const accepted = try parse(arena, &state,
        \\{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-a","output":"{\"accepted\":true}"}}
    );
    try std.testing.expectEqualStrings("question_accepted", accepted[0].ignored.kind);
}

test "Codex blocking answers follow the question order" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    var state = q.State.init(std.testing.allocator);
    defer state.deinit();
    _ = try parse(arena, &state,
        \\{"type":"response_item","payload":{"type":"function_call","call_id":"call-b","name":"request_user_input","arguments":"{\"questions\":[{\"id\":\"a\",\"question\":\"First?\"},{\"id\":\"b\",\"question\":\"Second?\"}]}"}}
    );
    // Codex writes the answers from a HashMap, so their text order is not the question order.
    const answers = try parse(arena, &state,
        \\{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-b","output":"{\"answers\":{\"b\":{\"answers\":[\"B1\"]},\"a\":{\"answers\":[\"A1\"]}}}"}}
    );
    try std.testing.expectEqualStrings("A1", answers[0].elicitation_result.answers[0].answer);
    try std.testing.expectEqualStrings("B1", answers[0].elicitation_result.answers[1].answer);
}
