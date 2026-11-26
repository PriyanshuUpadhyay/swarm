import { expect, test } from "@playwright/test";
import { resolve } from "node:path";

test("head B puts Home and project controls above New chat and palette search", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await page.goto(`${url.origin}/${url.hash}`);
  await page.waitForFunction(() => window.__swarmReady === true);
  const head = page.getByTestId("sidebar-head");
  const home = head.getByRole("button", { name: "Home" });
  const add = head.getByRole("button", { name: "Add project" });
  const settings = head.getByRole("button", { name: "Settings" });
  const chat = head.getByRole("button", { name: "New chat", exact: true });
  const search = head.getByRole("button", { name: "Command palette" });
  await expect(home).toHaveText("Swarm");
  const boxes = await Promise.all([home, add, settings, chat, search].map((control) => control.boundingBox()));
  const [homeBox, addBox, settingsBox, chatBox, searchBox] = boxes.map((box) => box!);
  expect(addBox.height).toBe(24);
  expect(settingsBox.height).toBe(24);
  expect(homeBox.y + homeBox.height).toBeLessThan(chatBox.y);
  expect(chatBox.y + chatBox.height).toBeLessThan(searchBox.y);
  expect(searchBox.height).toBe(32);

  for (const colorScheme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme });
    await page.screenshot({ path: resolve(import.meta.dirname, `../../tmp/shots/U2/head-${colorScheme}.png`) });
  }

  await search.click();
  await page.getByRole("combobox").fill("Skills");
  await page.getByRole("option", { name: "Skills…" }).click();
  await expect(page.getByTestId("skills")).toBeVisible();
});
