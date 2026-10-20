// Tests for translation stream.
const std = @import("std");
const root = @import("root.zig");
const translate = root.translate;
const translation = @import("translation.zig");
const Translator = translation.Translator;

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
        "{\"type\":\"user_message_chunk\",\"text\":\"hello\",\"meta\":{\"session_id\":\"\",\"uuid\":\"\",\"timestamp\":\"\",\"parent_uuid\":null,\"source_tool_use_id\":null}}\n",
        output.sink.written(),
    );
}
