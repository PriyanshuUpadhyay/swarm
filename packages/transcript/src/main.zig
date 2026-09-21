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
    const usage = "usage: transcript [--follow] [--format claude|codex|agy] [--tail N] [--before OFFSET] <file.jsonl>\n";
    var follow = false;
    var format: transcript.Format = .claude;
    var has_format = false;
    var tail: ?u64 = null;
    var before: ?u64 = null;
    var index: usize = 1;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--follow")) {
            if (follow) fail(init, usage);
            follow = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--format")) {
            if (has_format or index + 1 >= args.len) fail(init, usage);
            const name = args[index + 1];
            if (std.mem.eql(u8, name, "claude")) {
                format = .claude;
            } else if (std.mem.eql(u8, name, "codex")) {
                format = .codex;
            } else if (std.mem.eql(u8, name, "agy")) {
                format = .agy;
            } else {
                fail(init, usage);
            }
            has_format = true;
            index += 2;
        } else if (std.mem.eql(u8, args[index], "--tail")) {
            if (tail != null or index + 1 >= args.len) fail(init, usage);
            const line_count = std.fmt.parseUnsigned(u64, args[index + 1], 10) catch fail(init, usage);
            if (line_count == 0) fail(init, usage);
            tail = line_count;
            index += 2;
        } else if (std.mem.eql(u8, args[index], "--before")) {
            if (before != null or index + 1 >= args.len) fail(init, usage);
            before = std.fmt.parseUnsigned(u64, args[index + 1], 10) catch fail(init, usage);
            index += 2;
        } else {
            break;
        }
    }
    if (index + 1 != args.len or std.mem.startsWith(u8, args[index], "-")) fail(init, usage);
    if ((before != null and tail == null) or (before != null and follow)) fail(init, usage);
    const path = args[index];

    const file = std.Io.Dir.cwd().openFile(init.io, path, .{}) catch
        fail(init, "error: cannot read input file\n");
    defer file.close(init.io);

    var output_buffer: [4096]u8 = undefined;
    var file_writer: std.Io.File.Writer = .initStreaming(.stdout(), init.io, &output_buffer);

    if (tail) |line_count| {
        const file_size = (file.stat(init.io) catch fail(init, "error: failed to translate input file\n")).size;
        const end_offset = @min(before orelse file_size, file_size);
        var scan_buffer: [64 * 1024]u8 = undefined;
        const start_offset = transcript.findTailStart(file, init.io, end_offset, line_count, &scan_buffer) catch
            fail(init, "error: failed to translate input file\n");
        transcript.writePageJson(&file_writer.interface, start_offset, end_offset) catch
            fail(init, "error: failed to write output\n");
        file_writer.interface.writeByte('\n') catch fail(init, "error: failed to write output\n");
        if (follow) {
            transcript.translateFollowWindow(init.gpa, format, file, init.io, start_offset, end_offset, &file_writer.interface) catch
                fail(init, "error: failed to translate input file\n");
        } else {
            transcript.translateWindow(init.gpa, format, file, init.io, start_offset, end_offset, &file_writer.interface) catch
                fail(init, "error: failed to translate input file\n");
        }
    } else if (follow) {
        transcript.translateFollow(init.gpa, format, file, init.io, &file_writer.interface) catch
            fail(init, "error: failed to translate input file\n");
    } else {
        var input_buffer: [64 * 1024]u8 = undefined;
        var file_reader = file.reader(init.io, &input_buffer);
        transcript.translate(init.gpa, format, &file_reader.interface, &file_writer.interface) catch
            fail(init, "error: failed to translate input file\n");
    }
    file_writer.interface.flush() catch fail(init, "error: failed to write output\n");
}
