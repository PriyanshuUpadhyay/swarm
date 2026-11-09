const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const mod = b.addModule("transcript", .{
        .root_source_file = b.path("src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod.addAnonymousImport("fixture-claude-edit", .{
        .root_source_file = b.path("../../tests/fixtures/provider-logs/claude-edit.jsonl"),
    });
    mod.addAnonymousImport("fixture-pi-chat", .{
        .root_source_file = b.path("../../tests/fixtures/provider-logs/pi-chat.jsonl"),
    });
    mod.addAnonymousImport("fixture-claude-chair-run", .{
        .root_source_file = b.path("../../tests/fixtures/provider-logs/claude-chair-run.jsonl"),
    });
    const exe = b.addExecutable(.{
        .name = "transcript",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "transcript", .module = mod }},
        }),
    });
    b.installArtifact(exe);
    const mod_tests = b.addTest(.{ .root_module = mod });
    const test_step = b.step("test", "Run tests");
    test_step.dependOn(&b.addRunArtifact(mod_tests).step);
    test_step.dependOn(&exe.step);
}
