import { expect, test } from "@playwright/test";
import { openChatWithCard } from "./smoke-helpers";
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
  await expect(sidebar.locator('[data-testid="tree-row"][data-kind="chat"]')).toHaveCount(8);

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
  // Other cases and a repeat type into the same file, so only what this case typed counts.
  const typedBefore = read("typed").length;
  const typed = () => read("typed").slice(typedBefore);
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
    await expect.poll(typed).toContain(`${first}\nsecond line\n`);
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
    expect(typed()).not.toContain(retried);
    rmSync(join(home, "fail"));
    await banner.getByRole("button", { name: "Retry" }).click();
    await expect.poll(typed).toContain(retried);
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
