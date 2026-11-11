import { expect, test } from "@playwright/test";
import { resolve } from "node:path";

test("the / menu lists the swarm skills under their own heading, above the built-ins", async ({ page: tab }) => {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE}"]`).click();
  const composer = tab.getByTestId("composer");
  const menu = tab.getByTestId("slash-menu");
  await expect(composer).toBeEnabled();
  await composer.click();
  await tab.keyboard.type("/");

  // The stand-in claude lists /swarm:plan (source "swarm") and /clear (built-in).
  const headings = menu.getByTestId("slash-group");
  await expect(headings).toHaveText(["Swarm skills", "Other commands and skills"]);
  const rows = menu.getByTestId("slash-row");
  await expect(rows.first()).toContainText("/swarm:plan");
  await expect(rows.filter({ hasText: "/clear" })).toBeVisible();
  const top = async (l: ReturnType<typeof tab.locator>) => (await l.boundingBox())!.y;
  expect(await top(headings.nth(0))).toBeLessThan(await top(rows.filter({ hasText: "/swarm:plan" })));
  expect(await top(rows.filter({ hasText: "/swarm:plan" }))).toBeLessThan(await top(headings.nth(1)));
  expect(await top(headings.nth(1))).toBeLessThan(await top(rows.filter({ hasText: "/clear" })));
  await tab.screenshot({ path: resolve(import.meta.dirname, "../../tmp/work/menu.png") });
});
