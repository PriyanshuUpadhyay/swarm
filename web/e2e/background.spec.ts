import { expect, test, type Page, type TestInfo } from "@playwright/test";

/** Opens the fixture chat that has two child agents. */
async function openChat(tab: Page, info: TestInfo) {
  const served = process.env.SWARM_SERVE_URL;
  const chat = process.env[`SWARM_E2E_PANES_${info.project.name}`];
  if (!served || !chat) throw new Error("run this through playwright.config.ts, whose global setup starts swarm serve and its chats");
  const url = new URL(served);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${chat}"]`).click();
  await expect(tab.locator('[data-testid="transcript"][data-state="ready"]')).toBeVisible();
}

test("Meta+J shows and hides the background panel with the chat's agents", async ({ page: tab }, info) => {
  await openChat(tab, info);

  const panel = tab.getByTestId("background-panel");
  await expect(panel).toHaveCount(0);
  await tab.keyboard.press("Meta+j");
  await expect(panel.getByRole("button", { name: /reviewer/ })).toBeVisible();
  await expect(panel.getByRole("button", { name: /tester/ })).toBeVisible();

  // The open terminal takes the new colours when the theme changes.
  await panel.getByRole("button", { name: /reviewer/ }).click();
  const screen = panel.locator(".xterm-scrollable-element");
  const background = () => screen.evaluate((el) => getComputedStyle(el).backgroundColor);
  const before = await background();
  const theme = async (name: string) => {
    await tab.keyboard.press("Meta+k");
    await tab.getByPlaceholder("Search actions, workspaces, chats, agents, and prompts").fill(`Theme: ${name}`);
    await tab.getByRole("option").filter({ hasText: `Theme: ${name}` }).first().click();
  };
  await theme("Dark");
  await expect.poll(background).not.toBe(before);
  await theme("System");
  await expect.poll(background).toBe(before);
  // On "System", the OS switching to dark changes it too.
  await tab.emulateMedia({ colorScheme: "dark" });
  await expect.poll(background).not.toBe(before);
  await tab.emulateMedia({ colorScheme: "light" });
  await expect.poll(background).toBe(before);
  await tab.keyboard.press("Meta+j");
  await expect(panel).toHaveCount(0);
});

test("a resized panel keeps its height when Meta+J hides and shows it", async ({ page: tab }, info) => {
  await openChat(tab, info);
  // The library also puts the panel id on its own element as a test id; the last one is ours.
  const panel = tab.getByTestId("background-panel").last();
  await tab.keyboard.press("Meta+j");
  await expect(panel).toBeVisible();
  const handle = tab.locator('[data-slot="resizable-handle"]:has(+ #background-panel)');
  const box = (await handle.boundingBox())!;
  await tab.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await tab.mouse.down();
  await tab.mouse.move(box.x + box.width / 2, box.y - 120, { steps: 6 });
  await tab.mouse.up();
  const height = async () => (await panel.boundingBox())!.height;
  const resized = await height();
  expect(resized).toBeGreaterThan(330);
  await tab.keyboard.press("Meta+j");
  await expect(panel).toHaveCount(0);
  await tab.keyboard.press("Meta+j");
  await expect.poll(height).toBeCloseTo(resized, -1);
  // A panel that opens with the chat leaves the keys with the chat, also once its terminal connects.
  await expect(panel.locator(".xterm-rows")).not.toBeEmpty();
  await expect(panel.locator(".xterm-helper-textarea")).not.toBeFocused();
  // The saved layout brings it back after a reload, shown or hidden at the time.
  await openChat(tab, info);
  await expect.poll(height).toBeCloseTo(resized, -1);
  await tab.keyboard.press("Meta+j");
  await openChat(tab, info);
  await tab.keyboard.press("Meta+j");
  await expect.poll(height).toBeCloseTo(resized, -1);
});

test("a panel that fails to load says so and keeps the chat", async ({ page: tab }, info) => {
  await tab.route(/\/assets\/background-[^/]*\.js$/, (route) => route.abort());
  await openChat(tab, info);
  await tab.keyboard.press("Meta+j");
  await expect(tab.getByText("The background panel failed. Reload the page to try again.")).toBeVisible();
  await expect(tab.locator('[data-testid="transcript"][data-state="ready"]')).toBeVisible();
});
