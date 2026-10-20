// Event JSON output and streaming translation.
const std = @import("std");
const root = @import("root.zig");
const Format = root.Format;
const Event = root.Event;
const str = root.str;
const parseLine = root.parseLine;

pub fn writeEventJson(writer: *std.Io.Writer, event: Event) std.Io.Writer.Error!void {
    // Escape all non-ASCII code points so byte 0x0A is the only line separator.
    var stringify: std.json.Stringify = .{ .writer = writer, .options = .{ .escape_unicode = true } };
    try stringify.beginObject();
    try stringify.objectField("type");
    try stringify.write(@tagName(event));
    switch (event) {
        inline else => |value| {
            inline for (comptime std.meta.fieldNames(@TypeOf(value))) |name| {
                if (comptime !std.mem.eql(u8, name, "meta")) {
                    try stringify.objectField(name);
                    try stringify.write(@field(value, name));
                }
            }
            try stringify.objectField("meta");
            try stringify.write(value.meta);
        },
    }
    try stringify.endObject();
}

pub fn writePageJson(writer: *std.Io.Writer, start_offset: u64, end_offset: u64) std.Io.Writer.Error!void {
    var stringify: std.json.Stringify = .{ .writer = writer, .options = .{ .escape_unicode = true } };
    try stringify.beginObject();
    try stringify.objectField("type");
    try stringify.write("page");
    try stringify.objectField("start_offset");
    try stringify.write(start_offset);
    try stringify.objectField("end_offset");
    try stringify.write(end_offset);
    try stringify.endObject();
}

/// Returns the first byte of the last `line_count` lines in `[0, end_offset)`.
/// A newline at `end_offset - 1` ends the last line and does not start an empty line.
pub fn findTailStart(file: std.Io.File, io: std.Io, end_offset: u64, line_count: u64, buffer: []u8) !u64 {
    std.debug.assert(line_count > 0);
    std.debug.assert(buffer.len > 0);

    var cursor = end_offset;
    var boundaries: u64 = 0;
    while (cursor > 0) {
        const chunk_start = cursor - @min(cursor, @as(u64, @intCast(buffer.len)));
        const chunk_len: usize = @intCast(cursor - chunk_start);
        const read_len = try file.readPositionalAll(io, buffer[0..chunk_len], chunk_start);
        if (read_len != chunk_len) return error.EndOfStream;

        var index = read_len;
        while (index > 0) {
            index -= 1;
            if (buffer[index] != '\n') continue;
            const absolute = chunk_start + @as(u64, @intCast(index));
            if (absolute + 1 == end_offset) continue;
            boundaries += 1;
            if (boundaries == line_count) return absolute + 1;
        }
        cursor = chunk_start;
    }
    return 0;
}

pub const piSessionIdFromPath = @import("pi.zig").sessionIdFromPath;

pub fn agySessionIdFromPath(path: []const u8) []const u8 {
    var parts = std.mem.splitScalar(u8, path, std.fs.path.sep);
    while (parts.next()) |part| {
        if (std.mem.eql(u8, part, "brain")) {
            const session_id = parts.next() orelse return "";
            if (session_id.len != 0 and parts.next() != null) return session_id;
        }
    }
    return "";
}

