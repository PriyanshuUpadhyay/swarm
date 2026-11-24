import { expect, test, type Page } from "@playwright/test";
import type { Block } from "../src/core/api";
import { savedNotes, STORE_KEY, type Note } from "../src/core/notes";
import { mockRows, open, row, rows } from "./ux-helpers";

// Chat notes (ADR 0161): the owner marks agent reply text, writes a note in a numbered card, and
// the agent gets every note of the chat as one typed message.

const REPLY = "The page cache keeps each rendered page for 10 minutes, so an edit shows late.\n\nReply with A, B, or C.";

/** The bodies that the composer's send posts, answered as sent. */
async function captureSends(tab: Page) {
  const sent: Record<string, unknown>[] = [];
  await tab.route("**/api/chat/send", (route) => {
    sent.push(route.request().postDataJSON());
    return route.fulfill({ json: { ok: true } });
  });
  return sent;
}

/** Selects `text` inside the first element that matches `selector` and holds it. */
async function select(tab: Page, selector: string, text: string) {
  await tab.locator(selector).filter({ hasText: text }).first().evaluate((el, text) => {
    const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let node = walk.nextNode(); node; node = walk.nextNode()) {
      const at = (node as Text).data.indexOf(text);
      if (at < 0) continue;

      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + text.length);
      const sel = window.getSelection()!;
      sel.removeAllRanges();
      sel.addRange(range);
      return;
    }
    throw new Error(`no text ${text}`);
  }, text);
}

test("a text note from the row menu goes to the agent as one message with its quote", async ({ page: tab }) => {
  await mockRows(tab, [row("a1", REPLY)]);
  const sent = await captureSends(tab);
  await open(tab);
  const reply = '[data-testid="row"][data-id="a1"] .md p';
  await select(tab, reply, "10 minutes");
  await tab.locator(reply).first().click({ button: "right" });
  await tab.getByRole("menuitem", { name: "Add note" }).click();

  const card = tab.getByTestId("note-card");
  await expect(card).toHaveCount(1);
  await expect(card).toContainText("10 minutes");
  await expect(tab.getByTestId("note-input")).toBeFocused();
  await tab.keyboard.type("Why 10 minutes?");
  await expect(tab.getByTestId("notes-bar")).toContainText("1 note on this chat");

  await tab.getByRole("button", { name: "Send 1 note" }).click();
  await expect.poll(() => sent.length).toBe(1);
  expect(sent[0].text).toBe("Notes on your reply (1):\n\n1. > 10 minutes\n   Why 10 minutes?");
  await expect(card).toHaveCount(0);
  await expect(tab.getByTestId("notes-bar")).toBeHidden();
});

const para = (value: string) => ({ type: "paragraph" as const, value: [{ type: "text" as const, value }] });
const list = (ordered: boolean, start: number | null, items: Block[][]): Block => ({ type: "list", value: { ordered, start, tight: true, items } });
const CHOICES = ["A · Notes tray", "B · Quote into the composer", "C · Margin notes"];
/** The heading and the list of choices of a choice reply, as text and as the server renders it. */
const choices = {
  text: `Which design goes to the build?\n\n${CHOICES.map((c) => `- ${c}`).join("\n")}`,
  blocks: [para("Which design goes to the build?"), list(false, null, CHOICES.map((c) => [para(c)]))],
};

/** A reply with a heading, a list of choices and a closing line, as the server renders it. */
async function mockChoices(tab: Page) {
  const text = `${choices.text}\n\nReply with A, B, or C.`;
  const page = rows([row("a1", text)], 0, {}, { a1: [{ f: "text" }] });
  page.display = { a1: { blocks: [...choices.blocks, para("Reply with A, B, or C.")] } };
  page.body = { a1: "tree" };
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: page }));
  return text;
}

const rowTops = (tab: Page) => tab.locator('[data-testid="row"]').evaluateAll((els) => els.map((e) => e.getBoundingClientRect().top));

