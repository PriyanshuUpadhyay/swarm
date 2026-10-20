// Tests for translation window.
const std = @import("std");
const root = @import("root.zig");
const translateWindow = root.translateWindow;
const findTailStart = root.findTailStart;
const agySessionIdFromPath = root.agySessionIdFromPath;
const translate = root.translate;
const translation = @import("translation.zig");
const Translator = translation.Translator;
const fromWindow = translation.fromWindow;

test "from window emits only later fixture events and clamps beyond the end" {
    const fixture = @embedFile("fixtures/claude-chair-run.jsonl");
    const start = std.mem.lastIndexOfScalar(u8, fixture[0 .. fixture.len / 2], '\n').? + 1;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const file = try tmp.dir.createFile(std.testing.io, "input.jsonl", .{ .read = true });
    defer file.close(std.testing.io);
    var write_buffer: [128]u8 = undefined;
    var file_writer: std.Io.File.Writer = .init(file, std.testing.io, &write_buffer);
    try file_writer.interface.writeAll(fixture);
    try file_writer.interface.flush();
    var expected: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer expected.deinit();
    try translateWindow(std.testing.allocator, .claude, "", file, std.testing.io, start, fixture.len, &expected.writer);

    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var translator: Translator = .init(std.testing.allocator, .claude, "");
    defer translator.deinit();
    var buffer: [128]u8 = undefined;
    try std.testing.expectEqual(fixture.len, try fromWindow(&translator, file, std.testing.io, start, &output.writer, &buffer));
    var events: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer events.deinit();
    var lines = std.mem.splitScalar(u8, output.written(), '\n');
    const first = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, lines.next().?, .{});
    defer first.deinit();
    try std.testing.expectEqual(@as(i64, @intCast(start)), first.value.object.get("start_offset").?.integer);
    try std.testing.expectEqual(@as(i64, @intCast(fixture.len)), first.value.object.get("end_offset").?.integer);
    while (lines.next()) |line| {
        if (line.len == 0 or std.mem.startsWith(u8, line, "{\"type\":\"page\"")) continue;
        try events.writer.writeAll(line);
        try events.writer.writeByte('\n');
    }
    try std.testing.expectEqualStrings(expected.written(), events.written());
    try std.testing.expect(events.written().len > 0);
    output.clearRetainingCapacity();
    var past: Translator = .init(std.testing.allocator, .claude, "");
    defer past.deinit();
    _ = try fromWindow(&past, file, std.testing.io, fixture.len + 1000, &output.writer, &buffer);
    try std.testing.expectEqual(1, std.mem.count(u8, output.written(), "\n"));
    const page = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, std.mem.trimEnd(u8, output.written(), "\n"), .{});
    defer page.deinit();
    try std.testing.expectEqual(@as(i64, @intCast(fixture.len)), page.value.object.get("start_offset").?.integer);
    try std.testing.expectEqual(@as(i64, @intCast(fixture.len)), page.value.object.get("end_offset").?.integer);
}

test "from progress advances only past complete lines" {
    const complete = "{\"type\":\"user\",\"message\":{\"content\":\"first\"}}\n";
    var translator: Translator = .init(std.testing.allocator, .claude, "");
    defer translator.deinit();
    translator.offset = 10;
    translator.progress_pages = true;
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var first = std.Io.Reader.fixed(complete ++ "{\"type\":\"user\"");
    try translator.translateAvailable(&first, &output.writer, false);
    try std.testing.expectEqual(10 + complete.len, translator.offset);
    try std.testing.expectEqual(1, std.mem.count(u8, output.written(), "\"type\":\"page\""));
    var second = std.Io.Reader.fixed(",\"message\":{\"content\":\"second\"}}\n");
    try translator.translateAvailable(&second, &output.writer, false);
    try std.testing.expectEqual(2, std.mem.count(u8, output.written(), "\"type\":\"page\""));
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