pub const Translator = struct {
    line_buffer: std.Io.Writer.Allocating,
    arena_state: std.heap.ArenaAllocator,
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    owned_session_id: ?[]u8 = null,
    offset: u64 = 0,
    unknown_log: ?*std.Io.Writer = null,
    progress_pages: bool = false,

    pub fn init(gpa: std.mem.Allocator, format: Format, session_id: []const u8) Translator {
        return .{
            .line_buffer = .init(gpa),
            .arena_state = .init(gpa),
            .gpa = gpa,
            .format = format,
            .session_id = session_id,
        };
    }

    pub fn deinit(translator: *Translator) void {
        translator.line_buffer.deinit();
        translator.arena_state.deinit();
        if (translator.owned_session_id) |session_id| translator.gpa.free(session_id);
    }

    pub fn translateAvailable(translator: *Translator, reader: *std.Io.Reader, writer: *std.Io.Writer, parse_final: bool) !void {
        while (true) {
            _ = try reader.streamDelimiterEnding(&translator.line_buffer.writer, '\n');
            const at_end = end: {
                const byte = reader.takeByte() catch |err| switch (err) {
                    error.EndOfStream => break :end true,
                    else => return err,
                };
                std.debug.assert(byte == '\n');
                break :end false;
            };
            if (at_end and (translator.line_buffer.written().len == 0 or !parse_final)) return;
            _ = translator.arena_state.reset(.retain_capacity);
            const line = std.mem.trimEnd(u8, translator.line_buffer.written(), "\r");
            const events = try switch (translator.format) {
                .claude => parseLine(translator.arena_state.allocator(), line),
                .codex => @import("codex.zig").parseLine(translator.arena_state.allocator(), line),
                .agy => @import("agy.zig").parseLine(translator.arena_state.allocator(), line),
                .pi => @import("pi.zig").parseLine(translator.arena_state.allocator(), line),
            };
            if (translator.format == .codex) {
                for (events) |event| {
                    const meta = switch (event) {
                        .unknown => |unknown| unknown.meta orelse continue,
                        inline else => |value| value.meta,
                    };
                    if (meta.session_id.len == 0) continue;
                    const session_id = try translator.gpa.dupe(u8, meta.session_id);
                    if (translator.owned_session_id) |old| translator.gpa.free(old);
                    translator.owned_session_id = session_id;
                    translator.session_id = session_id;
                    break;
                }
            }
            for (events) |*event| {
                if (event.* == .ignored) event.ignored.raw = line;
                if (translator.format != .claude) {
                    switch (event.*) {
                        .unknown => |*unknown| {
                            if (unknown.meta) |*meta| meta.session_id = translator.session_id;
                        },
                        inline else => |*value| value.meta.session_id = translator.session_id,
                    }
                }
                try writeEventJson(writer, event.*);
                try writer.writeByte('\n');
                if (event.* == .unknown) translator.logUnknown(line);
            }
            try writer.flush();
            translator.offset += translator.line_buffer.written().len + @intFromBool(!at_end);
            translator.line_buffer.clearRetainingCapacity();
            if (translator.progress_pages and !at_end) {
                try writePageJson(writer, translator.offset, translator.offset);
                try writer.writeByte('\n');
                try writer.flush();
            }
            if (at_end) return;
        }
    }

    fn logUnknown(translator: *Translator, line: []const u8) void {
        const log = translator.unknown_log orelse return;
        var kind: []const u8 = "-";
        var detail: []const u8 = "";
        const parsed = std.json.parseFromSliceLeaky(std.json.Value, translator.arena_state.allocator(), line, .{}) catch .null;
        if (parsed == .object) {
            const rec = parsed.object;
            const record_type = str(rec, "type");
            if (record_type.len != 0) kind = record_type;
            if (translator.format == .claude and std.mem.eql(u8, record_type, "attachment")) {
                if (rec.get("attachment")) |attachment| {
                    if (attachment == .object) detail = str(attachment.object, "type");
                }
            } else if (translator.format == .claude and std.mem.eql(u8, record_type, "system")) {
                detail = str(rec, "subtype");
            } else if (translator.format == .codex and (std.mem.eql(u8, record_type, "response_item") or std.mem.eql(u8, record_type, "event_msg"))) {
                if (rec.get("payload")) |payload| {
                    if (payload == .object) detail = str(payload.object, "type");
                }
            } else if (translator.format == .pi and std.mem.eql(u8, record_type, "message")) {
                if (rec.get("message")) |message| {
                    if (message == .object) detail = str(message.object, "role");
                }
            } else if (translator.format == .agy) {
                detail = str(rec, "status");
            }
        }
        log.print("transcript: unknown format={s} offset={d} kind=", .{ @tagName(translator.format), translator.offset }) catch return;
        for (kind) |byte| {
            log.writeByte(if (std.ascii.isAlphanumeric(byte) or byte == '_' or byte == '-' or byte == '/') byte else '_') catch return;
        }
        if (detail.len != 0) {
            log.writeByte('/') catch return;
            for (detail) |byte| {
                log.writeByte(if (std.ascii.isAlphanumeric(byte) or byte == '_' or byte == '-' or byte == '/') byte else '_') catch return;
            }
        }
        log.writeByte('\n') catch return;
        log.flush() catch {};
    }
};

fn translateFileRange(
    translator: *Translator,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
    input_buffer: []u8,
    parse_final: bool,
) !void {
    var offset = start_offset;
    while (offset < end_offset) {
        const read_len: usize = @intCast(@min(end_offset - offset, @as(u64, @intCast(input_buffer.len))));
        const actual = try file.readPositionalAll(io, input_buffer[0..read_len], offset);
        if (actual != read_len) return error.EndOfStream;
        offset += actual;
        var reader = std.Io.Reader.fixed(input_buffer[0..actual]);
        try translator.translateAvailable(&reader, writer, false);
    }
    if (parse_final) {
        var reader = std.Io.Reader.fixed("");
        try translator.translateAvailable(&reader, writer, true);
    }
}

pub fn translate(gpa: std.mem.Allocator, format: Format, session_id: []const u8, reader: *std.Io.Reader, writer: *std.Io.Writer) !void {
    try translateWithLog(gpa, format, session_id, reader, writer, null);
}

