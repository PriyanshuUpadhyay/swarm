import { expect, test } from "@playwright/test";
import type { Snapshot } from "../src/core/api";
import { captureThemes, mockRows, mockStream, open, row, streamFrame } from "./ux-helpers";

test("the status line names the work and keeps the turn clock outside the log", async ({ page }) => {
  const session = process.env.SWARM_E2E_LIVE!;
  const start = new Date("2030-01-01T00:00:00Z");
  const startS = start.getTime() / 1000;
  let state = "done";
  let version = 0;

  await page.route("**/api/snapshot", async (route) => {
    const snapshot = await (await route.fetch()).json() as Snapshot;
    snapshot.version = version;
    Object.assign(snapshot.agents[session][0], { state, state_at_s: startS, turn_start_time_s: startS - 65 });
    await route.fulfill({ json: snapshot });
  });
  await mockStream(page, "status");

  await mockRows(page, [{
    ...row("f1", "Ran tests", "fold"),
    fold: { commands: 2, waits: 1, rings: 0, failed: 0, interrupted: 0, unreported: 0, latest_title: "Running serve tests", state: "waiting", is_running: true, text: "Running checks" },
  }]);
  await open(page);
  await expect(page.getByTestId("fold")).toBeVisible();
  await page.clock.install({ time: start });
  await page.clock.pauseAt(new Date(start.getTime() + 1000));

  const change = async (next: string) => {
    state = next;
    await streamFrame(page, "snapshot", { version: ++version });
  };
  const status = page.getByTestId("chat-status-line");
  const send = page.getByTestId("send");

  try {
    await change("working");
    await expect(send).toHaveCount(0);
    await page.clock.runFor(199);
    await expect(status).toHaveCount(0);
    await page.clock.runFor(1);
    await expect(status.getByRole("status")).toHaveText("Running serve tests", { timeout: 2000 });
    await expect(status.getByLabel("Turn elapsed: 1:06")).toBeVisible();
    await expect(status.getByTestId("stop")).toBeEnabled();
    await expect(page.getByTestId("transcript").getByText("Working…", { exact: true })).toHaveCount(0);
    await captureThemes(page, status, "working");

    await page.clock.runFor(100);
    await change("done");
    await expect(send).toBeVisible();
    await page.clock.runFor(299);
    await expect(status).toBeVisible();
    await page.clock.runFor(1);
    await expect(status).toHaveCount(0);

    await change("blocked");
    await expect(status).toContainText("Waiting for your answer");
    await expect(send).toBeDisabled();
    await captureThemes(page, status, "waiting");
  } finally {
    await page.clock.resume();
  }
});