test("annotate mode takes an item and a whole reply; ⇧⌘A, the palette and Esc switch it; rows stay put", async ({ page: tab }) => {
  const reply = await mockChoices(tab);
  const sent = await captureSends(tab);
  await open(tab);
  const mode = tab.getByTestId("annotate-mode");
  await expect(mode).toHaveAttribute("aria-pressed", "false");
  const off = await rowTops(tab);

  await tab.keyboard.press("Meta+Shift+A");
  await expect(mode).toHaveAttribute("aria-pressed", "true");
  expect(await rowTops(tab)).toEqual(off);
  await tab.keyboard.press("Escape");
  await expect(mode).toHaveAttribute("aria-pressed", "false");
  await tab.keyboard.press("Meta+k");
  await tab.keyboard.type("Annotate Chat");
  await tab.keyboard.press("Enter");
  await expect(mode).toHaveAttribute("aria-pressed", "true");
  expect(await rowTops(tab)).toEqual(off);

  await tab.locator('[data-testid="row"] li').filter({ hasText: "C · Margin notes" }).click();
  await expect(tab.getByTestId("note-input")).toBeFocused();
  await tab.keyboard.type("Go with this one.");
  await expect(tab.locator('[data-testid="row"] li[data-inote="1"]')).toHaveText("C · Margin notes");
  await tab.getByRole("button", { name: "Add a note on the whole reply" }).click();
  await expect(tab.getByTestId("note-card")).toHaveCount(2);
  await expect(mode).toHaveAttribute("aria-pressed", "true");

  await tab.getByRole("button", { name: "Send 2 notes" }).click();
  await expect.poll(() => sent.length).toBe(1);
  expect(sent[0].text).toBe([
    "Notes on your reply (2):",
    "",
    "1. > C · Margin notes",
    "   Go with this one.",
    "",
    "2. > [whole reply]",
    ...reply.split("\n").map((l) => (l ? `   > ${l}` : "   >")),
  ].join("\n"));
});

test("a quote holds only reply text: list numbers from the list's start, no card text", async ({ page: tab }) => {
  const page = rows([row("a1", "0. zero\n1. one\n   - inner\n2. two"), row("a2", "Next reply.")]);
  page.display = { a1: { blocks: [list(true, 0, [[para("zero")], [para("one"), list(false, null, [[para("inner")]])], [para("two")]])] } };
  page.body = { a1: "tree" };
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: page }));
  // Tall enough that the second reply stays mounted below the first one's 5 cards.
  await tab.setViewportSize({ width: 1280, height: 1600 });
  await open(tab);
  await tab.keyboard.press("Meta+Shift+A");
  // A click near an item's top left lands on its own first line, not on a nested list.
  const item = (text: string) => tab.locator('[data-testid="row"] .md li').filter({ hasText: new RegExp(`^${text}`) }).click({ position: { x: 2, y: 4 } });

  await item("zero");
  await item("inner");
  await item("one");
  await item("two");
  await tab.keyboard.press("Escape");

  // From "zero" to the end of `last` (in this reply, or in the next one), then Add note.
  const selectFromZero = async (last: string) => {
    await tab.getByTestId("transcript").evaluate((root, last) => {
      const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      const texts: Text[] = [];
      for (let node = walk.nextNode(); node; node = walk.nextNode()) if (!node.parentElement!.closest(".ncard")) texts.push(node as Text);

      const range = document.createRange();
      range.setStart(texts.find((t) => t.data === "zero")!, 0);
      const end = texts.filter((t) => t.data.endsWith(last)).pop()!;
      range.setEnd(end, end.data.length);
      window.getSelection()!.removeAllRanges();
      window.getSelection()!.addRange(range);
    }, last);
    await tab.locator('[data-testid="row"] .md li').filter({ hasText: /^two/ }).click({ button: "right", position: { x: 2, y: 4 } });
    await tab.getByRole("menuitem", { name: "Add note" }).click();
  };

  await selectFromZero("two");
  await selectFromZero("Next reply.");

  const quotes = tab.locator(".ncard .nquote");
  await expect(quotes).toHaveCount(7);
  const all = await quotes.allTextContents();
  const items = ["0. zero", "1. one\ninner", "2. two", "Next reply.", "inner"];
  expect(all.filter((q) => items.includes(q)).sort()).toEqual(items);
  const selected = all.filter((q) => !items.includes(q));
  expect(selected).toHaveLength(2);
  for (const q of selected) expect(q).toMatch(/^zero[^]*two\s*$/);
  for (const q of selected) expect(q).not.toMatch(/On item|0\. zero/);
});

