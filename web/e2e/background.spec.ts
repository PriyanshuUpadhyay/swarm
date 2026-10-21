import { expect, test } from "@playwright/test";

test("Meta+J shows and hides the background panel with the chat's agents", async ({ page: tab }, info) => {
  const served = process.env.SWARM_SERVE_URL;
  if (!served) throw new Error("SWARM_SERVE_URL is unset; run this through playwright.config.ts, whose global setup starts swarm serve");
  const url = new URL(served);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env[`SWARM_E2E_PANES_${info.project.name}`]}"]`).click();
  await expect(tab.locator('[data-testid="transcript"][data-state="ready"]')).toBeVisible();

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
