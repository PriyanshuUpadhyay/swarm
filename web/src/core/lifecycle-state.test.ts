import { describe, expect, it } from "vitest";
import { setupFailure, setupFrame } from "./lifecycle-state";

const NEW_WORKSPACE = "/work/proj-worktrees/fix";
const OTHER_WORKSPACE = "/work/proj-worktrees/other";

describe("setup frames", () => {
  it("follows the first workspace that starts, appends its output, and ends on its final frame", () => {
    let run = setupFrame(null, { workspace: NEW_WORKSPACE, state: "running" });
    run = setupFrame(run, { workspace: OTHER_WORKSPACE, state: "output", text: "not ours\n" });
    run = setupFrame(run, { workspace: NEW_WORKSPACE, state: "output", channel: "stdout", text: "installing\n" });
    run = setupFrame(run, { workspace: NEW_WORKSPACE, state: "output", channel: "stderr", text: "warn\n" });
    run = setupFrame(run, { workspace: NEW_WORKSPACE, state: "failed", exit_code: 3 });
    expect(run).toEqual({ workspace: NEW_WORKSPACE, state: "failed", output: "installing\nwarn\n", exitCode: 3, error: null });
    expect(setupFailure(run!)).toBe(`The setup script failed with exit code 3. The workspace was kept at ${NEW_WORKSPACE}.`);
  });

  it("ignores output before a run starts, and keeps only the newest output past the cap", () => {
    expect(setupFrame(null, { workspace: NEW_WORKSPACE, state: "output", text: "early" })).toBeNull();
    const run = setupFrame(setupFrame(null, { workspace: NEW_WORKSPACE, state: "running" }), { workspace: NEW_WORKSPACE, state: "output", text: "x".repeat(300 * 1024) + "END" });
    expect(run!.output.length).toBe(256 * 1024);
    expect(run!.output.endsWith("END")).toBe(true);
  });
});

