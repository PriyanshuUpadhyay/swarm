import { expect, test, type Page } from "@playwright/test";
import { GIANT_LINES } from "./serve";

test("select a chat by keyboard and see its rows", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  expect(new URL(tab.url()).hash).toBe("");

  const sidebar = tab.getByTestId("sidebar");
  await expect(sidebar.locator('[data-testid="tree-row"][data-kind="chat"]')).toHaveCount(3);

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

/** Opens each chat until one shows a tool card with this title. */
async function openChatWithCard(tab: Page, title: string) {
  const chats = tab.getByTestId("sidebar").locator('[data-testid="tree-row"][data-kind="chat"]');
  for (let i = 0; i < (await chats.count()); i++) {
    await chats.nth(i).click();
    const chatId = (await chats.nth(i).getAttribute("data-id"))!.slice(2);
    await expect(tab.locator(`[data-testid="transcript"][data-chat="${chatId}"][data-state="ready"]`).getByTestId("row").first()).toBeVisible();
    const card = tab.getByTestId("tool-card").filter({ hasText: title });
    if (await card.count()) return card.first();
  }
  throw new Error(`no chat has a "${title}" card`);
}

test("a giant output shows a capped card, and Show full output fetches every line", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);

  const card = await openChatWithCard(tab, "cat big.log");
  await card.getByRole("button", { name: /cat big\.log/ }).click();
  const output = card.getByTestId("output");
  // The preview is capped: 120 lines in one <pre>, and a count of the rest.
  await expect(output.locator("pre.out")).toContainText("line 119 of the build log");
  await expect(output.locator("pre.out")).not.toContainText("line 120 of the build log");
  await expect(output).toContainText(`+${(GIANT_LINES - 120).toLocaleString("en-US")} lines`);

  await output.getByTestId("show-full").click();
  await expect(output).toContainText(`Output · ${GIANT_LINES.toLocaleString("en-US")} lines`);
  // The full text is windowed: the box holds a few dozen line nodes, not 20,000.
  const box = output.getByTestId("full-output");
  await expect(box).toContainText("line 0 of the build log");
  expect(await box.locator(".vline").count()).toBeLessThan(200);
  await box.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(box).toContainText(`line ${GIANT_LINES - 1} of the build log`);
  expect(errors).toEqual([]);
});
