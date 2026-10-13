import { expect, test } from "@playwright/test";

test("select a chat by keyboard and see its rows", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  expect(new URL(tab.url()).hash).toBe("");

  const sidebar = tab.getByTestId("sidebar");
  await expect(sidebar.locator('[data-testid="tree-row"][data-kind="chat"]')).toHaveCount(2);

  // The first row is selected; move down to the first chat row, then open it.
  await tab.keyboard.press("j");
  await tab.keyboard.press("j");
  const selected = sidebar.locator('[aria-selected="true"]');
  await expect(selected).toHaveAttribute("data-kind", "chat");
  const chatId = (await selected.getAttribute("data-id"))!.slice(2);
  await tab.keyboard.press("Enter");

  const transcript = tab.locator(`[data-testid="transcript"][data-chat="${chatId}"][data-state="ready"]`);
  await expect(transcript.getByTestId("row").first()).toBeVisible();

  // Records land in the frame after the paint, so poll.
  await expect.poll(() => tab.evaluate(() => window.__swarmPerf.map((p) => `${p.kind}:${p.target}`))).toEqual([
    "key:" + (await sidebar.locator('[data-testid="tree-row"]').nth(1).getAttribute("data-id")),
    `key:c:${chatId}`,
    `open:${chatId}`,
  ]);
  expect(errors).toEqual([]);
});

test("⌘K opens the palette with the registered commands and runs one", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);

  await tab.keyboard.press("Meta+k");
  const palette = tab.getByTestId("palette");
  await expect(palette.getByRole("option", { name: /Next workspace/ })).toContainText("⌃⌘↓");
  await tab.keyboard.type("light");
  await expect(palette.getByRole("option")).toHaveText(["Theme: Light"]);
  await tab.keyboard.press("Enter");
  await expect(palette).toBeHidden();
  await expect(tab.locator("html")).toHaveAttribute("data-theme", "light");
  expect(errors).toEqual([]);
});
