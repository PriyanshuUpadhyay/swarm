// Tests for event json.
const std = @import("std");
const root = @import("root.zig");
const writeEventJson = root.writeEventJson;
const translateWithLog = root.translateWithLog;

test "event JSON output uses a type and nested meta" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try writeEventJson(&output.writer, .{ .agent_message_chunk = .{
        .meta = .{ .session_id = "s1", .uuid = "u1", .timestamp = "t" },
        .text = "hello",
    } });
    try std.testing.expectEqualStrings(
        "{\"type\":\"agent_message_chunk\",\"text\":\"hello\",\"meta\":{\"session_id\":\"s1\",\"uuid\":\"u1\",\"timestamp\":\"t\",\"parent_uuid\":null,\"source_tool_use_id\":null}}",
        output.written(),
    );
}

test "ignored event writes its kind" {
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try writeEventJson(&out.writer, .{ .ignored = .{ .meta = .{ .session_id = "", .uuid = "", .timestamp = "" }, .kind = "mode" } });
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"kind\":\"mode\"") != null);
}

test "translate gives an ignored record its source line" {
    var reader = std.Io.Reader.fixed("{\"type\":\"mode\",\"mode\":\"plan\"}\n");
    var out: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer out.deinit();
    try translateWithLog(std.testing.allocator, .claude, "", &reader, &out.writer, null);
    try std.testing.expect(std.mem.indexOf(u8, out.written(), "\"kind\":\"mode\",\"raw\":\"{\\\"type\\\":\\\"mode\\\",\\\"mode\\\":\\\"plan\\\"}\"") != null);
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
