import { expect, test } from "@playwright/test";
import type { Snapshot } from "../src/core/api";
import { mockRows, open, row } from "./ux-helpers";

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
  await page.route("**/api/subscribe", (route) => route.fulfill({ json: { ok: true } }));
  await page.addInitScript(() => {
    const fetch = window.fetch.bind(window);
    const scope = window as unknown as { statusSnapshot: (version: number) => void };
    window.fetch = (input, init) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (!url.endsWith("/api/stream")) return fetch(input, init);
      const body = new ReadableStream({ start(controller) {
        const send = (type: string, data: unknown) => controller.enqueue(new TextEncoder().encode(`event: ${type}\ndata: ${JSON.stringify(data)}\n\n`));
        scope.statusSnapshot = (version) => send("snapshot", { version });
        send("hello", { stream: "status", build: "fixture" });
      } });
      return Promise.resolve(new Response(body, { headers: { "Content-Type": "text/event-stream" } }));
    };
  });

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
    await page.evaluate((version) => (window as unknown as { statusSnapshot: (version: number) => void }).statusSnapshot(version), ++version);
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
    if (process.env.DESIGN_B_SHOTS) for (const theme of ["dark", "light"] as const) {
      await page.emulateMedia({ colorScheme: theme });
      await status.screenshot({ path: `${process.env.DESIGN_B_SHOTS}/working-${theme}.png`, animations: "disabled" });
    }

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
    if (process.env.DESIGN_B_SHOTS) for (const theme of ["dark", "light"] as const) {
      await page.emulateMedia({ colorScheme: theme });
      await status.screenshot({ path: `${process.env.DESIGN_B_SHOTS}/waiting-${theme}.png`, animations: "disabled" });
    }
  } finally {
    await page.clock.resume();
  }
});
