import { expect, test } from "@playwright/test";

// ADR 0118: a newer swarm in the tap shows one toast that upgrades on the first press and
// restarts the server, and a changed HUD, on the second. A refused restart can be pressed again.

const server = () => new URL(process.env.SWARM_SERVE_URL!);

test("an update upgrades, then restarts the server and the HUD, in two asks", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  const calls: string[] = [];
  await tab.route("**/api/update", (route) => {
    calls.push(`${route.request().method()} /api/update`);
    return route.fulfill({ json: route.request().method() === "GET" ? { current: "0.0.37", latest: "0.0.38", available: true } : { version: "0.0.38", hud: true } });
  });
  let restarts = 0;
  await tab.route("**/api/update/restart", (route) => {
    calls.push("POST /api/update/restart");
    // The first restart is refused, as a HUD that did not restart is.
    return restarts++ === 0
      ? route.fulfill({ status: 502, json: { error: "the HUD did not restart: no job" } })
      : route.fulfill({ json: { ok: true, hud: false } });
  });
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  const toast = tab.locator("[data-sonner-toast]").filter({ hasText: "Swarm 0.0.38" });
  await expect(toast).toContainText("Swarm 0.0.38 is available");
  await toast.getByRole("button", { name: "Upgrade" }).click();
  await expect(toast).toContainText("Swarm 0.0.38 is installed");
  // A progress step drops the buttons of the step before it.
  await expect(toast.getByRole("button", { name: "Upgrade" })).toHaveCount(0);
  await expect(toast).toContainText("the HUD still run the old version");
  await toast.getByRole("button", { name: "Restart server" }).click();
  await expect(tab.locator("[data-sonner-toast]").filter({ hasText: "the HUD did not restart" })).toBeVisible();
  // The server tried the HUD once, so the toast now names only the server.
  await expect(toast).toContainText("The swarm ui server still runs the old version");
  await toast.getByRole("button", { name: "Restart server" }).click();
  const restarting = tab.locator("[data-sonner-toast]").filter({ hasText: "Restarting the swarm ui server" });
  await expect(restarting).toBeVisible();
  await expect(restarting.getByRole("button")).toHaveCount(0);
  // The toast shows before the POST is sent, so wait for the route to see it.
  await expect.poll(() => calls.length).toBe(4);
  expect(calls).toEqual(["GET /api/update", "POST /api/update", "POST /api/update/restart", "POST /api/update/restart"]);
  expect(errors).toEqual([]);
});
