import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

test("a chat with two children shows their panes; keys move focus and zoom; a pane types, asks and closes", async ({ page: tab }, info) => {
  const errors: string[] = [];
  // WebKit reports a ResizeObserver loop when a short row list opens, also in a chat with no panes
  // and a short window (p1d-result.md). It changes nothing on screen, so it is not an error here.
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const home = process.env.SWARM_E2E_HOME!;
  const project = info.project.name;
  const read = (name: string) => (existsSync(join(home, name)) ? readFileSync(join(home, name), "utf8") : "");
  // Other cases and a repeat write the same logs, so only what this case wrote counts.
  const typedBefore = read("typed").length;
  const actionsBefore = read("actions").length;
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
    await expect.poll(() => read("typed").slice(typedBefore)).toContain(hello);
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
    // A column reset leaves the chat's width free.
    await grip("Agent column width").dblclick();
    expect(await tab.evaluate(() => JSON.parse(localStorage.getItem("swarm.panes.widths") ?? "{}").chat)).toBeUndefined();
    // A column drag before any chat drag keeps the chat's width, so the grip follows the pointer.
    const chatBefore = await width(".strip-chat");
    const gripX = (await grip("Agent column width").boundingBox())!.x;
    await drag("Agent column width", 100, 0);
    expect((await grip("Agent column width").boundingBox())!.x).toBeCloseTo(gripX + 100, 0);
    expect(await width(".strip-chat")).toBeCloseTo(chatBefore, 0);
    await grip("Agent column width").dblclick();
    await grip("Chat page width").dblclick();
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
    await expect.poll(() => read("actions").slice(actionsBefore)).toContain(`close fixture-${project}-tester`);

    // An ended child leaves the strip at once (ADR 0083).
    await expect(panes).toHaveCount(1);
  } finally {
    writeFileSync(testerScreen, readFileSync(join(screens, "claude-idle.txt"), "utf8"));
    // Later cases and a repeat need the tester again: its pane is listed, and the store has it.
    const pane = `fixture-${project}-tester`;
    rmSync(join(home, `closed-${pane}`), { force: true });
    const session = process.env[`SWARM_E2E_PANES_${project}`]!.replaceAll("'", "''");
    execFileSync("sqlite3", [process.env.SWARM_E2E_DB!, `UPDATE agent SET pane_id = '${pane}' WHERE session_id = '${session}' AND id = 'tester';`]);
  }
  expect(errors).toEqual([]);
});

test("a pasted image is saved by the server and its path goes in the draft", async ({ page: tab }) => {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE}"]`).click();
  const composer = tab.getByTestId("composer");
  await expect(composer).toBeEnabled();
  await composer.fill("look at");
  // A real 1x1 PNG, so the preview must decode it.
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==", "base64");
  await composer.evaluate((el, bytes) => {
    const data = new DataTransfer();
    data.items.add(new File([new Uint8Array(bytes)], "shot.png", { type: "image/png" }));
    el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  }, [...png]);
  await expect(composer).toHaveValue(/^look at \/.+\/uploads\/[0-9a-f-]+\.png $/);
  const path = (await composer.inputValue()).split(" ")[2];
  expect(readFileSync(path)).toEqual(png);
  // A preview shows above the box, and a click shows the image full size.
  const thumb = tab.getByTestId("image-thumb");
  // The page's content policy must let a blob image draw.
  await expect.poll(() => thumb.locator("img").evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(1);
  await thumb.getByRole("button", { name: "Show full size" }).click();
  await expect.poll(() => tab.getByTestId("image-full").locator("img").evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(1);
  await tab.keyboard.press("Escape");
  await expect(tab.getByTestId("image-full")).toBeHidden();
  // Its × takes the path out of the draft.
  await thumb.hover();
  await thumb.getByRole("button", { name: "Remove image" }).click();
  await expect(composer).toHaveValue("look at ");
  await expect(thumb).toHaveCount(0);
  await composer.fill("");
});

test("the tab strip's + asks for a profile before it starts a chat", async ({ page: tab }) => {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE}"]`).click();
  const started: string[] = [];
  tab.on("request", (r) => r.url().endsWith("/api/chat/new") && started.push(r.url()));
  await tab.getByRole("button", { name: "New chat in this workspace" }).click();
  const menu = tab.getByTestId("new-chat-menu");
  await expect(menu.getByRole("menuitem", { name: "Default profile" })).toBeVisible();
  await tab.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  expect(started).toEqual([]);
});
