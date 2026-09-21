const std = @import("std");
const transcript = @import("transcript");

fn fail(init: std.process.Init, message: []const u8) noreturn {
    var buffer: [256]u8 = undefined;
    var file_writer: std.Io.File.Writer = .initStreaming(.stderr(), init.io, &buffer);
    file_writer.interface.writeAll(message) catch {};
    file_writer.interface.flush() catch {};
    std.process.exit(1);
}

pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    const follow = args.len == 3 and std.mem.eql(u8, args[1], "--follow");
    const once = args.len == 2 and !std.mem.eql(u8, args[1], "--follow");
    if (!once and !follow) fail(init, "usage: transcript [--follow] <file.jsonl>\n");
    const path = if (follow) args[2] else args[1];

    const file = std.Io.Dir.cwd().openFile(init.io, path, .{}) catch
        fail(init, "error: cannot read input file\n");
    defer file.close(init.io);

    var output_buffer: [4096]u8 = undefined;
    var file_writer: std.Io.File.Writer = .initStreaming(.stdout(), init.io, &output_buffer);

    if (follow) {
        transcript.translateFollow(init.gpa, file, init.io, &file_writer.interface) catch
            fail(init, "error: failed to translate input file\n");
    } else {
        var input_buffer: [64 * 1024]u8 = undefined;
        var file_reader = file.reader(init.io, &input_buffer);
        transcript.translate(init.gpa, &file_reader.interface, &file_writer.interface) catch
            fail(init, "error: failed to translate input file\n");
    }
    file_writer.interface.flush() catch fail(init, "error: failed to write output\n");
}
