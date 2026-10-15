import { expect, test, type Page } from "@playwright/test";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { GIANT_LINES } from "./serve";

// WebKit reports a ResizeObserver loop when a short row list opens (p1d-result.md). A reload restores
// the owner's last chat, which can be a chat with child columns, so any case can see it at load.
// It changes nothing on screen, so no case counts it as an error.

test("select a chat by keyboard and see its rows", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  expect(new URL(tab.url()).hash).toBe("");

  const sidebar = tab.getByTestId("sidebar");
  await expect(sidebar.locator('[data-testid="tree-row"][data-kind="chat"]')).toHaveCount(9);

  // The first row (a project header) is selected; move down to the first chat, then open it. The Git
  // projects of the lifecycle cases list workspaces with no chat, so the chat may sit further down.
  const rows = sidebar.locator('[data-testid="tree-row"]');
  const ids = await rows.evaluateAll((els) => els.map((e) => e.getAttribute("data-id")!));
  const first = (await rows.evaluateAll((els) => els.map((e) => e.getAttribute("data-kind")))).indexOf("chat");
  for (let i = 0; i < first; i++) await tab.keyboard.press("j");
  const selected = sidebar.locator('[aria-selected="true"]');
  await expect(selected).toHaveAttribute("data-kind", "chat");
  const rowId = (await selected.getAttribute("data-id"))!;
  const chatId = (await selected.getAttribute("data-session"))!;
  await tab.keyboard.press("Enter");

  const transcript = tab.locator(`[data-testid="transcript"][data-chat="${chatId}"][data-state="ready"]`);
  await expect(transcript.getByTestId("row").first()).toBeVisible();

  // Records land in the frame after the paint, so poll.
  await expect.poll(() => tab.evaluate(() => window.__swarmPerf.map((p) => `${p.kind}:${p.target}`))).toEqual([
    ...ids.slice(1, first + 1).map((id) => `key:${id}`),
    `open:${chatId}`,
  ]);
  expect(ids[first]).toBe(rowId);
  expect(errors).toEqual([]);
});

test("⌘K opens the palette with the registered commands and runs one", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);

  await tab.keyboard.press("Meta+k");
  const palette = tab.getByTestId("palette");
  await expect(palette.getByRole("option", { name: /Next Workspace/ })).toContainText("⌃⌘↓");
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
    const chatId = (await chats.nth(i).getAttribute("data-session"))!;
    await expect(tab.locator(`[data-testid="transcript"][data-chat="${chatId}"][data-state="ready"]`).getByTestId("row").first()).toBeVisible();
    const card = tab.getByTestId("tool-card").filter({ hasText: title });
    if (await card.count()) return card.first();
  }
  throw new Error(`no chat has a "${title}" card`);
}

