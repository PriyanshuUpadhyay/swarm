const std = @import("std");
const transcript = @import("transcript");

fn fail(init: std.process.Init, message: []const u8) noreturn {
    var buffer: [256]u8 = undefined;
    var file_writer: std.Io.File.Writer = .init(.stderr(), init.io, &buffer);
    file_writer.interface.writeAll(message) catch {};
    file_writer.interface.flush() catch {};
    std.process.exit(1);
}

pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    if (args.len != 2) fail(init, "error: expected transcript <file.jsonl>\n");

    const file = std.Io.Dir.cwd().openFile(init.io, args[1], .{}) catch
        fail(init, "error: cannot read input file\n");
    defer file.close(init.io);

    var input_buffer: [64 * 1024]u8 = undefined;
    var file_reader = file.reader(init.io, &input_buffer);
    var output_buffer: [4096]u8 = undefined;
    var file_writer: std.Io.File.Writer = .init(.stdout(), init.io, &output_buffer);

    transcript.translate(init.gpa, &file_reader.interface, &file_writer.interface) catch
        fail(init, "error: failed to translate input file\n");
    try file_writer.interface.flush();
}