pub fn translateWithLog(gpa: std.mem.Allocator, format: Format, session_id: []const u8, reader: *std.Io.Reader, writer: *std.Io.Writer, unknown_log: ?*std.Io.Writer) !void {
    var translator: Translator = .init(gpa, format, session_id);
    defer translator.deinit();
    translator.unknown_log = unknown_log;
    try translator.translateAvailable(reader, writer, true);
}

/// Translates `[start_offset, end_offset)`, including a final line without a newline.
pub fn translateWindow(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
) !void {
    try translateWindowWithLog(gpa, format, session_id, file, io, start_offset, end_offset, writer, null);
}

pub fn translateWindowWithLog(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
    unknown_log: ?*std.Io.Writer,
) !void {
    var translator: Translator = .init(gpa, format, session_id);
    defer translator.deinit();
    translator.offset = start_offset;
    translator.unknown_log = unknown_log;
    var input_buffer: [64 * 1024]u8 = undefined;
    try translateFileRange(&translator, file, io, start_offset, end_offset, writer, &input_buffer, true);
}

pub fn translateFollow(gpa: std.mem.Allocator, format: Format, session_id: []const u8, file: std.Io.File, io: std.Io, writer: *std.Io.Writer) !void {
    try translateFollowWithLog(gpa, format, session_id, file, io, writer, null);
}

pub fn translateFollowWithLog(gpa: std.mem.Allocator, format: Format, session_id: []const u8, file: std.Io.File, io: std.Io, writer: *std.Io.Writer, unknown_log: ?*std.Io.Writer) !void {
    try translateFollowWindowWithLog(gpa, format, session_id, file, io, 0, 0, writer, unknown_log);
}

/// Translates the initial window, emits an empty page at its end, holds its final partial line,
/// and follows from `end_offset`. The empty page lets consumers finish loading without an idle timer.
pub fn translateFollowWindow(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
) !void {
    try translateFollowWindowWithLog(gpa, format, session_id, file, io, start_offset, end_offset, writer, null);
}

pub fn translateFollowWindowWithLog(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    start_offset: u64,
    end_offset: u64,
    writer: *std.Io.Writer,
    unknown_log: ?*std.Io.Writer,
) !void {
    var translator: Translator = .init(gpa, format, session_id);
    defer translator.deinit();
    translator.offset = start_offset;
    translator.unknown_log = unknown_log;
    var input_buffer: [64 * 1024]u8 = undefined;
    try translateFileRange(&translator, file, io, start_offset, end_offset, writer, &input_buffer, false);
    // An empty page marks the end of the initial window, including logs with no events.
    try writePageJson(writer, end_offset, end_offset);
    try writer.writeByte('\n');
    try writer.flush();
    try followReads(&translator, file, io, end_offset, writer, &input_buffer);
}

/// Clamps `from` to file size, holds incomplete lines, and emits pages at complete-line offsets.
pub fn translateFollowFromWithLog(
    gpa: std.mem.Allocator,
    format: Format,
    session_id: []const u8,
    file: std.Io.File,
    io: std.Io,
    from: u64,
    writer: *std.Io.Writer,
    unknown_log: ?*std.Io.Writer,
) !void {
    var translator: Translator = .init(gpa, format, session_id);
    defer translator.deinit();
    translator.unknown_log = unknown_log;
    var input_buffer: [64 * 1024]u8 = undefined;
    const end_offset = try fromWindow(&translator, file, io, from, writer, &input_buffer);
    try followReads(&translator, file, io, end_offset, writer, &input_buffer);
}

pub fn fromWindow(translator: *Translator, file: std.Io.File, io: std.Io, from: u64, writer: *std.Io.Writer, input_buffer: []u8) !u64 {
    const end_offset = (try file.stat(io)).size;
    const start_offset = @min(from, end_offset);
    translator.offset = start_offset;
    translator.progress_pages = true;
    try writePageJson(writer, start_offset, end_offset);
    try writer.writeByte('\n');
    try writer.flush();
    try translateFileRange(translator, file, io, start_offset, end_offset, writer, input_buffer, false);
    return end_offset;
}

fn followReads(translator: *Translator, file: std.Io.File, io: std.Io, end_offset: u64, writer: *std.Io.Writer, input_buffer: []u8) !void {
    var offset = end_offset;
    while (true) {
        const read_len = try file.readPositionalAll(io, input_buffer, offset);
        if (read_len == 0) {
            if (std.posix.getppid() == 1) return;
            try std.Io.sleep(io, .fromMilliseconds(200), .awake);
            continue;
        }
        offset += read_len;
        var reader = std.Io.Reader.fixed(input_buffer[0..read_len]);
        try translator.translateAvailable(&reader, writer, false);
    }
}