test("a giant output shows a capped card, and Show full output fetches every line", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
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

test("the composer types into the chair, ⌘. interrupts, errors show in a banner, and the prompt card answers", async ({ page: tab }, info) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  // The refused send logs its 409 response as a console error; that one is expected.
  tab.on("console", (m) => m.type() === "error" && !m.text().includes("409") && errors.push(m.text()));
  const home = process.env.SWARM_E2E_HOME!;
  const read = (name: string) => (existsSync(join(home, name)) ? readFileSync(join(home, name), "utf8") : "");
  const screen = join(home, "screen");
  // An empty screen leaves the last screen state; the idle capture ends the question.
  const screens = resolve(import.meta.dirname, "../../tests/fixtures/screens");
  const idle = readFileSync(join(screens, "claude-idle.txt"), "utf8");

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE}"]`).click();
  const composer = tab.getByTestId("composer");
  await expect(composer).toBeEnabled();
  try {
    // ⌘L focuses the box; ⇧Return adds a line; Return sends it and clears the box.
    await tab.keyboard.press("Meta+l");
    await expect(composer).toBeFocused();
    const first = `hello from ${info.project.name}`;
    await tab.keyboard.type(first);
    await tab.keyboard.press("Shift+Enter");
    await tab.keyboard.type("second line");
    await expect(composer).toHaveValue(`${first}\nsecond line`);
    await tab.keyboard.press("Enter");
    await expect.poll(() => read("typed")).toContain(`${first}\nsecond line\n`);
    await expect(composer).toHaveValue("");

    // Esc clears the draft.
    await tab.keyboard.type("a draft to drop");
    await tab.keyboard.press("Escape");
    await expect(composer).toHaveValue("");

    // ⌘. reaches interrupt.
    const interrupts = () => read("actions").split("interrupt fixture-chair").length - 1;
    const before = interrupts();
    await tab.keyboard.press("Meta+.");
    await expect.poll(interrupts).toBe(before + 1);

    // A refused send shows the server's text with Retry and Dismiss, keeps the draft, and runs
    // again only on Retry.
    writeFileSync(join(home, "fail"), "pane is busy in the stand-in adapter");
    const retried = `retried from ${info.project.name}`;
    await tab.keyboard.type(retried);
    await tab.keyboard.press("Enter");
    const banner = tab.getByTestId("chat-banner");
    await expect(banner).toContainText("pane is busy in the stand-in adapter");
    await expect(composer).toHaveValue(retried);
    expect(read("typed")).not.toContain(retried);
    rmSync(join(home, "fail"));
    await banner.getByRole("button", { name: "Retry" }).click();
    await expect.poll(() => read("typed")).toContain(retried);
    await expect(banner).toBeHidden();
    await expect(composer).toHaveValue("");

    // The chair's question: the card shows the CLI's choices; a click and a digit key answer.
    const keys = () => read("actions").split("\n").filter((l) => l.startsWith("key fixture-chair"));
    const keysBefore = keys().length;
    const waiting = readFileSync(join(screens, "claude-waiting.txt"), "utf8");
    writeFileSync(screen, waiting);
    const card = tab.getByTestId("prompt-card");
    await expect(card).toContainText("Do you want to proceed?");
    await expect(card.getByTestId("prompt-choice")).toHaveCount(3);
    await expect(tab.getByTestId("composer-status")).toContainText("The agent is asking a question. Answer it first.");
    await expect(tab.getByTestId("send")).toBeDisabled();
    await card.getByTestId("prompt-choice").first().click();
    await expect.poll(() => keys().slice(keysBefore)).toEqual(["key fixture-chair 1"]);

    writeFileSync(screen, idle);
    await expect(card).toBeHidden();
    writeFileSync(screen, waiting);
    await expect(card).toBeFocused();
    await tab.keyboard.press("3");
    await expect.poll(() => keys().slice(keysBefore)).toEqual(["key fixture-chair 1", "key fixture-chair 3"]);
  } finally {
    writeFileSync(screen, idle);
    rmSync(join(home, "fail"), { force: true });
  }
  expect(errors).toEqual([]);
});

test("the / menu lists the CLI's commands; the send key runs a whole command, a dialog row and Tab only insert, Esc closes it", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const home = process.env.SWARM_E2E_HOME!;
  const typed = () => (existsSync(join(home, "typed")) ? readFileSync(join(home, "typed"), "utf8") : "");
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE}"]`).click();
  const composer = tab.getByTestId("composer");
  const menu = tab.getByTestId("slash-menu");
  await expect(composer).toBeEnabled();
  await composer.click();

  // The rows come from the chair's CLI (the stand-in claude), plus the built-ins it omits.
  await tab.keyboard.type("/swa");
  await expect(menu.getByTestId("slash-row").first()).toContainText("/swarm:plan");
  await expect(menu).toContainText("Plan one change");
  await tab.keyboard.press("Tab");
  await expect(composer).toHaveValue("/swarm:plan ");
  await expect(menu).toBeHidden();
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("");

  // Esc closes the menu first and keeps the text; Up and Down move the selection.
  await tab.keyboard.type("/");
  await expect(menu).toBeVisible();
  await tab.keyboard.press("ArrowDown");
  await expect(menu.locator('[aria-selected="true"]')).toHaveCount(1);
  await tab.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(composer).toHaveValue("/");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("");

  // A dialog command is inserted and never sent, and says why.
  await tab.keyboard.type("/mode");
  await expect(menu.getByTestId("slash-row").first()).toContainText("opens in pane");
  await tab.keyboard.press("Enter");
  await expect(composer).toHaveValue("/model ");
  expect(typed()).not.toContain("/model");
  await tab.keyboard.press("Escape");

  // The send key on a whole-draft command picks it and sends the picked text once, hint or not.
  const before = typed().split("/clear ").length;
  await tab.keyboard.type("/cle");
  await expect(menu.getByTestId("slash-row").first()).toContainText("/clear");
  await tab.keyboard.press("Enter");
  await expect.poll(() => typed().split("/clear ").length).toBe(before + 1);
  await expect(composer).toHaveValue("");
  expect(typed()).not.toContain("/cle\n");

  // Mid-draft, a / that matches nothing shows no menu, so a path types as text.
  await tab.keyboard.type("see src/main.rs");
  await expect(menu).toBeHidden();
  await tab.keyboard.press("Escape");
  expect(errors).toEqual([]);
});

