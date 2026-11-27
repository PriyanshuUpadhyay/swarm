import { expect, test, type Page } from "@playwright/test";
import { captureThemes, mockRows, mockStream, open, row, rows, streamFrame } from "./ux-helpers";
import type { Block, Inline, Snapshot } from "../src/core/api";

const lines = (last: string) => Array.from({ length: 16 }, (_, i) => i === 15 ? last : `Check result ${i + 1}`).join("\n");

async function queuedChat(page: Page, initial: string[]) {
  const session = process.env.SWARM_E2E_LIVE!;
  let texts = initial;
  let version = 1;
  await mockStream(page, "queued-chat");
  await page.route("**/api/snapshot", async (route) => {
    const snapshot = await (await route.fetch()).json() as Snapshot;
    snapshot.version = version;
    const chair = snapshot.agents[session].find((agent) => agent.id === "orchestrator")!;
    chair.queue = texts.map((text, i) => ({ id: `queue-${i}`, text, mode: "steering", detail: "Steering" }));
    await route.fulfill({ json: snapshot });
  });

  await mockRows(page, [row("before-queue", "Last reply")]);
  await open(page);
  return async (next: string[]) => {
    texts = next;
    await streamFrame(page, "snapshot", { version: ++version });
  };
}

test("the next queued message does not inherit Show all", async ({ page }) => {
  const first = "First queued message";
  const next = "Next queued message";
  const queued = (id: number) => page.getByTestId("row").and(page.locator(`[data-id="queue-${id}"]`));

  try {
    const update = await queuedChat(page, [lines(first), lines(next)]);
    const show = queued(0).getByRole("button", { name: "Show all", exact: true });
    await show.click();
    await expect(queued(0)).toContainText(first);

    await update([lines(next)]);
    await expect(queued(1)).toHaveCount(0);
    await expect(show).toBeVisible({ timeout: 2_000 });
    await expect(queued(0)).not.toContainText(next);
  } finally {
    await page.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("typing the first draft letter does not move the next queued row", async ({ page }) => {
  const hint = page.locator('[data-id="queue-0"]').getByText("to edit queued messages");
  const editor = page.getByTestId("composer");
  try {
    await queuedChat(page, ["First queued text", "Next queued text"]);
    const distance = () => page.locator('[data-kind="queued"]').evaluateAll(([first, next]) =>
      next.getBoundingClientRect().top - first.getBoundingClientRect().top);
    await expect(hint).toBeVisible();
    const before = await distance();

    await editor.fill("a");
    await expect(hint).toBeHidden({ timeout: 2_000 });
    // The virtual list updates row positions after its resize observer runs.
    await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
    expect(await distance()).toBeCloseTo(before, 1);
    await captureThemes(page, page.getByTestId("transcript"), "queue-draft-spacing");
  } finally {
    await editor.fill("");
    await page.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("an opened saved thought reveals the cut lines on request", async ({ page }) => {
  await mockRows(page, [row("t1", lines("The saved thought ends here"), "thought")]);
  await open(page);
  const thought = page.getByTestId("row").filter({ has: page.getByRole("button", { name: "Thinking", exact: true }) });
  await thought.getByRole("button", { name: "Thinking", exact: true }).click();
  await expect(thought.getByTestId("thought-text")).toContainText("Check result 12");
  await expect(thought).toContainText("+4 lines");
  await expect(thought.getByText("The saved thought ends here")).not.toBeVisible();
  const x = await thought.evaluate((el) => [
    el.querySelector(".disclosure span"), el.querySelector("[data-testid=thought-text]"), el.querySelector(".cut-line span"),
  ].map((part) => part!.getBoundingClientRect().x));
  expect(x[1]).toBeCloseTo(x[0], 1);
  expect(x[2]).toBeCloseTo(x[0], 1);
  await captureThemes(page, thought, "thought-cut");
  await thought.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(thought.getByTestId("thought-text")).toContainText("The saved thought ends here");
});

test("Find reveals a cut user message and highlights a match beyond the cut", async ({ page }) => {
  const last = "Please check the socket timeout";
  const start = "Keep the worker response with the rest of this long review request";
  const messageText = lines(last).replace("Check result 1\n", `${start}\n`);
  const children: Inline[] = messageText.split("\n").flatMap((value, i): Inline[] =>
    i === 0 ? [{ type: "strong", value: [{ type: "text", value }] }] : [{ type: "break" }, { type: "text", value }]);
  const result = rows([row("u1", messageText, "user")], 0, {}, { u1: [{ f: "text" }] });
  result.display = { u1: { blocks: [{ type: "paragraph", value: children }] } };
  await page.route("**/api/rows?*", (route) => route.fulfill({ json: result }));
  await open(page);
  const message = page.locator('[data-id="u1"]');
  await expect(message.getByRole("button", { name: "Show all", exact: true })).toBeVisible();
  await expect(message).toContainText("+4 lines");
  await expect(message.locator("strong")).toHaveText(start);
  await expect(message).not.toContainText(last);

  const wide = page.viewportSize()!;
  await page.setViewportSize({ width: 390, height: wide.height });
  await expect.poll(() => message.locator("strong").evaluate((el) => el.getClientRects().length)).toBeGreaterThan(1);
  await expect(message).toContainText("+4 lines");
  await page.setViewportSize(wide);
  await captureThemes(page, message, "user-cut");

  await page.keyboard.press("Meta+f");
  await page.getByTestId("find-input").fill("socket timeout");
  await expect(page.getByTestId("find-count")).toHaveText("1 of 1");
  await expect.poll(() => page.evaluate(() => String(CSS.highlights.get("find-current")?.values().next().value))).toBe("socket timeout");
  await expect(message).toContainText(last);
});

test("a cut user message counts all the lines of an omitted code block", async ({ page }) => {
  const paragraphs: Block[] = Array.from({ length: 12 }, (_, i) => ({
    type: "paragraph", value: [{ type: "text", value: `Paragraph ${i + 1}` }],
  }));
  const text = Array.from({ length: 300 }, (_, i) => `const check${i + 1} = true;`).join("\n");
  const result = rows([row("u1", "A user message with a code block", "user")]);
  result.display = { u1: { blocks: [...paragraphs, { type: "code", value: { label: "ts", text } }] } };
  await page.route("**/api/rows?*", (route) => route.fulfill({ json: result }));

  await open(page);
  const message = page.locator('[data-id="u1"]');
  await expect(message).toContainText("+300 lines");
  await expect(message.locator(".md-code")).toHaveCount(0);
  await message.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(message.locator(".md-code pre")).toContainText("const check300 = true;");
});

test("live thinking shows its last four lines without growing a box", async ({ page }) => {
  await mockRows(page, [row("live-thought", lines("The current thought"), "thought")]);
  await open(page);
  const thought = page.getByTestId("thought-text");
  await expect(thought).toHaveText("Check result 13\nCheck result 14\nCheck result 15\nThe current thought", { useInnerText: true });
  const shape = await thought.evaluate((el) => {
    const s = getComputedStyle(el);
    return [el.clientHeight / parseFloat(s.lineHeight), s.borderTopWidth];
  });
  expect(shape[0]).toBeCloseTo(4, 1);
  expect(shape[1]).toBe("0px");
  await captureThemes(page, page.locator('[data-id="live-thought"]'), "thought-live");
});

test("long code reports its size and scrolls to the last line", async ({ page }) => {
  const code = Array.from({ length: 41 }, (_, i) => `const check${i + 1} = true;`).join("\n");
  await mockRows(page, [row("a1", `\`\`\`ts\n${code}\n\`\`\``)]);
  await open(page);
  const card = page.locator(".md-code");
  const box = card.locator("pre");
  await expect(card.locator(".md-head")).toContainText("41 lines");
  expect(await box.evaluate((el) => el.clientHeight)).toBeLessThanOrEqual(360);
  await captureThemes(page, card, "code-window");
  await box.evaluate((el) => { el.scrollTop = el.scrollHeight; });
  expect(await box.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  await expect(box.getByText("check41", { exact: true })).toBeInViewport();
});

test("long reply code keeps the reply font size and thought fences stay plain text", async ({ page }) => {
  const snippet = Array.from({ length: 41 }, (_, i) => `const item${i + 1} = true;`).join("\n");
  const fence = `\`\`\`ts\n${snippet}\n\`\`\``;
  const thought = "```ts\nconst thought = true;\n```";
  await mockRows(page, [row("t1", thought, "thought"), row("a1", fence)]);
  await open(page);

  await expect(page.locator('[data-id="a1"] pre.scroll-window code')).toHaveCSS("font-size", "12.25px");
  await page.getByRole("button", { name: "Thinking", exact: true }).click();
  const saved = page.getByTestId("thought-text");
  await expect(saved).toHaveText(thought);
  await expect(saved).toHaveCSS("font-size", "13px");
});

test("a failed turn presents the original CLI text as one non-interactive row", async ({ page }) => {
  const source = "API Error: 529 Overloaded";
  await mockRows(page, [row("e1", source, "error")]);
  await open(page);
  const error = page.locator('[data-id="e1"]');
  await expect(error.getByText(source, { exact: true })).toBeVisible();
  await expect(error.getByText(source, { exact: true })).toHaveCSS("font-family", /JetBrains Mono/);
  await expect(error.getByText("Error", { exact: true })).toBeVisible();
  await expect(error.getByRole("button")).toHaveCount(0);
  await captureThemes(page, error, "cli-error");
});