test("in annotate mode a click on a link in an item only adds a note, also when the add is refused", async ({ page: tab }) => {
  const url = "https://example.com/trace";
  const page = rows([row("a1", `- See [the trace](${url})`)]);
  const link = { type: "link" as const, value: { url, title: null, children: [{ type: "text" as const, value: "the trace" }] } };
  page.display = { a1: { blocks: [list(false, null, [[{ type: "paragraph", value: [{ type: "text", value: "See " }, link] }]])] } };
  page.body = { a1: "tree" };
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: page }));
  await tab.context().route(`${url}*`, (route) => route.fulfill({ body: "trace" }));
  // 199 saved notes on a row not on screen, so the first click adds the 200th and the second hits the cap.
  const saved = Array.from({ length: 199 }, (_, i): Note => ({ id: i + 1, kind: "item", rowId: "gone", quote: "Q", text: "", at: i }));
  await tab.addInitScript(([key, value]) => localStorage.setItem(key, value), [STORE_KEY, savedNotes([[process.env.SWARM_E2E_LIVE!, saved]])]);
  await open(tab);
  await tab.keyboard.press("Meta+Shift+A");
  // A popup shows within milliseconds, so half a second is enough to see none.
  const noPopup = async () => expect(await tab.context().waitForEvent("page", { timeout: 500 }).then(() => true, () => false)).toBe(false);

  const opened = noPopup();
  await tab.getByRole("link", { name: "the trace" }).click();
  await expect(tab.getByTestId("note-card")).toHaveCount(1);
  await expect(tab.getByTestId("note-card")).toContainText("See the trace");
  await opened;

  const refused = noPopup();
  await tab.getByRole("link", { name: "the trace" }).click();
  await expect(tab.getByText("This chat has 200 notes, the most it keeps.")).toBeVisible();
  await expect(tab.getByTestId("note-card")).toHaveCount(1);
  await refused;
});

test("Find's current match stays on reply text, not on a card or the Whole reply handle", async ({ page: tab }) => {
  await mockChoices(tab);
  await open(tab);
  await tab.keyboard.press("Meta+Shift+A");
  await tab.locator('[data-testid="row"] li').filter({ hasText: "A · Notes tray" }).click();
  await expect(tab.locator(".note-under .ncard")).toHaveCount(1);
  // The text of the block that holds the current match.
  const current = () => tab.evaluate(() => {
    const range = [...(CSS.highlights.get("find-current") ?? [])][0] as Range | undefined;
    return range?.startContainer.parentElement?.closest(".ncard, [data-note-reply], li, p")?.textContent ?? null;
  });

  await tab.keyboard.press("Escape");
  await tab.keyboard.press("Meta+f");
  await tab.getByTestId("find-input").fill("notes");
  // Find starts at the newest match, the last one in the reply.
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 2");
  await expect.poll(current).toBe("C · Margin notes");
  await tab.getByTestId("find-input").fill("reply");
  await expect(tab.getByTestId("find-count")).toHaveText("1 of 1");
  await expect.poll(current).toBe("Reply with A, B, or C.");
});