test("a chat with two children shows their panes; keys move focus and zoom; a pane types, asks and closes", async ({ page: tab }, info) => {
  const errors: string[] = [];
  // WebKit reports a ResizeObserver loop when a short row list opens, also in a chat with no panes
  // and a short window (p1d-result.md). It changes nothing on screen, so it is not an error here.
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const home = process.env.SWARM_E2E_HOME!;
  const project = info.project.name;
  const read = (name: string) => (existsSync(join(home, name)) ? readFileSync(join(home, name), "utf8") : "");
  const testerScreen = join(home, `screen-fixture-${project}-tester`);
  const screens = resolve(import.meta.dirname, "../../tests/fixtures/screens");
  const waiting = readFileSync(join(screens, "claude-waiting.txt"), "utf8");

  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  // Wide enough to drag a column's edge to the right.
  await tab.setViewportSize({ width: 1600, height: 900 });
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env[`SWARM_E2E_PANES_${project}`]}"]`).click();
  try {
    // Two children are one column of two panes (ADR 0030), oldest first, each with its own chat.
    const panes = tab.getByTestId("pane");
    await expect(panes).toHaveCount(2);
    await expect(tab.locator(".strip-col")).toHaveCount(1);
    // Opening a chat that has columns lays them out with no slide.
    expect(await tab.getByTestId("pane-strip").evaluate((el) => el.getAnimations().length)).toBe(0);
    const reviewer = tab.locator('[data-testid="pane"][data-pane="reviewer"]');
    const tester = tab.locator('[data-testid="pane"][data-pane="tester"]');
    await expect(reviewer.locator('[data-testid="pane-transcript"][data-state="ready"]').getByTestId("row").first()).toBeVisible();
    await expect(tester.locator('[data-testid="pane-transcript"][data-state="ready"]').getByTestId("row").first()).toBeVisible();
    await expect(tab.locator('[data-testid="transcript"][data-state="ready"]').getByTestId("row").first()).toBeVisible();

    // The chat page leaves the right side to the column, so it shows with no scroll.
    await expect(reviewer).toBeInViewport({ ratio: 0.99 });
    // Polled, since the column slides in for 240 ms.
    await expect
      .poll(async () => {
        const main = (await tab.getByTestId("strip").boundingBox())!;
        const column = (await tab.locator(".strip-col").boundingBox())!;
        return Math.round(main.x + main.width - (column.x + column.width));
      })
      .toBe(7);

    // ⌥⌘→ focuses the first pane, scrolls the strip to it, and puts the keyboard in its composer.
    await tab.keyboard.press("Alt+Meta+ArrowRight");
    await expect(reviewer).toHaveAttribute("data-focused", "true");
    await expect(reviewer).toBeInViewport({ ratio: 0.99 });
    await expect(reviewer.getByTestId("composer")).toBeFocused();
    await tab.keyboard.press("Alt+Meta+ArrowDown");
    await expect(tester).toHaveAttribute("data-focused", "true");
    await expect(tester.getByTestId("composer")).toBeFocused();

    // The pane's composer types into that child.
    const hello = `hello tester from ${project}`;
    await tab.keyboard.type(hello);
    await tab.keyboard.press("Enter");
    await expect.poll(() => read("typed")).toContain(hello);
    await expect(tester.getByTestId("composer")).toHaveValue("");

    // ⇧⌘↩ zooms the focused pane over the strip, and the same key returns it.
    const strip = (await tab.getByTestId("strip").boundingBox())!;
    await tab.keyboard.press("Shift+Meta+Enter");
    await expect(tester).toHaveAttribute("data-zoomed", "true");
    expect((await tester.boundingBox())!.width).toBeCloseTo(strip.width, 0);
    await expect(tester.getByTestId("composer")).toHaveValue("");
    await tab.keyboard.press("Shift+Meta+Enter");
    await expect(tester).toHaveAttribute("data-zoomed", "false");

    // ⌘F finds in the focused pane.
    await tab.keyboard.press("Meta+f");
    await expect(tester.getByTestId("find-input")).toBeFocused();
    await tab.keyboard.press("Escape");

    // ⌥⌘← goes back to the chat page.
    await tab.keyboard.press("Alt+Meta+ArrowUp");
    await tab.keyboard.press("Alt+Meta+ArrowLeft");
    await expect(reviewer).toHaveAttribute("data-focused", "false");
    await expect(tab.getByTestId("strip")).toHaveJSProperty("scrollLeft", 0);

    // Drag resize (ADR 0026): the chat page's edge, a column's edge, and the line between two panes.
    // The widths are kept, and a double-click resets each.
    const width = async (selector: string) => (await tab.locator(selector).boundingBox())!.width;
    const grip = (name: string) => tab.getByRole("separator", { name });
    const drag = async (name: string, dx: number, dy: number) => {
      const box = (await grip(name).boundingBox())!;
      const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
      await tab.mouse.move(x, y);
      await tab.mouse.down();
      await tab.mouse.move(x + dx / 2, y + dy / 2);
      await tab.mouse.move(x + dx, y + dy);
      await tab.mouse.up();
    };
    const chatWidth = await width(".strip-chat");
    await drag("Chat page width", -300, 0);
    expect(await width(".strip-chat")).toBeCloseTo(chatWidth - 300, 0);
    const columnWidth = await width(".strip-col");
    await drag("Agent column width", 100, 0);
    expect(await width(".strip-col")).toBeCloseTo(columnWidth + 100, 0);
    const topHeight = (await reviewer.boundingBox())!.height;
    await drag("Split of reviewer", 0, 80);
    expect((await reviewer.boundingBox())!.height).toBeGreaterThan(topHeight + 70);
    const saved = await tab.evaluate(() => JSON.parse(localStorage.getItem("swarm.panes.widths")!));
    expect(saved.chat).toBeCloseTo(chatWidth + 7 - 300, 0);
    expect(saved.column).toBeCloseTo(columnWidth + 7 + 100, 0);
    expect(saved.splits[process.env[`SWARM_E2E_PANES_${project}`]!].reviewer).toBeGreaterThan(0.55);
    await grip("Split of reviewer").dblclick();
    await grip("Agent column width").dblclick();
    await grip("Chat page width").dblclick();
    expect(await width(".strip-chat")).toBeCloseTo(chatWidth, 0);
    expect(await width(".strip-col")).toBeCloseTo(columnWidth, 0);
    expect((await reviewer.boundingBox())!.height).toBeCloseTo(topHeight, 0);

    // The child's question shows on the chat page too; "Show column" focuses its pane.
    writeFileSync(testerScreen, waiting);
    const asking = tab.getByTestId("asking").getByTestId("prompt-card");
    await expect(asking).toContainText("tester is waiting for you");
    await expect(tester.getByTestId("prompt-card")).toContainText("Do you want to proceed?");
    await asking.getByRole("button", { name: "Show column" }).click();
    await expect(tester).toHaveAttribute("data-focused", "true");
    await expect(tester).toBeInViewport({ ratio: 0.99 });

    // One menu serves the headers: the same actions as the header buttons.
    await reviewer.locator(".pane-head").click({ button: "right" });
    await expect(tab.getByRole("menuitem")).toHaveText(["Stop", "Close agent…", "Copy id", "Copy attach command", "Zoom pane (⇧⌘↩)"]);
    await tab.keyboard.press("Escape");

    // Close agent… asks first for a child in a turn, then reaches /api/agent/close.
    await tester.getByRole("button", { name: "Close agent tester" }).click();
    const dialog = tab.getByTestId("close-agent");
    await expect(dialog).toContainText("“tester” is in a turn. Closing the agent stops it.");
    const closed = tab.waitForRequest((r) => r.url().endsWith("/api/agent/close") && r.method() === "POST");
    await dialog.getByRole("button", { name: "Close Agent" }).click();
    expect((await closed).postDataJSON()).toMatchObject({ agent: "tester" });
    await expect.poll(() => read("actions")).toContain(`close fixture-${project}-tester`);

    // An ended child leaves the strip at once (ADR 0083).
    await expect(panes).toHaveCount(1);
  } finally {
    writeFileSync(testerScreen, readFileSync(join(screens, "claude-idle.txt"), "utf8"));
  }
  expect(errors).toEqual([]);
});
