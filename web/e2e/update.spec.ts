import { expect, test } from "@playwright/test";

// ADR 0118: a newer swarm in the tap shows one toast that upgrades on the first press and
// restarts the server, and a changed HUD, on the second. A failed upgrade and a refused restart
// can each be pressed again.

const server = () => new URL(process.env.SWARM_SERVE_URL!);

test("an update upgrades, then restarts the server and the HUD, in two asks", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  const calls: string[] = [];
  let upgrades = 0;
  await tab.route("**/api/update", (route) => {
    calls.push(`${route.request().method()} /api/update`);
    if (route.request().method() === "GET") return route.fulfill({ json: { current: "0.0.37", latest: "0.0.38", available: true } });
    // The first upgrade fails, as a brew that cannot reach the tap does.
    return upgrades++ === 0
      ? route.fulfill({ status: 502, json: { error: "brew update failed" } })
      : route.fulfill({ json: { version: "0.0.38", hud: true } });
  });
  let restarts = 0;
  await tab.route("**/api/update/restart", (route) => {
    calls.push("POST /api/update/restart");
    // The first restart meets a busy lock, the second a HUD that did not restart, and the third a
    // server that cannot start after the HUD restarted.
    const refusals = [
      { status: 409, json: { error: "an upgrade or a restart is already running" } },
      { status: 502, json: { error: "the HUD did not restart: no job" } },
      { status: 500, json: { error: "Permission denied (os error 13)" } },
    ];
    return route.fulfill(refusals[restarts++] ?? { json: { ok: true } });
  });
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  const toast = tab.locator("[data-sonner-toast]").filter({ hasText: "Swarm 0.0.38" });
  await expect(toast).toContainText("Swarm 0.0.38 is available");
  await toast.getByRole("button", { name: "Upgrade" }).click();
  await expect(toast).toContainText("Upgrade: brew update failed");
  await expect(toast).toContainText("Swarm 0.0.38 is available");
  await toast.getByRole("button", { name: "Upgrade" }).click();
  await expect(toast).toContainText("Swarm 0.0.38 is installed");
  // A progress step drops the buttons of the step before it.
  await expect(toast.getByRole("button", { name: "Upgrade" })).toHaveCount(0);
  await expect(toast).toContainText("the HUD still run the old version");
  await toast.getByRole("button", { name: "Restart server" }).click();
  await expect(toast).toContainText("Restart: an upgrade or a restart is already running");
  // The server did not try the HUD, so the toast still names it.
  await expect(toast).toContainText("the HUD still run the old version");
  await toast.getByRole("button", { name: "Restart server" }).click();
  await expect(toast).toContainText("Restart: the HUD did not restart: no job");
  // The server tried the HUD once, so the toast now names only the server.
  await expect(toast).toContainText("The swarm ui server still runs the old version");
  await expect(toast).not.toContainText("the HUD still run the old version");
  await toast.getByRole("button", { name: "Restart server" }).click();
  // A later refusal keeps the HUD out of the toast.
  await expect(toast).toContainText("Restart: Permission denied (os error 13)");
  await expect(toast).toContainText("The swarm ui server still runs the old version");
  await toast.getByRole("button", { name: "Restart server" }).click();
  const restarting = tab.locator("[data-sonner-toast]").filter({ hasText: "Restarting the swarm ui server" });
  await expect(restarting).toBeVisible();
  await expect(restarting.getByRole("button")).toHaveCount(0);
  // The toast shows before the POST is sent, so wait for the route to see it.
  const expected = ["GET /api/update", "POST /api/update", "POST /api/update", ...Array(4).fill("POST /api/update/restart")];
  await expect.poll(() => calls.length).toBe(expected.length);
  expect(calls).toEqual(expected);
  // Each error shows in the one toast and is still recorded.
  await expect(tab.locator("[data-sonner-toast]")).toHaveCount(1);
  expect(errors).toEqual([]);
});
