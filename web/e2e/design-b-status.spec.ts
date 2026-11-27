import { expect, test } from "@playwright/test";
import type { Snapshot } from "../src/core/api";
import { captureThemes, mockRows, mockStream, open, row, streamFrame } from "./ux-helpers";

test("the status line names the work and keeps the turn clock outside the log", async ({ page }) => {
  const session = process.env.SWARM_E2E_LIVE!;
  const start = new Date("2030-01-01T00:00:00Z");
  const startS = start.getTime() / 1000;
  const browserTime = new Date(start.getTime() - 120_000);
  let state = "done";
  let version = 0;

  await page.route("**/api/snapshot", async (route) => {
    const snapshot = await (await route.fetch()).json() as Snapshot;
    snapshot.version = version;
    Object.assign(snapshot.agents[session][0], {
      state,
      state_at_s: startS,
      turn_start_time_s: state === "done" ? undefined : startS - 65,
      turn_clock_time_s: state === "done" ? undefined : startS,
    });
    await route.fulfill({ json: snapshot });
  });
  await mockStream(page, "status");

  await mockRows(page, [{
    ...row("f1", "Ran tests", "fold"),
    fold: { commands: 2, waits: 1, rings: 0, failed: 0, interrupted: 0, unreported: 0, latest_title: "Running serve tests", state: "waiting", is_running: true, text: "Running checks" },
  }]);
  await open(page);
  await expect(page.getByTestId("fold")).toBeVisible();
  await page.clock.install({ time: browserTime });
  await page.clock.pauseAt(new Date(browserTime.getTime() + 1000));

  const change = async (next: string) => {
    state = next;
    await streamFrame(page, "snapshot", { version: ++version });
  };
  const status = page.getByTestId("chat-status-line");
  const send = page.getByTestId("send");
  const transcript = page.getByTestId("transcript");
  const bottom = () => transcript.evaluate((el) => el.getBoundingClientRect().bottom);

  try {
    const idleBottom = await bottom();
    await captureThemes(page, page.locator(".chat-input"), "status-idle");
    await change("working");
    await expect(send).toHaveCount(0);
    await page.clock.runFor(199);
    await expect(status).toHaveCount(0);
    await page.clock.runFor(1);
    await expect(status.getByRole("status")).toHaveText("Running serve tests", { timeout: 2000 });
    await expect(status.getByText("Turn elapsed: 1:05", { exact: true })).toBeAttached({ timeout: 2000 });
    await expect(status.getByTestId("stop")).toBeEnabled();
    expect(await bottom()).toBeCloseTo(idleBottom, 1);
    await captureThemes(page, page.locator(".chat-input"), "status-working");
    await expect(page.getByTestId("transcript").getByText("Working…", { exact: true })).toHaveCount(0);
    await captureThemes(page, status, "working");

    await page.clock.runFor(100);
    await change("done");
    await expect(send).toBeVisible();
    await page.clock.runFor(299);
    await expect(status).toBeVisible();
    await page.clock.runFor(1);
    await expect(status).toHaveCount(0);
    expect(await bottom()).toBeCloseTo(idleBottom, 1);

    await change("blocked");
    await expect(status).toContainText("Waiting for your answer");
    await expect(send).toBeDisabled();
    await captureThemes(page, status, "waiting");
  } finally {
    await page.clock.resume();
  }
});

test("a slow sidebar read does not delay the turn clock sample", async ({ page }) => {
  const session = process.env.SWARM_E2E_LIVE!;
  const serverS = Date.parse("2030-01-01T00:00:00Z") / 1_000;
  let hold = false;
  let release!: () => void;
  const held = new Promise<void>((done) => { release = done; });
  await page.clock.install();
  await mockStream(page, "slow-sidebar");

  await page.route("**/api/snapshot", async (route) => {
    const snapshot = await (await route.fetch()).json() as Snapshot;
    snapshot.version = hold ? 700 : 699;
    Object.assign(snapshot.agents[session][0], {
      state: hold ? "working" : "done",
      alive: true,
      turn_start_time_s: hold ? serverS - 65 : undefined,
      turn_clock_time_s: hold ? serverS : undefined,
    });
    await route.fulfill({ json: snapshot });
  });
  await page.route("**/api/tree?view=workspaces", async (route) => {
    if (hold) await held;
    await route.continue();
  });

  try {
    await open(page);
    const send = page.getByTestId("send");
    await expect(send).toBeVisible();
    hold = true;
    const [reply] = await Promise.all([page.waitForResponse("**/api/snapshot"), streamFrame(page, "snapshot", { version: 700 })]);
    await reply.finished();
    await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));

    await page.clock.fastForward(5_000);
    release();
    await expect(send).toHaveCount(0, { timeout: 2_000 });
    await page.clock.runFor(200);
    const status = page.getByTestId("chat-status-line");
    await expect(status).toContainText(/Turn elapsed: 1:1\d/, { timeout: 2_000 });
    await captureThemes(page, status, "clock-after-slow-sidebar");
  } finally {
    release();
    await page.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("a reopened working chat counts from the accepted snapshot", async ({ page }) => {
  const session = process.env.SWARM_E2E_GIT!;
  const time = new Date("2030-01-01T00:00:00Z");
  const serverS = time.getTime() / 1000;
  await page.clock.install({ time });

  try {
    await mockStream(page, "clock-age");
    await page.route("**/api/snapshot", async (route) => {
      const snapshot = await (await route.fetch()).json() as Snapshot;
      Object.assign(snapshot.agents[session][0], {
        state: "working", alive: true, turn_start_time_s: serverS - 65, turn_clock_time_s: serverS,
      });
      await route.fulfill({ json: snapshot });
    });
    await mockRows(page, [row("reply", "Last reply")]);
    await open(page);
    const now = await page.evaluate(() => Date.now());
    await page.clock.pauseAt(new Date(now + 100));
    await page.clock.runFor(5_000);

    const target = page.locator(`[data-testid="tree-row"][data-session="${session}"]`);
    await target.evaluate((el) => (el as HTMLElement).click());
    await expect(page.getByTestId("composer")).toBeVisible({ timeout: 2_000 });
    await page.clock.runFor(200);
    await expect(page.getByTestId("chat-status-line")).toContainText(/Turn elapsed: 1:1\d/, { timeout: 2_000 });
    await captureThemes(page, page.getByTestId("chat-status-line"), "clock-age");
  } finally {
    await page.unrouteAll({ behavior: "ignoreErrors" });
    await page.clock.resume();
  }
});
