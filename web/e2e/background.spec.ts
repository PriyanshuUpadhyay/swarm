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

test("clicking the row that is already shown gives its terminal the keys", async ({ page: tab }, info) => {
  await openChat(tab, info);
  await tab.keyboard.press("Meta+j");
  const panel = tab.getByTestId("background-panel").last();
  await expect(panel.locator(".xterm-rows")).not.toBeEmpty();
  await expect(panel.locator(".xterm-helper-textarea")).not.toBeFocused();
  const row = panel.locator(`button[aria-current="true"]`);
  await row.click();
  await expect(panel.locator(".xterm-helper-textarea")).toBeFocused();
  // A second click, after the keys went elsewhere, gives them back.
  await row.focus();
  await expect(panel.locator(".xterm-helper-textarea")).not.toBeFocused();
  await row.click();
  await expect(panel.locator(".xterm-helper-textarea")).toBeFocused();
});

test("a workspace shell takes keys, outlives the panel, and closes on request", async ({ page: tab }, info) => {
  await openChat(tab, info);
  const panel = tab.getByTestId("background-panel").last();
  await tab.keyboard.press("Meta+j");
  // A second click inside the round trip must not make a second shell.
  await panel.getByTestId("new-shell").dblclick();
  const row = panel.getByRole("button", { name: /Shell 1/ });
  await expect(row).toHaveAttribute("aria-current", "true");
  await expect(panel.getByRole("button", { name: /Shell 2/ })).toHaveCount(0);
  const screen = panel.locator(".xterm-rows");
  // The prompt's look is the owner's, so the check waits for the command's own output.
  await expect(panel.locator(".xterm-helper-textarea")).toBeFocused();
  await tab.keyboard.type("echo shell-$((6*7)) $SWARM_SESSION_ID.\n");
  await expect(screen).toContainText("shell-42 .");
  await tab.keyboard.press("Meta+j");
  await expect(panel).toHaveCount(0);
  await tab.keyboard.press("Meta+j");
  await row.click();
  await expect(screen).toContainText("shell-42 .");
  try {
    await info.attach("shell", { body: await tab.screenshot(), contentType: "image/png" });
    await panel.getByTestId("close-shell").click();
    // Closing ends what runs in the shell, so it asks first and a stray click changes nothing.
    await expect(tab.getByTestId("confirm-body")).toContainText("ends");
    await tab.keyboard.press("Escape");
    await expect(row).toHaveCount(1);
  } finally {
    // Cleanup holds no assertion, so a failure above still closes the real shell.
    await tab.keyboard.press("Escape");
    await panel.getByTestId("close-shell").click();
    await tab.getByTestId("confirm-action").click();
  }
  await expect(row).toHaveCount(0);
});

test("a shell that fails with no text says so, and a close whose refresh fails still closes", async ({ page: tab }, info) => {
  await openChat(tab, info);
  const panel = tab.getByTestId("background-panel").last();
  await tab.keyboard.press("Meta+j");
  // A refusal that names no reason.
  // The glob has no query, so the page's list reads pass; only the create is refused.
  await tab.route("**/api/shells", (route) => route.fulfill({ status: 500, json: { error: "" } }));
  await panel.getByTestId("new-shell").click();
  await expect(panel.getByRole("alert")).toHaveText("The shell did not start.");
  await tab.unroute("**/api/shells");

  await panel.getByTestId("new-shell").click();
  const row = panel.getByRole("button", { name: /Shell 1/ });
  await expect(row).toHaveAttribute("aria-current", "true");
  // The list read after the close fails; the close itself worked.
  await tab.route("**/api/shells?**", (route) => route.abort());
  await panel.getByTestId("close-shell").click();
  await tab.getByTestId("confirm-action").click();
  await expect(tab.getByTestId("confirm")).toHaveCount(0);
  await tab.unroute("**/api/shells?**");
  await expect(row).toHaveCount(0);
});
