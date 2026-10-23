import { expect, test } from "@playwright/test";

// A newer swarm in the tap shows one toast that upgrades on the first press and restarts the
// server on the second.

const server = () => new URL(process.env.SWARM_SERVE_URL!);

test("an update upgrades, then restarts the server, in two asks", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  const calls: string[] = [];
  await tab.route("**/api/update", (route) => {
    calls.push(`${route.request().method()} /api/update`);
    return route.fulfill({ json: route.request().method() === "GET" ? { current: "0.0.37", latest: "0.0.38", available: true } : { version: "0.0.38" } });
  });
  await tab.route("**/api/update/restart", (route) => {
    calls.push("POST /api/update/restart");
    return route.fulfill({ json: { ok: true } });
  });
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  const toast = tab.locator("[data-sonner-toast]").filter({ hasText: "Swarm" });
  await expect(toast).toContainText("Swarm 0.0.38 is available");
  await toast.getByRole("button", { name: "Upgrade" }).click();
  await expect(toast).toContainText("Swarm 0.0.38 is installed");
  await toast.getByRole("button", { name: "Restart server" }).click();
  await expect(toast).toContainText("Restarting the swarm ui server");
  expect(calls).toEqual(["GET /api/update", "POST /api/update", "POST /api/update/restart"]);
  expect(errors).toEqual([]);
});
