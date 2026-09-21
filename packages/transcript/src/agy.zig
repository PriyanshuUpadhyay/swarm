const std = @import("std");
const root = @import("root.zig");

pub fn parseLine(arena: std.mem.Allocator, line: []const u8) ![]root.Event {
    const events = try arena.alloc(root.Event, 1);
    events[0] = try root.unknownEvent(arena, null, line);
    return events;
}

test "agy translation makes the line unknown" {
    var reader = std.Io.Reader.fixed("agy line\n");
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try root.translate(std.testing.allocator, .agy, &reader, &output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"type\":\"unknown\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"raw\":\"agy line\"") != null);
}