test("a reload brings the cards back with their unfinished note text", async ({ page: tab }) => {
  await mockChoices(tab);
  await open(tab);
  await tab.keyboard.press("Meta+Shift+A");
  await tab.locator('[data-testid="row"] li').filter({ hasText: "B · Quote into the composer" }).click();
  await tab.keyboard.type("Half a thou");
  await tab.reload();
  await tab.waitForFunction(() => window.__swarmReady === true);
  await expect(tab.getByTestId("note-card")).toHaveCount(1);
  await expect(tab.getByTestId("note-card")).toContainText("B · Quote into the composer");
  await expect(tab.getByTestId("note-input")).toHaveValue("Half a thou");
  await expect(tab.locator('[data-testid="row"] li[data-inote="1"]')).toHaveText("B · Quote into the composer");
  await expect(tab.getByTestId("notes-bar")).toContainText("1 note on this chat");
});

/** The worst-case drafts, saved before the page loads: an item note with no text, a text note on
 *  one 300-char URL, and a whole-reply note on a 12k-char reply. */
async function mockWorstCase(tab: Page) {
  const url = `https://example.com/${"a1b2c3d4e5".repeat(28)}`;
  const filler = "The cache keeps each page for ten minutes, so an edit shows late. ".repeat(180);
  const long = `${filler}\n\n\`\`\`\nconst ttl = 600;\n\`\`\`\n\n| Key | TTL |\n| --- | --- |\n| page | 600 |`;

  const page = rows([row("a3", choices.text), row("a2", `See ${url} for the trace.`), row("a1", long)]);
  const t = (value: string) => [{ type: "text" as const, value }];
  page.display = {
    a3: { blocks: choices.blocks },
    a1: { blocks: [
      para(filler),
      { type: "code", value: { label: null, text: "const ttl = 600;" } },
      { type: "table", value: { source: "| Key | TTL |", align: ["none", "none"], head: [t("Key"), t("TTL")], rows: [[t("page"), t("600")]] } },
    ] },
  };
  page.body = { a3: "tree", a1: "tree" };
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: page }));

  const note = (id: number, kind: Note["kind"], rowId: string, quote: string, text: string): Note => ({ id, kind, rowId, quote, text, at: id });
  const chats: [string, Note[]][] = [[process.env.SWARM_E2E_LIVE!, [
    note(1, "item", "a3", "C · Margin notes", ""),
    note(2, "text", "a2", url, "This link is the trace."),
    note(3, "reply", "a1", long, "Zażółć gęślą jaźń 👍"),
  ]]];
  await tab.addInitScript(([key, value]) => localStorage.setItem(key, value), [STORE_KEY, savedNotes(chats)]);
}

const box = (tab: Page, selector: string) => tab.locator(selector).first().evaluate((el) => el.getBoundingClientRect().toJSON() as DOMRect);

test("wide, cards sit in a gutter beside their block; narrow, under it, with no sideways scroll", async ({ page: tab }) => {
  await mockWorstCase(tab);
  await tab.setViewportSize({ width: 1440, height: 900 });
  await open(tab);
  await expect(tab.getByTestId("note-card")).toHaveCount(3);
  await expect(tab.getByTestId("transcript")).toHaveClass(/wide-notes/);
  const item = await box(tab, '[data-testid="row"] li[data-inote="1"]');
  const card = await box(tab, '.note-gutter [data-note="1"]');
  expect(card.left).toBeGreaterThanOrEqual(item.right);
  expect(Math.abs(card.top - item.top)).toBeLessThan(2);

  await tab.setViewportSize({ width: 375, height: 800 });
  await expect(tab.getByTestId("transcript")).not.toHaveClass(/wide-notes/);
  await expect(tab.locator('[data-testid="row"] li[data-inote="1"] + .note-under [data-note="1"]')).toBeVisible();
  const sideways = () => tab.evaluate(() => [document.documentElement, document.querySelector('[data-testid="transcript"]')!].map((el) => el.scrollWidth - el.clientWidth));
  expect(await sideways()).toEqual([0, 0]);
});
